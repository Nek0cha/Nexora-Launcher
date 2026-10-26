//! ゲーム終了後の提案の収集と、ユーザーの判断の反映。

use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use serde::Deserialize;
use ts_rs::TS;

use super::registry::{Category, KeyCodec, field_for_key, key_codec};
use super::store::{self, Membership, Proposal, SyncProfile};
use super::{ALL_CATEGORIES, SyncCtx, find_profile, options_path, read_options, registered_values};
use crate::error::CoreError;

/// 同じ項目に、異なる値を提案しているインスタンスの組。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictGroup {
    pub field_id: String,
    /// (インスタンス ID, 提案の値)
    pub entries: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PendingProposals {
    /// (提案しているインスタンスの ID, 提案)。値が 1 通りの項目は、
    /// 同じ提案が複数あっても 1 件にまとめ、提案した全インスタンスを並べる。
    pub single: Vec<(Vec<String>, Proposal)>,
    pub conflicts: Vec<ConflictGroup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Resolution {
    ApplyToProfile,
    KeepForInstance,
    Ignore,
}

/// ゲーム終了後に、options.txt の変更を提案として記録する。失敗してもパニックも伝播もしない。
pub fn collect_proposals(ctx: &SyncCtx, instance_id: &str) {
    match catch_unwind(AssertUnwindSafe(|| collect(ctx, instance_id))) {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            tracing::warn!(
                instance_id,
                code = err.code(),
                "failed to collect sync proposals: {err}"
            );
        }
        Err(_) => tracing::warn!(instance_id, "collecting sync proposals panicked"),
    }
}

fn collect(ctx: &SyncCtx, instance_id: &str) -> Result<(), CoreError> {
    if store::membership(ctx.db, instance_id)?.is_none() {
        return Ok(());
    }
    // 読めない・無いときは何もしない（既存の提案を残す）
    let Some(file) = read_options(&options_path(ctx, instance_id)?)? else {
        return Ok(());
    };
    let current = registered_values(&file, &ALL_CATEGORIES);

    // 読み出しから書き込みまでを 1 つのトランザクションにして、同時の解決を失わない
    store::update_snapshot(ctx.db, instance_id, |snapshot| {
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let mut merged: BTreeMap<String, String> = BTreeMap::new();
        // 既存の提案は残す。適用が値を共通設定に戻した後でも、ユーザーが解決するまで消えない
        // （意味の無くなったものは `pending` が隠し、ユーザーは無視できる）
        for p in &snapshot.proposals {
            if is_syncable_proposal(&p.field_id, &p.value) {
                merged.insert(p.field_id.clone(), p.value.clone());
            }
        }
        // 基準値にある項目だけを比べる。今回計算した値が古い提案に勝つ
        for (field_id, base) in &snapshot.baseline {
            let Some(now) = current.get(field_id) else {
                continue;
            };
            if now == base || !is_syncable_proposal(field_id, now) {
                continue;
            }
            // キー割り当ては、基準値も `key.keyboard.*` の書式のときだけ比べる
            if field_for_key(field_id).is_some_and(|f| f.category == Category::Keys)
                && key_codec(base) != KeyCodec::Keyboard
            {
                continue;
            }
            merged.insert(field_id.clone(), now.clone());
        }
        let proposals: Vec<Proposal> = merged
            .into_iter()
            .map(|(field_id, value)| Proposal { field_id, value })
            .collect();
        if proposals == snapshot.proposals {
            return Ok(None);
        }
        // 提案の収集は「最後に適用した時刻」を動かさない
        Ok(Some(store::SnapshotEdit {
            proposals,
            ..store::SnapshotEdit::of(&snapshot, false)
        }))
    })
}

/// 提案にできる項目と値か。登録表に無い項目と、`key.keyboard.*` 以外のキー割り当ては対象外。
fn is_syncable_proposal(field_id: &str, value: &str) -> bool {
    let Some(field) = field_for_key(field_id) else {
        return false;
    };
    field.category != Category::Keys || key_codec(value) == KeyCodec::Keyboard
}

/// 提案が今も意味を持つか。インスタンスの実効値（上書きがあれば上書き、無ければ共通設定の値）と
/// 同じ、除外された項目、無効なカテゴリは意味が無い。
fn is_live(
    profile: &SyncProfile,
    values: &BTreeMap<String, String>,
    membership: &Membership,
    p: &Proposal,
) -> bool {
    let Some(field) = field_for_key(&p.field_id) else {
        return false;
    };
    let effective = membership
        .overrides
        .get(&p.field_id)
        .or_else(|| values.get(&p.field_id));
    profile.categories.contains(&field.category)
        && !membership.excluded.contains(&p.field_id)
        && effective != Some(&p.value)
}

