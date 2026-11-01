//! 異常終了の診断（ゲームのログとクラッシュレポートから、原因の MOD の候補を求める）を Launcher を通して確かめる。
mod common;

use std::io::Write;
use std::path::Path;
use std::time::{Duration, SystemTime};

use common::content::Harness;
use nexora_core::diagnose::{Confidence, CrashReason};
use zip::write::SimpleFileOptions;

const LAUNCHED_AT: i64 = 1_700_000_000_000;

fn jar_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut buf);
    for (name, body) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(body).unwrap();
    }
    zip.finish().unwrap();
    buf.into_inner()
}

fn fabric_jar(id: &str) -> Vec<u8> {
    jar_bytes(&[(
        "fabric.mod.json",
        format!(r#"{{"id":"{id}","name":"{id}","version":"1"}}"#).as_bytes(),
    )])
}

fn write_game_log(h: &Harness, launched_at: i64, text: &[u8]) {
    let dir = h.paths.game_logs_dir().join(&h.id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{launched_at}.log")), text).unwrap();
}

const MISSING_LINE: &str = "[12:00:01] [main/ERROR] (FabricLoader) \t - Mod 'Foo' (foo) 1.0.0 requires version 2.0 or later of mod 'Bar' (bar), which is missing!\n";

fn set_mtime(path: &Path, at: SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

#[tokio::test]
async fn a_missing_dependency_in_the_game_log_names_the_requesting_mod() {
    let h = Harness::new().await;
    h.seed_local("foo.jar", &fabric_jar("foo"));
    h.seed_local("other.jar", &fabric_jar("other"));
    write_game_log(&h, LAUNCHED_AT, MISSING_LINE.as_bytes());

    let got = h.launcher.diagnose_crash(&h.id, LAUNCHED_AT).await.unwrap();

    assert_eq!(got.log_name.as_deref(), Some("1700000000000.log"));
    assert_eq!(got.candidates.len(), 1, "{got:?}");
    let c = &got.candidates[0];
    assert_eq!(c.rel_path, "mods/foo.jar");
    assert_eq!(c.reasons, [CrashReason::MissingDependency]);
    assert_eq!(c.confidence, Confidence::High);
    assert_eq!(c.missing.as_deref(), Some("bar"));
}

#[tokio::test]
async fn a_disabled_mod_is_not_a_candidate() {
    let h = Harness::new().await;
    h.seed_local("foo.jar.disabled", &fabric_jar("foo"));
    write_game_log(&h, LAUNCHED_AT, MISSING_LINE.as_bytes());

    let got = h.launcher.diagnose_crash(&h.id, LAUNCHED_AT).await.unwrap();

    assert!(got.candidates.is_empty(), "{got:?}");
}

#[tokio::test]
async fn a_missing_log_and_no_crash_reports_folder_give_no_candidates_without_an_error() {
    let h = Harness::new().await;
    h.seed_local("foo.jar", &fabric_jar("foo"));

    let got = h.launcher.diagnose_crash(&h.id, LAUNCHED_AT).await.unwrap();

    assert!(got.candidates.is_empty());
    assert_eq!(got.log_name, None);
}

#[tokio::test]
async fn a_log_that_is_not_utf8_and_a_broken_jar_do_not_fail_the_diagnosis() {
    let h = Harness::new().await;
    h.seed_local("foo.jar", &fabric_jar("foo"));
    h.seed_local("broken.jar", b"not a zip");
    let mut log = vec![0xff, 0xfe, b'\n'];
    log.extend_from_slice(MISSING_LINE.as_bytes());
    write_game_log(&h, LAUNCHED_AT, &log);

    let got = h.launcher.diagnose_crash(&h.id, LAUNCHED_AT).await.unwrap();

    assert_eq!(got.candidates.len(), 1, "{got:?}");
    assert_eq!(got.candidates[0].rel_path, "mods/foo.jar");
}

#[tokio::test]
async fn only_a_crash_report_written_since_the_launch_is_read() {
    let h = Harness::new().await;
    h.seed_local("foo.jar", &fabric_jar("foo"));
    h.seed_local("old.jar", &fabric_jar("old"));
    let dir = h.game_dir().join("crash-reports");
    std::fs::create_dir_all(&dir).unwrap();
    let launch = SystemTime::UNIX_EPOCH + Duration::from_millis(LAUNCHED_AT as u64);
    // 起動の前の古いレポート
    let old = dir.join("crash-old.txt");
    std::fs::write(&old, "Suspected Mods: Old (old)\n").unwrap();
    set_mtime(&old, launch - Duration::from_secs(3600));
    // 起動のあとのレポート
    let fresh = dir.join("crash-new.txt");
    std::fs::write(&fresh, "Suspected Mods: Foo (foo)\n").unwrap();
    set_mtime(&fresh, launch + Duration::from_secs(60));

    let got = h.launcher.diagnose_crash(&h.id, LAUNCHED_AT).await.unwrap();

    assert_eq!(got.candidates.len(), 1, "{got:?}");
    assert_eq!(got.candidates[0].rel_path, "mods/foo.jar");
    assert_eq!(got.candidates[0].reasons, [CrashReason::SuspectedInReport]);
}
