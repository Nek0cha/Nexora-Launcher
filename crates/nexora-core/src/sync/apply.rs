//! 起動前の適用。参加中のインスタンスの options.txt に、共通設定の値を書き込む。
//! この処理はゲームの起動を止めない。失敗は `ApplyOutcome::Failed` にして返す。

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use super::SyncCtx;
use super::options::{OptionsFile, write_atomic};
use super::registry::{Category, KeyCodec, field_for_key, key_codec};
use super::store::{self, Snapshot, SnapshotState};
use super::version::read_data_version;
use crate::instance::{self, Instance};
use crate::meta::mojang::manifest::client_jar_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// 共通設定に参加していない。
    NotMember,
    /// options.txt を書き換えた。`changed` は値が変わった行の数。
    Applied {
        changed: usize,
    },
    /// 適用する値はあるが、すべてインスタンスの値と同じ。ファイルには触れない。
    UpToDate,
    Skipped(SkipReason),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    NoOptionsFile,
    NoDataVersion,
    VersionMismatch,
    NothingToApply,
}

/// 起動前に共通設定を適用する。`Result` を返さず、パニックもしない。
pub fn apply_before_launch(ctx: &SyncCtx, inst: &Instance) -> ApplyOutcome {
    match catch_unwind(AssertUnwindSafe(|| run(ctx, inst))) {
        Ok(outcome) => outcome,
        Err(_) => ApplyOutcome::Failed("sync apply panicked".to_string()),
    }
}

fn run(ctx: &SyncCtx, inst: &Instance) -> ApplyOutcome {
    let membership = match store::membership(ctx.db, &inst.id) {
        Ok(Some(m)) => m,
        Ok(None) => return ApplyOutcome::NotMember,
        Err(e) => return ApplyOutcome::Failed(format!("failed to read membership: {e}")),
    };
    match apply_member(ctx, inst, &membership) {
        Ok(outcome) => outcome,
        Err(message) => {
            record_state(ctx, &inst.id, SnapshotState::Failed);
            ApplyOutcome::Failed(message)
        }
    }
}

