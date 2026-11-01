//! ゲームの異常終了の診断。ログとクラッシュレポートから、原因の MOD の候補を求める。

mod clues;
mod modids;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use ts_rs::TS;

use crate::content::ContentItem;
use crate::instance::logs;

pub use clues::{extract_clues, stack_classes};
pub use modids::{jar_defines_class, read_mod_ids};

/// 原因の見つかった理由。強いものほど前に並ぶ（`Ord` の順）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CrashReason {
    MissingDependency,
    IncompatibleVersion,
    ModLoadingError,
    MixinFailure,
    SuspectedInReport,
    StackTrace,
}

/// ログやレポートの 1 つの手がかり。`mod_id` は原因として疑う MOD。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clue {
    pub mod_id: String,
    pub reason: CrashReason,
    /// 根拠として見せる行（短く切ってある）
    pub evidence: String,
    /// `MissingDependency` / `IncompatibleVersion` で、足りない側（または合わない側）の MOD ID
    pub missing: Option<String>,
}

/// 候補の確かさ。`StackTrace` だけが手がかりのものは低い。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Confidence {
    High,
    Low,
}

/// 診断に渡す、有効な MOD の 1 つ。
#[derive(Debug, Clone)]
pub struct ModEntry {
    pub rel_path: String,
    pub name: String,
    pub mod_ids: Vec<String>,
    pub jar: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CrashCandidate {
    pub rel_path: String,
    pub name: String,
    pub mod_id: String,
    /// 強い順
    pub reasons: Vec<CrashReason>,
    pub confidence: Confidence,
    /// 最も強い理由の根拠の行
    pub evidence: String,
    /// 最も強い理由が依存に関するとき、足りない（合わない）側の MOD ID
    pub missing: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CrashDiagnosis {
    pub candidates: Vec<CrashCandidate>,
    /// 今回の起動のゲームのログの名前（ログ画面で開く）。無ければ None
    pub log_name: Option<String>,
}

/// 候補を組み立てる途中の、MOD 1 つ分の集まり。
struct Pending {
    reasons: Vec<CrashReason>,
    /// 最も強い手がかり（同じ強さなら先に出たもの）
    best: Option<Clue>,
    /// `best` が無いとき（スタックトレースだけ）に見せる根拠
    stack_evidence: Option<String>,
}

fn slot(pending: &mut [Option<Pending>], i: usize) -> &mut Pending {
    pending[i].get_or_insert(Pending {
        reasons: Vec::new(),
        best: None,
        stack_evidence: None,
    })
}

/// 手がかりと、入っている MOD から、候補を求める。`mods` は有効な MOD だけを渡す。
pub fn build_candidates(
    clues: Vec<Clue>,
    stack: &[String],
    mods: &[ModEntry],
) -> Vec<CrashCandidate> {
    let mut pending: Vec<Option<Pending>> = mods.iter().map(|_| None).collect();
    for clue in clues {
        // 同じ ID の jar が複数あるとき（重複）は、どれが原因か分からないので全部を候補にする
        for i in (0..mods.len()).filter(|&i| mods[i].mod_ids.contains(&clue.mod_id)) {
            let p = slot(&mut pending, i);
            if !p.reasons.contains(&clue.reason) {
                p.reasons.push(clue.reason);
            }
            if p.best.as_ref().is_none_or(|b| clue.reason < b.reason) {
                p.best = Some(clue.clone());
            }
        }
    }
    for class in stack {
        for (i, m) in mods.iter().enumerate() {
            if jar_defines_class(&m.jar, class) {
                let p = slot(&mut pending, i);
                if !p.reasons.contains(&CrashReason::StackTrace) {
                    p.reasons.push(CrashReason::StackTrace);
                }
                p.stack_evidence
                    .get_or_insert_with(|| format!("at {class}"));
            }
        }
    }
    let mut out: Vec<CrashCandidate> = pending
        .into_iter()
        .zip(mods)
        .filter_map(|(p, m)| {
            let mut p = p?;
            p.reasons.sort();
            let confidence = if p.reasons.iter().any(|r| *r != CrashReason::StackTrace) {
                Confidence::High
            } else {
                Confidence::Low
            };
            let (mod_id, evidence, missing) = match p.best {
                Some(b) => (b.mod_id, b.evidence, b.missing),
                None => (
                    m.mod_ids.first().cloned().unwrap_or_default(),
                    p.stack_evidence.unwrap_or_default(),
                    None,
                ),
            };
            Some(CrashCandidate {
                rel_path: m.rel_path.clone(),
                name: m.name.clone(),
                mod_id,
                reasons: p.reasons,
                confidence,
                evidence,
                missing,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        let key =
            |c: &CrashCandidate| (c.confidence == Confidence::Low, c.reasons.first().copied());
        key(a).cmp(&key(b)).then_with(|| a.name.cmp(&b.name))
    });
    out
}

/// スタックトレースから調べるクラスの数の上限（jar を開く回数を抑える）。
const MAX_STACK_CLASSES: usize = 20;

/// 今回の起動以降に更新された `crash-reports/` のファイルのうち、最も新しい 1 つの中身。
fn newest_crash_report(game_dir: &Path, launched_at: i64) -> Option<String> {
    let since = SystemTime::UNIX_EPOCH + Duration::from_millis(u64::try_from(launched_at).ok()?);
    let entries = std::fs::read_dir(game_dir.join("crash-reports")).ok()?;
    let newest = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "txt"))
        .filter_map(|e| {
            let meta = std::fs::symlink_metadata(e.path()).ok()?;
            let modified = meta.modified().ok()?;
            (meta.is_file() && modified >= since).then_some((modified, e.path()))
        })
        .max_by_key(|(modified, _)| *modified)?;
    logs::read_tail(&newest.1).ok().map(|c| c.text)
}

/// 異常終了の診断。ログとレポートが読めなくても、壊れた jar があっても失敗せず、候補なしにする。
/// `items` は MOD の一覧（無効のものは候補にしない）。
pub fn diagnose(
    items: &[ContentItem],
    game_dir: &Path,
    log_dir: &Path,
    launched_at: i64,
) -> CrashDiagnosis {
    let log_name = format!("{launched_at}.log");
    let log = logs::read(log_dir, &log_name).ok().map(|c| c.text);
    let report = newest_crash_report(game_dir, launched_at);
    let texts: Vec<&str> = [log.as_deref(), report.as_deref()]
        .into_iter()
        .flatten()
        .collect();

    let mods: Vec<ModEntry> = items
        .iter()
        .filter(|i| i.enabled)
        .map(|i| {
            let jar = game_dir.join(&i.rel_path);
            ModEntry {
                rel_path: i.rel_path.clone(),
                name: i.name.clone().unwrap_or_else(|| i.file_name.clone()),
                mod_ids: read_mod_ids(&jar),
                jar,
            }
        })
        .collect();

    let clues: Vec<Clue> = texts.iter().flat_map(|t| extract_clues(t)).collect();
    let mut stack: Vec<String> = Vec::new();
    for class in texts.iter().flat_map(|t| stack_classes(t)) {
        if !stack.contains(&class) {
            stack.push(class);
        }
    }
    stack.truncate(MAX_STACK_CLASSES);

    CrashDiagnosis {
        candidates: build_candidates(clues, &stack, &mods),
        log_name: log.is_some().then_some(log_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn entry(path: &str, ids: &[&str]) -> ModEntry {
        ModEntry {
            rel_path: format!("mods/{path}"),
            name: path.to_string(),
            mod_ids: ids.iter().map(|s| s.to_string()).collect(),
            jar: PathBuf::from("missing.jar"),
        }
    }

    fn clue(id: &str, reason: CrashReason, missing: Option<&str>) -> Clue {
        Clue {
            mod_id: id.into(),
            reason,
            evidence: format!("{id} {reason:?}"),
            missing: missing.map(str::to_string),
        }
    }

    fn jar_with_class(dir: &std::path::Path, name: &str, class_entry: &str) -> ModEntry {
        let jar = dir.join(name);
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
        zip.start_file(class_entry, SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"x").unwrap();
        zip.finish().unwrap();
        ModEntry {
            rel_path: format!("mods/{name}"),
            name: name.to_string(),
            mod_ids: vec![],
            jar,
        }
    }

    #[test]
    fn clue_for_an_installed_mod_becomes_a_high_confidence_candidate() {
        let mods = [entry("foo.jar", &["foo"])];
        let got = build_candidates(
            vec![clue("foo", CrashReason::MixinFailure, None)],
            &[],
            &mods,
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].rel_path, "mods/foo.jar");
        assert_eq!(got[0].mod_id, "foo");
        assert_eq!(got[0].confidence, Confidence::High);
        assert_eq!(got[0].reasons, [CrashReason::MixinFailure]);
        assert_eq!(got[0].evidence, "foo MixinFailure");
    }

    #[test]
    fn clue_for_a_mod_that_is_not_installed_is_dropped() {
        let mods = [entry("foo.jar", &["foo"])];
        let got = build_candidates(
            vec![clue("ghost", CrashReason::ModLoadingError, None)],
            &[],
            &mods,
        );
        assert!(got.is_empty());
    }

    #[test]
    fn missing_dependency_names_the_requester_not_the_missing_mod() {
        let mods = [entry("foo.jar", &["foo"])];
        let got = build_candidates(
            vec![clue("foo", CrashReason::MissingDependency, Some("bar"))],
            &[],
            &mods,
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].mod_id, "foo");
        assert_eq!(got[0].missing.as_deref(), Some("bar"));
    }

    #[test]
    fn several_clues_for_one_mod_make_one_row_with_all_reasons() {
        let mods = [entry("foo.jar", &["foo"])];
        let got = build_candidates(
            vec![
                clue("foo", CrashReason::SuspectedInReport, None),
                clue("foo", CrashReason::MissingDependency, Some("bar")),
            ],
            &[],
            &mods,
        );
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0].reasons,
            [
                CrashReason::MissingDependency,
                CrashReason::SuspectedInReport
            ]
        );
        assert_eq!(got[0].evidence, "foo MissingDependency");
        assert_eq!(got[0].missing.as_deref(), Some("bar"));
    }