/// 共通設定に参加している全インスタンスの、未解決の提案。
pub fn pending(ctx: &SyncCtx, profile_id: &str) -> Result<PendingProposals, CoreError> {
    let profile = find_profile(ctx, profile_id)?;
    let values = store::values(ctx.db, profile_id)?;
    // field_id -> [(instance_id, value)]
    let mut by_field: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for instance_id in store::members(ctx.db, profile_id)? {
        let Some(membership) = store::membership(ctx.db, &instance_id)? else {
            continue;
        };
        let Some(snapshot) = store::snapshot(ctx.db, &instance_id)? else {
            continue;
        };
        for p in snapshot.proposals {
            if is_live(&profile, &values, &membership, &p) {
                by_field
                    .entry(p.field_id)
                    .or_default()
                    .push((instance_id.clone(), p.value));
            }
        }
    }
    let mut out = PendingProposals::default();
    for (field_id, entries) in by_field {
        let distinct: BTreeSet<&String> = entries.iter().map(|(_, v)| v).collect();
        if distinct.len() > 1 {
            out.conflicts.push(ConflictGroup { field_id, entries });
            continue;
        }
        let Some(value) = entries.first().map(|(_, v)| v.clone()) else {
            continue;
        };
        let ids = entries.into_iter().map(|(id, _)| id).collect();
        out.single.push((ids, Proposal { field_id, value }));
    }
    Ok(out)
}

impl PendingProposals {
    /// 指定のインスタンスに関わる件数。そのインスタンスの単独の提案と、
    /// そのインスタンスが加わっている競合（1 項目を 1 件）を数える。
    pub fn count_for_instance(&self, instance_id: &str) -> usize {
        self.single
            .iter()
            .filter(|(ids, _)| ids.iter().any(|id| id == instance_id))
            .count()
            + self
                .conflicts
                .iter()
                .filter(|c| c.entries.iter().any(|(id, _)| id == instance_id))
                .count()
    }
}

/// 提案 1 件へのユーザーの判断を反映する。どの場合も、そのインスタンスのその項目の提案は消す。
pub fn resolve(
    ctx: &SyncCtx,
    instance_id: &str,
    field_id: &str,
    resolution: Resolution,
    chosen_value: Option<String>,
) -> Result<(), CoreError> {
    let membership = store::membership(ctx.db, instance_id)?.ok_or(CoreError::SyncNotMember)?;
    let snapshot = store::snapshot(ctx.db, instance_id)?.ok_or(CoreError::SyncNoProposal)?;
    let proposal = snapshot
        .proposals
        .iter()
        .find(|p| p.field_id == field_id)
        .ok_or(CoreError::SyncNoProposal)?;

    match resolution {
        Resolution::ApplyToProfile => {
            let profile = find_profile(ctx, &membership.profile_id)?;
            let current = store::values(ctx.db, &membership.profile_id)?;
            // もう意味の無い提案は、共通設定を変えずに消すだけ
            if !is_live(&profile, &current, &membership, proposal) {
                return clear_proposal(ctx, instance_id, field_id, None);
            }
            let all = pending(ctx, &membership.profile_id)?;
            let value = match all.conflicts.iter().find(|c| c.field_id == field_id) {
                Some(group) => {
                    let chosen = chosen_value.ok_or(CoreError::SyncNeedsChosenValue)?;
                    if !group.entries.iter().any(|(_, v)| *v == chosen) {
                        return Err(CoreError::SyncNeedsChosenValue);
                    }
                    chosen
                }
                None => proposal.value.clone(),
            };
            store::set_values(
                ctx.db,
                &membership.profile_id,
                &[(field_id.to_string(), value)],
            )?;
            // 共通設定に反映したので、このインスタンスの上書きは外す（次の適用で共通の値になる）
            store::set_override(ctx.db, instance_id, field_id, None)?;
        }
        Resolution::KeepForInstance => {
            store::set_override(ctx.db, instance_id, field_id, Some(&proposal.value))?;
        }
        Resolution::Ignore => {}
    }
    // 判断した値を基準値にして、同じ値が次回また提案されないようにする
    let accepted = proposal.value.clone();
    clear_proposal(ctx, instance_id, field_id, Some(accepted))
}