/// 適用の本体。ここでの `Err` は `Failed` のメッセージになる。
fn apply_member(
    ctx: &SyncCtx,
    inst: &Instance,
    membership: &store::Membership,
) -> Result<ApplyOutcome, String> {
    let path = instance::game_dir(ctx.paths, inst).join("options.txt");
    // UTF-8 として厳密に読む。読めなければ何も書かない
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(skipped(ctx, &inst.id, SkipReason::NoOptionsFile));
        }
        Err(e) => return Err(format!("failed to read {}: {e}", path.display())),
    };
    let text =
        String::from_utf8(bytes).map_err(|_| format!("{} is not valid UTF-8", path.display()))?;
    let mut file = OptionsFile::parse(&text);

    let Some(data_version) = read_data_version(&client_jar_path(ctx.paths, &inst.mc_version))
    else {
        return Ok(skipped(ctx, &inst.id, SkipReason::NoDataVersion));
    };
    if file.version() != Some(data_version) {
        return Ok(skipped(ctx, &inst.id, SkipReason::VersionMismatch));
    }

    let profile = store::list_profiles(ctx.db)
        .map_err(|e| format!("failed to read profiles: {e}"))?
        .into_iter()
        .find(|p| p.id == membership.profile_id)
        .ok_or_else(|| format!("sync profile {} was not found", membership.profile_id))?;
    let profile_values = store::values(ctx.db, &profile.id)
        .map_err(|e| format!("failed to read profile values: {e}"))?;

    // 管理対象 = 有効なカテゴリで、除外されておらず、インスタンスに行がある項目。
    // (field_id, options.txt のキー, 適用する値, 現在値)
    let mut managed: Vec<(String, &'static str, String, String)> = Vec::new();
    for (id, profile_value) in &profile_values {
        let Some(field) = field_for_key(id) else {
            continue;
        };
        if !profile.categories.contains(&field.category) || membership.excluded.contains(id) {
            continue;
        }
        // インスタンスの例外が共通設定の値より優先
        let value = membership.overrides.get(id).unwrap_or(profile_value);
        // 改行を含む値は行を壊すので書かない
        if value.contains(['\n', '\r']) {
            continue;
        }
        // 行が無い項目は足さない
        let Some(current) = file.get(field.key) else {
            continue;
        };
        // キー割り当ては、書式が同じで判定できるものだけ
        if field.category == Category::Keys {
            let codec = key_codec(current);
            if codec == KeyCodec::Unknown || codec != key_codec(value) {
                continue;
            }
        }
        managed.push((id.clone(), field.key, value.clone(), current.to_string()));
    }
    if managed.is_empty() {
        return Ok(skipped(ctx, &inst.id, SkipReason::NothingToApply));
    }

    let updates: Vec<(&str, String)> = managed
        .iter()
        .filter(|(_, _, value, current)| value != current)
        .map(|(_, key, value, _)| (*key, value.clone()))
        .collect();
    // 適用後に管理対象の項目が持つ値
    let baseline: BTreeMap<String, String> = managed
        .iter()
        .map(|(id, _, value, _)| (id.clone(), value.clone()))
        .collect();

    if updates.is_empty() {
        // ファイルには触れない。記録が無い、または古いときだけ基準値を残す
        let stale = match store::snapshot(ctx.db, &inst.id) {
            Ok(Some(s)) => !is_current(&s, profile.revision),
            Ok(None) => true,
            Err(_) => false,
        };
        if stale
            && let Err(e) = save_keeping_proposals(
                ctx.db,
                &inst.id,
                profile.revision,
                &baseline,
                SnapshotState::Applied,
            )
        {
            tracing::warn!(instance_id = %inst.id, "failed to save the sync snapshot: {e}");
        }
        return Ok(ApplyOutcome::UpToDate);
    }

    file.set_all(&updates);
    write_atomic(&path, &ctx.paths.backups_dir(), &inst.id, &file.render())
        .map_err(|e| format!("failed to write options.txt: {e}"))?;
    save_keeping_proposals(
        ctx.db,
        &inst.id,
        profile.revision,
        &baseline,
        SnapshotState::Applied,
    )
    .map_err(|e| format!("options.txt was updated but the sync record failed: {e}"))?;
    Ok(ApplyOutcome::Applied {
        changed: updates.len(),
    })
}

fn is_current(snapshot: &Snapshot, revision: i64) -> bool {
    snapshot.state == SnapshotState::Applied && snapshot.applied_revision == revision
}

fn skipped(ctx: &SyncCtx, instance_id: &str, reason: SkipReason) -> ApplyOutcome {
    record_state(ctx, instance_id, SnapshotState::Skipped);
    ApplyOutcome::Skipped(reason)
}

/// 記録を保存する。`save_snapshot` は提案を空に戻すので、保存前の提案を戻す。
/// 未処理の提案は、どの経路でも適用では消さない（古くなったものは後の処理で除く）。
fn save_keeping_proposals(
    db: &crate::db::Db,
    instance_id: &str,
    applied_revision: i64,
    baseline: &BTreeMap<String, String>,
    state: SnapshotState,
) -> Result<(), crate::error::CoreError> {
    let proposals = store::snapshot(db, instance_id)?
        .map(|s| s.proposals)
        .unwrap_or_default();
    store::save_snapshot(db, instance_id, applied_revision, baseline, state)?;
    if !proposals.is_empty() {
        store::set_proposals(db, instance_id, &proposals)?;
    }
    Ok(())
}