    #[test]
    fn stack_only_candidate_has_low_confidence() {
        let dir = tempfile::tempdir().unwrap();
        let mods = [jar_with_class(dir.path(), "a.jar", "com/x/Foo.class")];
        let got = build_candidates(vec![], &["com.x.Foo".to_string()], &mods);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].confidence, Confidence::Low);
        assert_eq!(got[0].reasons, [CrashReason::StackTrace]);
    }

    #[test]
    fn a_stack_hit_adds_a_reason_to_a_mod_with_a_clue_and_keeps_it_high() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = jar_with_class(dir.path(), "a.jar", "com/x/Foo.class");
        m.mod_ids = vec!["a".into()];
        let got = build_candidates(
            vec![clue("a", CrashReason::MixinFailure, None)],
            &["com.x.Foo".to_string()],
            &[m],
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].confidence, Confidence::High);
        assert_eq!(
            got[0].reasons,
            [CrashReason::MixinFailure, CrashReason::StackTrace]
        );
    }

    #[test]
    fn order_is_confidence_then_strongest_reason() {
        let dir = tempfile::tempdir().unwrap();
        let mut mods = vec![
            entry("mixin.jar", &["mixin"]),
            entry("dep.jar", &["dep"]),
            jar_with_class(dir.path(), "stack.jar", "com/x/Foo.class"),
        ];
        mods[2].name = "stack.jar".into();
        let got = build_candidates(
            vec![
                clue("mixin", CrashReason::MixinFailure, None),
                clue("dep", CrashReason::MissingDependency, Some("z")),
            ],
            &["com.x.Foo".to_string()],
            &mods,
        );
        let names: Vec<_> = got.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["dep.jar", "mixin.jar", "stack.jar"]);
    }

    #[test]
    fn every_jar_with_the_clued_mod_id_is_a_candidate() {
        let mods = [entry("foo-1.jar", &["foo"]), entry("foo-2.jar", &["foo"])];
        let got = build_candidates(
            vec![clue("foo", CrashReason::ModLoadingError, None)],
            &[],
            &mods,
        );
        let names: Vec<_> = got.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["foo-1.jar", "foo-2.jar"]);
    }

    #[test]
    fn a_jar_with_two_mod_ids_matches_either() {
        let mods = [entry("pack.jar", &["one", "two"])];
        let a = build_candidates(
            vec![clue("two", CrashReason::ModLoadingError, None)],
            &[],
            &mods,
        );
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].mod_id, "two");
        let b = build_candidates(
            vec![
                clue("one", CrashReason::ModLoadingError, None),
                clue("two", CrashReason::MixinFailure, None),
            ],
            &[],
            &mods,
        );
        assert_eq!(b.len(), 1);
    }
}
