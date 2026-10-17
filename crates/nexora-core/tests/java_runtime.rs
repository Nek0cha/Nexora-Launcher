mod common;

use std::path::Path;
use std::pin::Pin;

use nexora_core::db::Db;
use nexora_core::error::CoreError;
use nexora_core::java::verify::{JavaInfo, JavaProbe};
use nexora_core::java::{find_registered, finish_runtime, prepare_runtime};
use nexora_core::net::{Depth, fetch_all};
use nexora_core::paths::AppPaths;
use tokio_util::sync::CancellationToken;

use common::{FakeMojang, FixedProbe, http};

#[tokio::test]
async fn installs_and_registers_runtime() {
    let fake = FakeMojang::start().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    paths.ensure_dirs().unwrap();
    let db = Db::open_in_memory().unwrap();

    let prepared = prepare_runtime(&http(), &paths, &fake.runtime_index_url(), "jre-test", 17)
        .await
        .unwrap();
    fetch_all(
        &http(),
        prepared.downloads.clone(),
        Depth::Full,
        4,
        &CancellationToken::new(),
        |_, _| {},
    )
    .await
    .unwrap();
    let java = finish_runtime(&db, &prepared, &FixedProbe(17), 1)
        .await
        .unwrap();
    assert!(java.is_file());
    assert_eq!(find_registered(&db, "jre-test").unwrap(), Some(java));
}

#[tokio::test]
async fn wrong_major_is_rejected_and_not_registered() {
    let fake = FakeMojang::start().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    paths.ensure_dirs().unwrap();
    let db = Db::open_in_memory().unwrap();
    let prepared = prepare_runtime(&http(), &paths, &fake.runtime_index_url(), "jre-test", 17)
        .await
        .unwrap();
    fetch_all(
        &http(),
        prepared.downloads.clone(),
        Depth::Full,
        4,
        &CancellationToken::new(),
        |_, _| {},
    )
    .await
    .unwrap();
    let err = finish_runtime(&db, &prepared, &FixedProbe(8), 1)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "java.verify_failed");
    assert_eq!(find_registered(&db, "jre-test").unwrap(), None);
}

#[tokio::test]
async fn unknown_component_is_no_compatible_runtime() {
    let fake = FakeMojang::start().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    let err = prepare_runtime(&http(), &paths, &fake.runtime_index_url(), "jre-missing", 8)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "java.no_compatible_runtime");
}

struct WrongArchProbe;

impl JavaProbe for WrongArchProbe {
    fn probe<'a>(
        &'a self,
        _java: &'a Path,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<JavaInfo, CoreError>> + Send + 'a>> {
        Box::pin(async {
            Ok(JavaInfo {
                major: 17,
                arch: "incompatible".into(),
            })
        })
    }
}

#[tokio::test]
async fn wrong_arch_is_rejected_and_not_registered() {
    let fake = FakeMojang::start().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    let db = Db::open_in_memory().unwrap();
    let prepared = prepare_runtime(&http(), &paths, &fake.runtime_index_url(), "jre-test", 17)
        .await
        .unwrap();
    fetch_all(
        &http(),
        prepared.downloads.clone(),
        Depth::Full,
        4,
        &CancellationToken::new(),
        |_, _| {},
    )
    .await
    .unwrap();
    let err = finish_runtime(&db, &prepared, &WrongArchProbe, 1)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "java.verify_failed");
    assert_eq!(find_registered(&db, "jre-test").unwrap(), None);
}

/// Java が起動できず、出力を読めなかった場合。
struct UnreadableProbe;

impl JavaProbe for UnreadableProbe {
    fn probe<'a>(
        &'a self,
        _java: &'a Path,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<JavaInfo, CoreError>> + Send + 'a>> {
        Box::pin(async {
            Err(CoreError::JavaVerifyFailed {
                expected_major: 0,
                found: "exit code 2: Error: could not find java.dll".into(),
            })
        })
    }
}

#[tokio::test]
async fn unreadable_probe_reports_the_expected_major() {
    let fake = FakeMojang::start().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    let db = Db::open_in_memory().unwrap();
    let prepared = prepare_runtime(&http(), &paths, &fake.runtime_index_url(), "jre-test", 17)
        .await
        .unwrap();
    fetch_all(
        &http(),
        prepared.downloads.clone(),
        Depth::Full,
        4,
        &CancellationToken::new(),
        |_, _| {},
    )
    .await
    .unwrap();
    let err = finish_runtime(&db, &prepared, &UnreadableProbe, 1)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        CoreError::JavaVerifyFailed { expected_major: 17, ref found }
            if found == "exit code 2: Error: could not find java.dll"
    ));
    assert_eq!(find_registered(&db, "jre-test").unwrap(), None);
}

#[tokio::test]
async fn corrupt_runtime_manifest_is_rejected() {
    let fake = FakeMojang::start().await;
    wiremock::Mock::given(wiremock::matchers::path("/runtime.json"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_bytes(b"corrupt".to_vec()))
        .with_priority(1)
        .mount(&fake.server)
        .await;
    let temp = tempfile::tempdir().unwrap();
    let err = prepare_runtime(
        &http(),
        &AppPaths::from_root(temp.path()),
        &fake.runtime_index_url(),
        "jre-test",
        17,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "net.hash_mismatch");
    assert_eq!(fake.hits("/java").await, 0);
}

#[tokio::test]
async fn registration_is_updated_and_missing_java_is_ignored() {
    let fake = FakeMojang::start().await;
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    let db = Db::open_in_memory().unwrap();
    let prepared = prepare_runtime(&http(), &paths, &fake.runtime_index_url(), "jre-test", 17)
        .await
        .unwrap();
    fetch_all(
        &http(),
        prepared.downloads.clone(),
        Depth::Full,
        4,
        &CancellationToken::new(),
        |_, _| {},
    )
    .await
    .unwrap();
    finish_runtime(&db, &prepared, &FixedProbe(17), 1)
        .await
        .unwrap();
    finish_runtime(&db, &prepared, &FixedProbe(17), 2)
        .await
        .unwrap();
    let (count, verified): (i64, i64) = db
        .with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT count(*), max(last_verified_at) FROM java_runtimes",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap();
    assert_eq!((count, verified), (1, 2));
    std::fs::remove_file(&prepared.java).unwrap();
    assert_eq!(find_registered(&db, "jre-test").unwrap(), None);
}