/// 状態だけを記録する。既にある基準値、revision、提案は消さない。
/// 記録がまだ無いときは、空の基準値と revision 0 で作る。
fn record_state(ctx: &SyncCtx, instance_id: &str, state: SnapshotState) {
    let result = (|| {
        let existing = store::snapshot(ctx.db, instance_id)?;
        let (revision, baseline) = match existing {
            Some(s) => (s.applied_revision, s.baseline),
            None => (0, BTreeMap::new()),
        };
        save_keeping_proposals(ctx.db, instance_id, revision, &baseline, state)
    })();
    if let Err(e) = result {
        tracing::warn!(instance_id, "failed to record the sync state: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::instance::NewInstance;
    use crate::paths::AppPaths;
    use crate::sync::registry::Category as C;
    use std::io::Write;
    use std::path::PathBuf;
    use zip::write::SimpleFileOptions;

    const VERSION: i64 = 4325;
    const ALL: [Category; 6] = [
        C::Keys,
        C::Mouse,
        C::Sound,
        C::Graphics,
        C::Chat,
        C::Accessibility,
    ];

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
            Env {
                _t: t,
                paths,
                db: Db::open_in_memory().unwrap(),
            }
        }

        fn ctx(&self) -> SyncCtx<'_> {
            SyncCtx {
                db: &self.db,
                paths: &self.paths,
            }
        }

        fn jar(&self, body: Option<&str>) {
            let path = client_jar_path(&self.paths, "1.21.5");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
            if let Some(body) = body {
                zip.start_file("version.json", SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }

        /// 版 1.21.5 のインスタンス。jar には DataVersion を入れておく。
        fn instance(&self, name: &str, options: Option<&[u8]>) -> Instance {
            self.jar(Some(&format!("{{\"world_version\":{VERSION}}}")));
            let inst = instance::create(
                &self.db,
                &self.paths,
                &NewInstance::vanilla(name, "1.21.5"),
                1,
            )
            .unwrap();
            if let Some(bytes) = options {
                std::fs::write(self.options(&inst), bytes).unwrap();
            }
            inst
        }

        fn options(&self, inst: &Instance) -> PathBuf {
            instance::game_dir(&self.paths, inst).join("options.txt")
        }

        fn profile(&self, values: &[(&str, &str)]) -> store::SyncProfile {
            let p = store::create_profile(&self.db, "P", &ALL).unwrap();
            let values: Vec<(String, String)> = values
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            store::set_values(&self.db, &p.id, &values).unwrap();
            p
        }

        fn join(&self, inst: &Instance, p: &store::SyncProfile) {
            store::join(&self.db, &inst.id, &p.id).unwrap();
        }

        fn read(&self, inst: &Instance) -> String {
            std::fs::read_to_string(self.options(inst)).unwrap()
        }

        fn revision(&self, p: &store::SyncProfile) -> i64 {
            store::list_profiles(&self.db)
                .unwrap()
                .into_iter()
                .find(|x| x.id == p.id)
                .unwrap()
                .revision
        }
    }

    const SAMPLE: &str = "version:4325\r\nfov:0.5\r\nmouseSensitivity:0.3\nlastServer:x\nkey_key.jump:key.keyboard.space\nresourcePacks:[\"a\"]\n";

    #[test]
    fn not_member_is_a_noop() {
        let env = Env::new();
        let inst = env.instance("A", Some(SAMPLE.as_bytes()));
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::NotMember
        );
        assert_eq!(env.read(&inst), SAMPLE);
        assert!(store::snapshot(&env.db, &inst.id).unwrap().is_none());
    }

    #[test]
    fn applies_registered_values_and_preserves_other_lines() {
        let env = Env::new();
        let inst = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = env.profile(&[
            ("fov", "0.9"),
            ("mouseSensitivity", "0.3"),
            ("gamma", "1.0"),
        ]);
        env.join(&inst, &p);
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Applied { changed: 1 }
        );
        // 他の行、改行の種類、並び順はそのまま。行が無い gamma は足さない
        assert_eq!(env.read(&inst), SAMPLE.replace("fov:0.5", "fov:0.9"));
    }

    #[test]
    fn respects_disabled_categories() {
        let env = Env::new();
        let inst = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = env.profile(&[("fov", "0.9"), ("mouseSensitivity", "0.8")]);
        store::set_categories(&env.db, &p.id, &[C::Mouse]).unwrap();
        env.join(&inst, &p);
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Applied { changed: 1 }
        );
        assert_eq!(
            env.read(&inst),
            SAMPLE.replace("mouseSensitivity:0.3", "mouseSensitivity:0.8")
        );
    }

    #[test]
    fn respects_excluded_fields_and_overrides() {
        let env = Env::new();
        let inst = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = env.profile(&[("fov", "0.9"), ("mouseSensitivity", "0.8")]);
        env.join(&inst, &p);
        store::set_excluded(&env.db, &inst.id, &["fov".to_string()]).unwrap();
        store::set_override(&env.db, &inst.id, "mouseSensitivity", Some("0.1")).unwrap();
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Applied { changed: 1 }
        );
        assert_eq!(
            env.read(&inst),
            SAMPLE.replace("mouseSensitivity:0.3", "mouseSensitivity:0.1")
        );
        let snap = store::snapshot(&env.db, &inst.id).unwrap().unwrap();
        assert_eq!(snap.baseline.len(), 1);
        assert_eq!(snap.baseline["mouseSensitivity"], "0.1");
    }

    #[test]
    fn skips_when_options_file_missing() {
        let env = Env::new();
        let inst = env.instance("A", None);
        let p = env.profile(&[("fov", "0.9")]);
        env.join(&inst, &p);
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Skipped(SkipReason::NoOptionsFile)
        );
        assert!(!env.options(&inst).exists());
        let snap = store::snapshot(&env.db, &inst.id).unwrap().unwrap();
        assert_eq!(snap.state, SnapshotState::Skipped);
    }

    #[test]
    fn skips_when_data_version_unreadable() {
        let env = Env::new();
        let inst = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = env.profile(&[("fov", "0.9")]);
        env.join(&inst, &p);
        env.jar(None);
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Skipped(SkipReason::NoDataVersion)
        );
        assert_eq!(env.read(&inst), SAMPLE);
    }

    #[test]
    fn skips_when_version_line_mismatches() {
        let env = Env::new();
        let other = SAMPLE.replace("version:4325", "version:4000");
        let inst = env.instance("A", Some(other.as_bytes()));
        let p = env.profile(&[("fov", "0.9")]);
        env.join(&inst, &p);
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Skipped(SkipReason::VersionMismatch)
        );
        assert_eq!(env.read(&inst), other);
        // version の行が無い場合も同じ
        let bare = env.instance("B", Some(b"fov:0.5\n"));
        env.join(&bare, &p);
        assert_eq!(
            apply_before_launch(&env.ctx(), &bare),
            ApplyOutcome::Skipped(SkipReason::VersionMismatch)
        );
    }

    #[test]
    fn skipping_keeps_an_existing_baseline() {
        let env = Env::new();
        let inst = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = env.profile(&[("fov", "0.9")]);
        env.join(&inst, &p);
        assert!(matches!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Applied { .. }
        ));
        let before = store::snapshot(&env.db, &inst.id).unwrap().unwrap();
        env.jar(None);
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Skipped(SkipReason::NoDataVersion)
        );
        let after = store::snapshot(&env.db, &inst.id).unwrap().unwrap();
        assert_eq!(after.state, SnapshotState::Skipped);
        assert_eq!(after.baseline, before.baseline);
        assert_eq!(after.applied_revision, before.applied_revision);
    }

    #[test]
    fn skips_keys_whose_codec_differs() {
        let env = Env::new();
        let text = "version:4325\nkey_key.jump:57\nkey_key.drop:key.keyboard.q\nkey_key.chat:scancode.4\nkey_key.use:key.mouse.left\n";
        let inst = env.instance("A", Some(text.as_bytes()));
        let p = env.profile(&[
            ("key_key.jump", "key.keyboard.space"),
            ("key_key.drop", "key.keyboard.x"),
            ("key_key.chat", "key.keyboard.t"),
            ("key_key.use", "key.mouse.right"),
            ("key_key.sneak", "key.keyboard.left.shift"),
        ]);
        env.join(&inst, &p);
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Applied { changed: 2 }
        );
        // 系統が違う jump、現在値が判定できない chat は書き換えず、行が無い sneak は足さない
        assert_eq!(
            env.read(&inst),
            "version:4325\nkey_key.jump:57\nkey_key.drop:key.keyboard.x\nkey_key.chat:scancode.4\nkey_key.use:key.mouse.right\n"
        );
    }

    #[test]
    fn unchanged_values_do_not_rewrite_the_file() {
        let env = Env::new();
        let inst = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = env.profile(&[("fov", "0.5")]);
        env.join(&inst, &p);
        let path = env.options(&inst);
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::UpToDate
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before
        );
        assert_eq!(env.read(&inst), SAMPLE);
        // バックアップも作らない
        assert!(!env.paths.backups_dir().join("sync").join(&inst.id).exists());
        // 基準値だけは残る
        let snap = store::snapshot(&env.db, &inst.id).unwrap().unwrap();
        assert_eq!(snap.state, SnapshotState::Applied);
        assert_eq!(snap.baseline["fov"], "0.5");
    }

    #[test]
    fn saves_snapshot_with_baseline_and_revision() {
        let env = Env::new();
        let inst = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = env.profile(&[("fov", "0.9"), ("mouseSensitivity", "0.3")]);
        env.join(&inst, &p);
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Applied { changed: 1 }
        );
        let snap = store::snapshot(&env.db, &inst.id).unwrap().unwrap();
        assert_eq!(snap.state, SnapshotState::Applied);
        assert_eq!(snap.applied_revision, env.revision(&p));
        assert_eq!(
            snap.baseline,
            BTreeMap::from([
                ("fov".to_string(), "0.9".to_string()),
                ("mouseSensitivity".to_string(), "0.3".to_string()),
            ])
        );
        // 書き換え前のバックアップが残る
        let dir = env.paths.backups_dir().join("sync").join(&inst.id);
        assert_eq!(std::fs::read_dir(dir).unwrap().count(), 1);
    }

    #[test]
    fn write_failure_returns_failed_and_keeps_original() {
        let env = Env::new();
        let inst = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = env.profile(&[("fov", "0.9")]);
        env.join(&inst, &p);
        // 一時ファイルの場所をディレクトリで塞いで、書き込みを失敗させる（OS を問わない）
        let mut tmp = env.options(&inst).into_os_string();
        tmp.push(".tmp");
        std::fs::create_dir(&tmp).unwrap();
        let outcome = apply_before_launch(&env.ctx(), &inst);
        assert!(matches!(outcome, ApplyOutcome::Failed(_)), "{outcome:?}");
        assert_eq!(env.read(&inst), SAMPLE);
        let snap = store::snapshot(&env.db, &inst.id).unwrap().unwrap();
        assert_eq!(snap.state, SnapshotState::Failed);
    }

    #[test]
    fn invalid_utf8_fails_and_leaves_the_file() {
        let env = Env::new();
        let bytes = b"version:4325\nfov:0.5\nlastServer:\xff\n";
        let inst = env.instance("A", Some(bytes));
        let p = env.profile(&[("fov", "0.9")]);
        env.join(&inst, &p);
        let outcome = apply_before_launch(&env.ctx(), &inst);
        assert!(matches!(outcome, ApplyOutcome::Failed(_)), "{outcome:?}");
        assert_eq!(std::fs::read(env.options(&inst)).unwrap(), bytes);
    }

    fn proposal() -> Vec<store::Proposal> {
        vec![store::Proposal {
            field_id: "gamma".to_string(),
            value: "1.0".to_string(),
        }]
    }

    #[test]
    fn pending_proposals_survive_skip_then_up_to_date_refresh() {
        let env = Env::new();
        let inst = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = env.profile(&[("fov", "0.5")]);
        env.join(&inst, &p);
        // 版が合わない起動で Skipped、記録ができる
        env.jar(None);
        assert!(matches!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Skipped(_)
        ));
        store::set_proposals(&env.db, &inst.id, &proposal()).unwrap();
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Skipped(SkipReason::NoDataVersion)
        );
        // jar が読めるようになり、値は同じ（基準値の更新だけが走る）
        env.jar(Some("{\"world_version\":4325}"));
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::UpToDate
        );
        let snap = store::snapshot(&env.db, &inst.id).unwrap().unwrap();
        assert_eq!(snap.state, SnapshotState::Applied);
        assert_eq!(snap.proposals, proposal());
    }

    #[test]
    fn pending_proposals_survive_an_applied_launch_after_revision_bump() {
        let env = Env::new();
        let inst = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = env.profile(&[("fov", "0.9")]);
        env.join(&inst, &p);
        apply_before_launch(&env.ctx(), &inst);
        store::set_proposals(&env.db, &inst.id, &proposal()).unwrap();
        let before = env.revision(&p);
        store::set_values(
            &env.db,
            &p.id,
            &[("mouseSensitivity".to_string(), "0.7".to_string())],
        )
        .unwrap();
        assert!(env.revision(&p) > before);
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Applied { changed: 1 }
        );
        let snap = store::snapshot(&env.db, &inst.id).unwrap().unwrap();
        assert_eq!(snap.applied_revision, env.revision(&p));
        assert_eq!(snap.proposals, proposal());
    }

    #[test]
    fn empty_or_unterminated_options_are_a_version_mismatch_without_writes() {
        let env = Env::new();
        let p = env.profile(&[("fov", "0.9")]);
        for body in ["", "fov:0.5"] {
            let inst = env.instance("A", Some(body.as_bytes()));
            env.join(&inst, &p);
            assert_eq!(
                apply_before_launch(&env.ctx(), &inst),
                ApplyOutcome::Skipped(SkipReason::VersionMismatch)
            );
            assert_eq!(env.read(&inst), body);
        }
        // 末尾に改行の無いファイルでも、版が合えば最後の行を壊さず書き換える
        let inst = env.instance("B", Some(b"version:4325\nfov:0.5"));
        env.join(&inst, &p);
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Applied { changed: 1 }
        );
        assert_eq!(env.read(&inst), "version:4325\nfov:0.9");
    }

    #[test]
    fn exclusions_and_overrides_still_hold_after_a_revision_bump() {
        let env = Env::new();
        let inst = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = env.profile(&[("fov", "0.9"), ("mouseSensitivity", "0.8")]);
        env.join(&inst, &p);
        store::set_excluded(&env.db, &inst.id, &["fov".to_string()]).unwrap();
        store::set_override(&env.db, &inst.id, "mouseSensitivity", Some("0.1")).unwrap();
        let before = env.revision(&p);
        store::set_values(
            &env.db,
            &p.id,
            &[
                ("fov".to_string(), "0.7".to_string()),
                ("mouseSensitivity".to_string(), "0.6".to_string()),
            ],
        )
        .unwrap();
        assert!(env.revision(&p) > before);
        assert_eq!(
            apply_before_launch(&env.ctx(), &inst),
            ApplyOutcome::Applied { changed: 1 }
        );
        assert_eq!(
            env.read(&inst),
            SAMPLE.replace("mouseSensitivity:0.3", "mouseSensitivity:0.1")
        );
    }

    #[test]
    fn only_members_are_rewritten() {
        let env = Env::new();
        let member = env.instance("A", Some(SAMPLE.as_bytes()));
        let outsider = env.instance("B", Some(SAMPLE.as_bytes()));
        let p = env.profile(&[("fov", "0.9")]);
        env.join(&member, &p);
        apply_before_launch(&env.ctx(), &member);
        apply_before_launch(&env.ctx(), &outsider);
        assert!(env.read(&member).contains("fov:0.9"));
        assert_eq!(env.read(&outsider), SAMPLE);
    }
}