/// そのインスタンスの項目の提案を消す。`baseline_value` があれば基準値もその値にする。
/// 読み出しと書き込みは 1 つのトランザクション。適用済みの revision、状態、
/// 「最後に適用した時刻」、他の提案は変えない。
fn clear_proposal(
    ctx: &SyncCtx,
    instance_id: &str,
    field_id: &str,
    baseline_value: Option<String>,
) -> Result<(), CoreError> {
    store::update_snapshot(ctx.db, instance_id, |snapshot| {
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let mut edit = store::SnapshotEdit::of(&snapshot, false);
        edit.proposals.retain(|p| p.field_id != field_id);
        if let Some(value) = baseline_value {
            edit.baseline.insert(field_id.to_string(), value);
        }
        Ok(Some(edit))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::instance::{self, Instance, NewInstance};
    use crate::meta::mojang::manifest::client_jar_path;
    use crate::paths::AppPaths;
    use crate::sync::apply::{ApplyOutcome, apply_before_launch};
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    struct Env {
        _t: tempfile::TempDir,
        paths: AppPaths,
        db: Db,
    }

    impl Env {
        fn new() -> Env {
            let t = tempfile::tempdir().unwrap();
            let paths = AppPaths::from_root(t.path().join("data"));
            paths.ensure_dirs().unwrap();
            let env = Env {
                _t: t,
                paths,
                db: Db::open_in_memory().unwrap(),
            };
            let path = client_jar_path(&env.paths, "1.21.5");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
            zip.start_file("version.json", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"{\"world_version\":4325}").unwrap();
            zip.finish().unwrap();
            env
        }

        fn ctx(&self) -> SyncCtx<'_> {
            SyncCtx {
                db: &self.db,
                paths: &self.paths,
            }
        }

        fn instance(&self, name: &str, options: &str) -> Instance {
            let inst = instance::create(
                &self.db,
                &self.paths,
                &NewInstance::vanilla(name, "1.21.5"),
                1,
            )
            .unwrap();
            std::fs::write(self.options(&inst), options).unwrap();
            inst
        }

        fn options(&self, inst: &Instance) -> std::path::PathBuf {
            instance::game_dir(&self.paths, inst).join("options.txt")
        }

        fn profile(&self, values: &[(&str, &str)]) -> SyncProfile {
            let p = store::create_profile(&self.db, "P", &ALL_CATEGORIES).unwrap();
            let values: Vec<(String, String)> = values
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            store::set_values(&self.db, &p.id, &values).unwrap();
            p
        }

        /// 参加させ、起動前の適用で基準値を作る。
        fn member(&self, inst: &Instance, p: &SyncProfile) {
            store::join(&self.db, &inst.id, &p.id).unwrap();
            apply_before_launch(&self.ctx(), inst);
        }

        fn proposals(&self, inst: &Instance) -> Vec<(String, String)> {
            store::snapshot(&self.db, &inst.id)
                .unwrap()
                .unwrap()
                .proposals
                .into_iter()
                .map(|p| (p.field_id, p.value))
                .collect()
        }

        fn revision(&self, p: &SyncProfile) -> i64 {
            store::list_profiles(&self.db)
                .unwrap()
                .into_iter()
                .find(|x| x.id == p.id)
                .unwrap()
                .revision
        }
    }

    const BASE: &str = "version:4325\nfov:0.5\nmouseSensitivity:0.3\nkey_key.jump:key.keyboard.space\nlastServer:x\n";

    fn pair(a: &str, b: &str) -> (String, String) {
        (a.to_string(), b.to_string())
    }

    #[test]
    fn collects_only_changed_registered_fields() {
        let env = Env::new();
        let inst = env.instance("A", BASE);
        let p = env.profile(&[("fov", "0.5"), ("mouseSensitivity", "0.3")]);
        env.member(&inst, &p);
        std::fs::write(
            env.options(&inst),
            "version:4325\nfov:0.9\nmouseSensitivity:0.3\nkey_key.jump:key.keyboard.space\nlastServer:y\n",
        )
        .unwrap();
        collect_proposals(&env.ctx(), &inst.id);
        assert_eq!(env.proposals(&inst), vec![pair("fov", "0.9")]);
    }

    #[test]
    fn ignores_keys_with_unknown_codec() {
        let env = Env::new();
        let inst = env.instance("A", BASE);
        let p = env.profile(&[("key_key.jump", "key.keyboard.space")]);
        env.member(&inst, &p);
        // 不明な書式、旧式の整数コード（書式が違う）は対象外
        for changed in ["scancode.4", "32"] {
            std::fs::write(
                env.options(&inst),
                format!("version:4325\nkey_key.jump:{changed}\n"),
            )
            .unwrap();
            collect_proposals(&env.ctx(), &inst.id);
            assert!(env.proposals(&inst).is_empty(), "{changed}");
        }
        std::fs::write(
            env.options(&inst),
            "version:4325\nkey_key.jump:key.keyboard.w\n",
        )
        .unwrap();
        collect_proposals(&env.ctx(), &inst.id);
        assert_eq!(
            env.proposals(&inst),
            vec![pair("key_key.jump", "key.keyboard.w")]
        );
    }

    #[test]
    fn no_baseline_means_no_proposals() {
        let env = Env::new();
        let inst = env.instance("A", BASE);
        let p = env.profile(&[("fov", "0.5")]);
        // 参加はしたが一度も適用していない（記録なし）
        store::join(&env.db, &inst.id, &p.id).unwrap();
        collect_proposals(&env.ctx(), &inst.id);
        assert!(store::snapshot(&env.db, &inst.id).unwrap().is_none());
        // 基準値に無い項目は提案にならない
        env.member(&inst, &p);
        std::fs::write(env.options(&inst), "version:4325\nfov:0.5\ngamma:1.0\n").unwrap();
        collect_proposals(&env.ctx(), &inst.id);
        assert!(env.proposals(&inst).is_empty());
        // 参加していないインスタンスも何もしない
        let other = env.instance("B", BASE);
        collect_proposals(&env.ctx(), &other.id);
    }

    #[test]
    fn missing_options_keeps_existing_proposals() {
        let env = Env::new();
        let inst = env.instance("A", BASE);
        let p = env.profile(&[("fov", "0.5")]);
        env.member(&inst, &p);
        std::fs::write(env.options(&inst), "version:4325\nfov:0.9\n").unwrap();
        collect_proposals(&env.ctx(), &inst.id);
        std::fs::remove_file(env.options(&inst)).unwrap();
        collect_proposals(&env.ctx(), &inst.id);
        assert_eq!(env.proposals(&inst), vec![pair("fov", "0.9")]);
        // UTF-8 でないときも残す
        std::fs::write(env.options(&inst), b"fov:0.1\nx:\xff\n").unwrap();
        collect_proposals(&env.ctx(), &inst.id);
        assert_eq!(env.proposals(&inst), vec![pair("fov", "0.9")]);
    }

    #[test]
    fn collect_merges_with_stored_proposals() {
        let env = Env::new();
        let inst = env.instance("A", BASE);
        let p = env.profile(&[("fov", "0.5"), ("mouseSensitivity", "0.3")]);
        env.member(&inst, &p);
        std::fs::write(
            env.options(&inst),
            "version:4325\nfov:0.9\nmouseSensitivity:0.7\n",
        )
        .unwrap();
        collect_proposals(&env.ctx(), &inst.id);
        // fov は新しい値が勝つ。mouseSensitivity は基準値に戻っても、解決するまで残る
        std::fs::write(
            env.options(&inst),
            "version:4325\nfov:1.1\nmouseSensitivity:0.3\n",
        )
        .unwrap();
        collect_proposals(&env.ctx(), &inst.id);
        let both = vec![pair("fov", "1.1"), pair("mouseSensitivity", "0.7")];
        assert_eq!(env.proposals(&inst), both);
        // 行が消えた項目の提案は残る
        std::fs::write(env.options(&inst), "version:4325\nmouseSensitivity:0.3\n").unwrap();
        collect_proposals(&env.ctx(), &inst.id);
        assert_eq!(env.proposals(&inst), both);
    }

    fn two_members(env: &Env, a_fov: &str, b_fov: &str) -> (SyncProfile, Instance, Instance) {
        let a = env.instance("A", BASE);
        let b = env.instance("B", BASE);
        let p = env.profile(&[("fov", "0.5"), ("mouseSensitivity", "0.3")]);
        env.member(&a, &p);
        env.member(&b, &p);
        for (inst, fov) in [(&a, a_fov), (&b, b_fov)] {
            std::fs::write(env.options(inst), format!("version:4325\nfov:{fov}\n")).unwrap();
            collect_proposals(&env.ctx(), &inst.id);
        }
        (p, a, b)
    }

    #[test]
    fn pending_separates_conflicts_from_single_proposals() {
        let env = Env::new();
        let (p, a, b) = two_members(&env, "0.9", "0.8");
        // B だけが mouseSensitivity も提案する
        std::fs::write(
            env.options(&b),
            "version:4325\nfov:0.8\nmouseSensitivity:0.6\n",
        )
        .unwrap();
        collect_proposals(&env.ctx(), &b.id);
        let got = pending(&env.ctx(), &p.id).unwrap();
        assert_eq!(
            got.single,
            vec![(
                vec![b.id.clone()],
                Proposal {
                    field_id: "mouseSensitivity".into(),
                    value: "0.6".into()
                }
            )]
        );
        assert_eq!(got.conflicts.len(), 1);
        assert_eq!(got.conflicts[0].field_id, "fov");
        assert_eq!(
            got.conflicts[0].entries,
            vec![
                (a.id.clone(), "0.9".to_string()),
                (b.id.clone(), "0.8".to_string())
            ]
        );
    }

    #[test]
    fn identical_proposals_from_two_instances_are_not_a_conflict() {
        let env = Env::new();
        let (p, _a, _b) = two_members(&env, "0.9", "0.9");
        let got = pending(&env.ctx(), &p.id).unwrap();
        assert!(got.conflicts.is_empty());
        assert_eq!(got.single.len(), 1);
        assert_eq!(got.single[0].1.value, "0.9");
    }

    #[test]
    fn single_proposal_merges_instance_ids() {
        let env = Env::new();
        let (p, a, b) = two_members(&env, "0.9", "0.9");
        let got = pending(&env.ctx(), &p.id).unwrap();
        let mut ids = vec![a.id.clone(), b.id.clone()];
        ids.sort();
        assert_eq!(
            got.single,
            vec![(
                ids.clone(),
                Proposal {
                    field_id: "fov".into(),
                    value: "0.9".into()
                }
            )]
        );
        assert_eq!(got.count_for_instance(&a.id), 1);
        assert_eq!(got.count_for_instance(&b.id), 1);
        let view = crate::sync::wire::PendingView::from(got);
        assert_eq!(view.single[0].instance_ids, ids);
        // 解決は指定した 1 つだけ。もう 1 つの提案は残る
        resolve(&env.ctx(), &a.id, "fov", Resolution::Ignore, None).unwrap();
        let got = pending(&env.ctx(), &p.id).unwrap();
        assert_eq!(got.single.len(), 1);
        assert_eq!(got.single[0].0, vec![b.id.clone()]);
    }

    #[test]
    fn pending_hides_obsolete_excluded_and_disabled() {
        let env = Env::new();
        let (p, a, b) = two_members(&env, "0.9", "0.8");
        // 共通設定の値が提案と同じになった側は出ない
        store::set_values(&env.db, &p.id, &[("fov".into(), "0.9".into())]).unwrap();
        let got = pending(&env.ctx(), &p.id).unwrap();
        assert!(got.conflicts.is_empty());
        assert_eq!(got.single.len(), 1);
        assert_eq!(got.single[0].0, vec![b.id.clone()]);
        store::set_excluded(&env.db, &b.id, &["fov".to_string()]).unwrap();
        assert_eq!(
            pending(&env.ctx(), &p.id).unwrap(),
            PendingProposals::default()
        );
        store::set_excluded(&env.db, &b.id, &[]).unwrap();
        store::set_categories(&env.db, &p.id, &[Category::Sound]).unwrap();
        assert_eq!(
            pending(&env.ctx(), &p.id).unwrap(),
            PendingProposals::default()
        );
        let _ = a;
    }

    #[test]
    fn resolve_apply_updates_profile_and_clears_proposal() {
        let env = Env::new();
        let (p, a, b) = two_members(&env, "0.9", "0.9");
        let before = env.revision(&p);
        resolve(&env.ctx(), &a.id, "fov", Resolution::ApplyToProfile, None).unwrap();
        assert_eq!(store::values(&env.db, &p.id).unwrap()["fov"], "0.9");
        assert_eq!(env.revision(&p), before + 1);
        assert!(env.proposals(&a).is_empty());
        // B の提案は残るが、値が同じになったので pending には出ない
        assert_eq!(env.proposals(&b).len(), 1);
        assert_eq!(
            pending(&env.ctx(), &p.id).unwrap(),
            PendingProposals::default()
        );
    }

    #[test]
    fn resolve_apply_on_conflict_requires_a_proposed_value() {
        let env = Env::new();
        let (p, a, b) = two_members(&env, "0.9", "0.8");
        let ctx = env.ctx();
        assert!(resolve(&ctx, &a.id, "fov", Resolution::ApplyToProfile, None).is_err());
        assert!(
            resolve(
                &ctx,
                &a.id,
                "fov",
                Resolution::ApplyToProfile,
                Some("0.1".into())
            )
            .is_err()
        );
        assert_eq!(store::values(&env.db, &p.id).unwrap()["fov"], "0.5");
        assert_eq!(env.proposals(&a).len(), 1);
        resolve(
            &ctx,
            &a.id,
            "fov",
            Resolution::ApplyToProfile,
            Some("0.8".into()),
        )
        .unwrap();
        assert_eq!(store::values(&env.db, &p.id).unwrap()["fov"], "0.8");
        assert!(env.proposals(&a).is_empty());
        assert_eq!(env.proposals(&b).len(), 1);
        // 提案が無い項目はエラー
        assert!(resolve(&ctx, &a.id, "fov", Resolution::Ignore, None).is_err());
    }

    #[test]
    fn resolve_keep_records_override() {
        let env = Env::new();
        let (p, a, _b) = two_members(&env, "0.9", "0.8");
        resolve(&env.ctx(), &a.id, "fov", Resolution::KeepForInstance, None).unwrap();
        let m = store::membership(&env.db, &a.id).unwrap().unwrap();
        assert_eq!(m.overrides.get("fov").map(String::as_str), Some("0.9"));
        assert!(env.proposals(&a).is_empty());
        assert_eq!(store::values(&env.db, &p.id).unwrap()["fov"], "0.5");
    }

    #[test]
    fn resolve_ignore_only_clears_proposal() {
        let env = Env::new();
        let (p, a, _b) = two_members(&env, "0.9", "0.8");
        let before = env.revision(&p);
        resolve(&env.ctx(), &a.id, "fov", Resolution::Ignore, None).unwrap();
        assert!(env.proposals(&a).is_empty());
        assert_eq!(env.revision(&p), before);
        assert!(
            store::membership(&env.db, &a.id)
                .unwrap()
                .unwrap()
                .overrides
                .is_empty()
        );
    }

    fn set_fov(env: &Env, inst: &Instance, fov: &str) {
        std::fs::write(
            env.options(inst),
            format!(
                "version:4325
fov:{fov}
"
            ),
        )
        .unwrap();
        collect_proposals(&env.ctx(), &inst.id);
    }

    #[test]
    fn keep_for_instance_is_sticky() {
        let env = Env::new();
        let (_p, a, _b) = two_members(&env, "0.9", "0.8");
        resolve(&env.ctx(), &a.id, "fov", Resolution::KeepForInstance, None).unwrap();
        collect_proposals(&env.ctx(), &a.id);
        assert!(env.proposals(&a).is_empty());
    }

    #[test]
    fn ignore_is_sticky_but_a_new_value_proposes_again() {
        let env = Env::new();
        let (p, a, _b) = two_members(&env, "0.9", "0.8");
        let snap = store::snapshot(&env.db, &a.id).unwrap().unwrap();
        resolve(&env.ctx(), &a.id, "fov", Resolution::Ignore, None).unwrap();
        collect_proposals(&env.ctx(), &a.id);
        assert!(env.proposals(&a).is_empty());
        let after = store::snapshot(&env.db, &a.id).unwrap().unwrap();
        assert_eq!(after.applied_revision, snap.applied_revision);
        assert_eq!(after.state, snap.state);
        set_fov(&env, &a, "0.2");
        assert_eq!(env.proposals(&a), vec![pair("fov", "0.2")]);
        let _ = p;
    }

    #[test]
    fn apply_to_profile_on_a_stale_proposal_changes_nothing() {
        let env = Env::new();
        let (p, a, _b) = two_members(&env, "0.9", "0.8");
        store::set_values(&env.db, &p.id, &[("fov".into(), "0.9".into())]).unwrap();
        let before = env.revision(&p);
        resolve(&env.ctx(), &a.id, "fov", Resolution::ApplyToProfile, None).unwrap();
        assert_eq!(env.revision(&p), before);
        assert!(env.proposals(&a).is_empty());
    }

    #[test]
    fn profile_change_during_play_is_applied_next_launch() {
        let env = Env::new();
        let (p, a, _b) = two_members(&env, "0.9", "0.9");
        let applied = store::snapshot(&env.db, &a.id)
            .unwrap()
            .unwrap()
            .applied_revision;
        resolve(&env.ctx(), &a.id, "fov", Resolution::ApplyToProfile, None).unwrap();
        let revision = env.revision(&p);
        assert!(revision > applied);
        let outcome = apply_before_launch(&env.ctx(), &a);
        assert!(matches!(
            outcome,
            ApplyOutcome::Applied { .. } | ApplyOutcome::UpToDate
        ));
        let snap = store::snapshot(&env.db, &a.id).unwrap().unwrap();
        assert_eq!(snap.applied_revision, revision);
        assert_eq!(snap.baseline["fov"], "0.9");
    }

    #[test]
    fn unresolved_proposal_survives_apply_and_the_next_collect() {
        let env = Env::new();
        let inst = env.instance("A", BASE);
        let p = env.profile(&[("fov", "0.5")]);
        env.member(&inst, &p);
        set_fov(&env, &inst, "0.9");
        assert_eq!(env.proposals(&inst), vec![pair("fov", "0.9")]);
        // 次の起動前の適用が共通の値 0.5 を書き戻す。遊ばずに終了しても提案は消えない
        assert!(matches!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Applied { .. }
        ));
        collect_proposals(&env.ctx(), &inst.id);
        assert_eq!(env.proposals(&inst), vec![pair("fov", "0.9")]);
    }

    #[test]
    fn apply_to_profile_clears_the_instance_override() {
        let env = Env::new();
        let (p, a, _b) = two_members(&env, "0.9", "0.8");
        resolve(&env.ctx(), &a.id, "fov", Resolution::KeepForInstance, None).unwrap();
        // ゲーム内でさらに 1.0 にした。実効値は上書きの 0.9 なので、提案は有効
        set_fov(&env, &a, "1.0");
        assert_eq!(env.proposals(&a), vec![pair("fov", "1.0")]);
        // B も 0.8 を提案していて競合するので、採用する値を選ぶ
        resolve(
            &env.ctx(),
            &a.id,
            "fov",
            Resolution::ApplyToProfile,
            Some("1.0".into()),
        )
        .unwrap();
        assert_eq!(store::values(&env.db, &p.id).unwrap()["fov"], "1.0");
        assert!(
            store::membership(&env.db, &a.id)
                .unwrap()
                .unwrap()
                .overrides
                .is_empty()
        );
        // 上書きが残っていれば 0.9 が書かれるはず。次の適用は共通の値 1.0 になる
        std::fs::write(env.options(&a), "version:4325\nfov:0.7\n").unwrap();
        apply_before_launch(&env.ctx(), &a);
        assert!(
            std::fs::read_to_string(env.options(&a))
                .unwrap()
                .contains("fov:1.0")
        );
    }

    #[test]
    fn liveness_is_judged_against_the_effective_value() {
        let env = Env::new();
        let a = env.instance("A", BASE);
        let p = env.profile(&[("fov", "0.5")]);
        env.member(&a, &p);
        set_fov(&env, &a, "0.9");
        resolve(&env.ctx(), &a.id, "fov", Resolution::KeepForInstance, None).unwrap();
        // 共通の値 0.5 に戻しても、実効値は上書きの 0.9。次の適用で 0.9 に戻るので提案は有効
        set_fov(&env, &a, "0.5");
        let got = pending(&env.ctx(), &p.id).unwrap();
        assert!(
            got.single
                .iter()
                .any(|(ids, pr)| ids.contains(&a.id) && pr.value == "0.5")
        );
        // 上書きと同じ値の提案は有効ではない（共通の値と違っても）
        let membership = store::membership(&env.db, &a.id).unwrap().unwrap();
        let profile = find_profile(&env.ctx(), &p.id).unwrap();
        let values = store::values(&env.db, &p.id).unwrap();
        let same_as_override = Proposal {
            field_id: "fov".into(),
            value: "0.9".into(),
        };
        assert!(!is_live(&profile, &values, &membership, &same_as_override));
    }

    #[test]
    fn lwjgl2_key_values_are_never_proposed() {
        let env = Env::new();
        let inst = env.instance("A", BASE);
        let p = env.profile(&[("key_key.jump", "key.keyboard.space")]);
        env.member(&inst, &p);
        std::fs::write(env.options(&inst), "version:4325\nkey_key.jump:57\n").unwrap();
        collect_proposals(&env.ctx(), &inst.id);
        assert!(env.proposals(&inst).is_empty());
        // 保存済みの旧式の値の提案も、収集で取り除く
        store::update_snapshot(&env.db, &inst.id, |s| {
            let mut edit = store::SnapshotEdit::of(&s.unwrap(), false);
            edit.proposals = vec![Proposal {
                field_id: "key_key.jump".into(),
                value: "57".into(),
            }];
            Ok(Some(edit))
        })
        .unwrap();
        collect_proposals(&env.ctx(), &inst.id);
        assert!(env.proposals(&inst).is_empty());
    }

    #[test]
    fn parallel_resolves_on_different_fields_both_persist() {
        let env = Env::new();
        let inst = env.instance("A", BASE);
        let p = env.profile(&[("fov", "0.5"), ("mouseSensitivity", "0.3")]);
        env.member(&inst, &p);
        std::fs::write(
            env.options(&inst),
            "version:4325\nfov:0.9\nmouseSensitivity:0.7\n",
        )
        .unwrap();
        collect_proposals(&env.ctx(), &inst.id);
        assert_eq!(env.proposals(&inst).len(), 2);
        std::thread::scope(|scope| {
            for (field, resolution) in [
                ("fov", Resolution::KeepForInstance),
                ("mouseSensitivity", Resolution::Ignore),
            ] {
                let (env, id) = (&env, inst.id.clone());
                scope.spawn(move || {
                    resolve(&env.ctx(), &id, field, resolution, None).unwrap();
                });
            }
        });
        let snap = store::snapshot(&env.db, &inst.id).unwrap().unwrap();
        assert!(snap.proposals.is_empty());
        assert_eq!(snap.baseline["fov"], "0.9");
        assert_eq!(snap.baseline["mouseSensitivity"], "0.7");
    }

    #[test]
    fn resolving_and_collecting_do_not_move_last_applied() {
        let env = Env::new();
        let (_p, a, _b) = two_members(&env, "0.9", "0.8");
        let stamp = 12_345;
        env.db
            .with_conn(|conn| {
                conn.execute("UPDATE sync_snapshots SET updated_at = ?1", [stamp])?;
                Ok(())
            })
            .unwrap();
        set_fov(&env, &a, "0.95");
        resolve(&env.ctx(), &a.id, "fov", Resolution::KeepForInstance, None).unwrap();
        let snap = store::snapshot(&env.db, &a.id).unwrap().unwrap();
        assert_eq!(snap.updated_at, Some(stamp));
    }

    #[test]
    fn resolve_errors_have_specific_codes() {
        let env = Env::new();
        let (_p, a, _b) = two_members(&env, "0.9", "0.8");
        let code = |r: Result<(), CoreError>| r.unwrap_err().code();
        let ctx = env.ctx();
        assert_eq!(
            code(resolve(&ctx, "ghost", "fov", Resolution::Ignore, None)),
            "sync.not_member"
        );
        assert_eq!(
            code(resolve(&ctx, &a.id, "gamma", Resolution::Ignore, None)),
            "sync.no_proposal"
        );
        assert_eq!(
            code(resolve(
                &ctx,
                &a.id,
                "fov",
                Resolution::ApplyToProfile,
                None
            )),
            "sync.needs_chosen_value"
        );
    }

    #[test]
    fn pending_count_is_per_instance() {
        let env = Env::new();
        let (p, a, b) = two_members(&env, "0.9", "0.8");
        std::fs::write(
            env.options(&b),
            "version:4325\nfov:0.8\nmouseSensitivity:0.6\n",
        )
        .unwrap();
        collect_proposals(&env.ctx(), &b.id);
        let got = pending(&env.ctx(), &p.id).unwrap();
        // fov は A と B の競合、mouseSensitivity は B だけの提案
        assert_eq!(got.count_for_instance(&a.id), 1);
        assert_eq!(got.count_for_instance(&b.id), 2);
        assert_eq!(got.count_for_instance("other"), 0);
    }
}
