//! Modrinth からの導入と更新を Launcher のタスクとして通す。
mod common;

use std::time::Duration;

use common::content::Harness;
use nexora_core::content::ContentKind;
use nexora_core::net::hash::sha1_hex;
use nexora_core::task::TaskState;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// project `p` を導入できる状態にする（版 `p-v1`、ファイル `p.jar`）。
async fn publish(h: &Harness, project: &str, body: &[u8], delay: Option<Duration>) {
    let version_id = format!("{project}-v1");
    let filename = format!("{project}.jar");
    let version = h.version_json(project, &version_id, &filename, body);
    h.serve_project_versions(project, vec![version]).await;
    h.serve_file(project, &version_id, &filename, body, delay)
        .await;
}

#[tokio::test]
async fn install_content_is_refused_while_the_game_runs() {
    let h = Harness::new().await;
    publish(&h, "pa", b"a", None).await;
    h.run();

    let err = h.launcher.install_content(&h.id, "pa", false).unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    let err = h
        .launcher
        .update_content(&h.id, vec!["mods/a.jar".into()])
        .unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    assert!(h.mods().is_empty());
}

#[tokio::test]
async fn launch_is_refused_while_a_content_task_runs() {
    let h = Harness::new().await;
    publish(&h, "pa", b"a", Some(Duration::from_millis(600))).await;
    let task = h.launcher.install_content(&h.id, "pa", false).unwrap();

    let err = h.launcher.launch_instance(&h.id, true).unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    // 同じインスタンスに、もう 1 つ中身のタスクは始められない
    let err = h.launcher.install_content(&h.id, "pa", false).unwrap_err();
    assert_eq!(err.code(), "instance.busy");

    let snap = h.wait(&task).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
}

#[tokio::test]
async fn content_task_blocks_toggle_and_remove() {
    let h = Harness::new().await;
    h.seed_local("old.jar", b"old");
    publish(&h, "pa", b"a", Some(Duration::from_millis(600))).await;
    let task = h.launcher.install_content(&h.id, "pa", false).unwrap();

    let err = h
        .launcher
        .set_content_enabled(&h.id, ContentKind::Mod, "mods/old.jar", false)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    let err = h
        .launcher
        .remove_content(&h.id, ContentKind::Mod, "mods/old.jar")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    let err = h.launcher.delete_instance(&h.id).await.unwrap_err();
    assert_eq!(err.code(), "instance.busy");

    h.wait(&task).await;
    // タスクが終われば、また操作できる
    h.launcher
        .set_content_enabled(&h.id, ContentKind::Mod, "mods/old.jar", false)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_left_prepared_operation_refuses_further_changes() {
    let h = Harness::new().await;
    h.seed_local("old.jar", b"old");
    publish(&h, "pa", b"a", None).await;
    h.leave_prepared_operation();

    let err = h.launcher.install_content(&h.id, "pa", false).unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    let err = h
        .launcher
        .update_content(&h.id, vec!["mods/old.jar".into()])
        .unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    let err = h
        .launcher
        .remove_content(&h.id, ContentKind::Mod, "mods/old.jar")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    assert_eq!(h.mods(), ["old.jar"]);
}

#[tokio::test]
async fn install_refuses_when_incompatible_installed_and_not_accepted() {
    let h = Harness::new().await;
    h.seed("rival.jar", b"rival", "prival", "r1");
    let mut version = h.version_json("pa", "pa-v1", "pa.jar", b"a");
    version["dependencies"] =
        json!([{ "project_id": "prival", "dependency_type": "incompatible" }]);
    h.serve_project_versions("pa", vec![version]).await;
    h.serve_file("pa", "pa-v1", "pa.jar", b"a", None).await;

    // 導入の前の確認に、非互換の導入済みの MOD が出る
    let preview = h
        .launcher
        .preview_content_install(&h.id, "pa")
        .await
        .unwrap();
    assert_eq!(preview.incompatible, ["rival.jar"]);

    let task = h.launcher.install_content(&h.id, "pa", false).unwrap();
    let snap = h.wait(&task).await;

    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "content.incompatible");
    assert_eq!(h.mods(), ["rival.jar"], "mods/ is unchanged");
    assert_eq!(
        h.hits("/data/pa/pa-v1/pa.jar").await,
        0,
        "nothing was downloaded"
    );
    assert!(h.launcher.content_task_report(&task).is_none());

    // 承知したうえでなら入る
    let task = h.launcher.install_content(&h.id, "pa", true).unwrap();
    let snap = h.wait(&task).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(h.mods(), ["pa.jar", "rival.jar"]);
}

#[tokio::test]
async fn install_records_last_added_instance_and_report() {
    let h = Harness::new().await;
    publish(&h, "lib", b"lib", None).await;
    let mut version = h.version_json("pa", "pa-v1", "pa.jar", b"a");
    version["dependencies"] = json!([{ "project_id": "lib", "dependency_type": "required" }]);
    h.serve_project_versions("pa", vec![version]).await;
    h.serve_file("pa", "pa-v1", "pa.jar", b"a", None).await;

    let preview = h
        .launcher
        .preview_content_install(&h.id, "pa")
        .await
        .unwrap();
    assert_eq!(preview.version_number, "pa-v1-number");
    assert_eq!(preview.dependency_count, 1);
    assert!(preview.incompatible.is_empty());
    // 確認だけでは何も変わらない
    assert!(h.mods().is_empty());
    let before = nexora_core::settings::load(h.launcher.db()).unwrap();
    assert_eq!(before.last_added_instance, None);

    let task = h.launcher.install_content(&h.id, "pa", false).unwrap();
    let snap = h.wait(&task).await;

    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(snap.kind, "content");
    assert_eq!(h.mods(), ["lib.jar", "pa.jar"]);
    let settings = nexora_core::settings::load(h.launcher.db()).unwrap();
    assert_eq!(settings.last_added_instance.as_deref(), Some(h.id.as_str()));
    let report = h.launcher.content_task_report(&task).unwrap();
    assert_eq!(report.installed, 1);
    assert_eq!(report.dependencies_added, 1);
    assert_eq!(report.updated, 0);
    assert!(report.failures.is_empty());
    // 取り出しても残る
    assert_eq!(h.launcher.content_task_report(&task), Some(report));
}

#[tokio::test]
async fn update_all_reports_partial_failures() {
    let h = Harness::new().await;
    h.seed("good.jar", b"good1", "pgood", "good1");
    h.seed("bad.jar", b"bad1", "pbad", "bad1");
    h.seed("gone.jar", b"gone1", "pgone", "gone1");
    let good = h.version_json("pgood", "good2", "good2.jar", b"good2");
    let bad = h.version_json("pbad", "bad2", "bad2.jar", b"bad2");
    h.serve_updates(vec![(sha1_hex(b"good1"), good), (sha1_hex(b"bad1"), bad)])
        .await;
    h.serve_file("pgood", "good2", "good2.jar", b"good2", None)
        .await;
    // 配られるファイルがハッシュと合わない
    h.serve_file("pbad", "bad2", "bad2.jar", b"tampered", None)
        .await;

    let check = h
        .launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();
    assert_eq!(check.updates.len(), 2);
    let paths = vec![
        "mods/bad.jar".to_string(),
        "mods/good.jar".to_string(),
        "mods/gone.jar".to_string(),
    ];
    let task = h.launcher.update_content(&h.id, paths).unwrap();
    let snap = h.wait(&task).await;

    // 一部が失敗しても、タスクは最後まで進んで成功で終わる
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(snap.progress.current, 3);
    assert_eq!(snap.progress.total, 3);
    let report = h.launcher.content_task_report(&task).unwrap();
    assert_eq!(report.updated, 1);
    let mut failed: Vec<&str> = report.failures.iter().map(|f| f.name.as_str()).collect();
    failed.sort();
    assert_eq!(failed, ["bad.jar", "gone.jar"]);
    let codes: Vec<&str> = report
        .failures
        .iter()
        .map(|f| f.error.code.as_str())
        .collect();
    assert!(codes.contains(&"net.hash_mismatch"), "{codes:?}");
    assert!(codes.contains(&"content.dependency"), "{codes:?}");
    // 失敗した分は元のまま、成功した分は入れ替わっている
    assert_eq!(h.mods(), ["bad.jar", "gone.jar", "good2.jar"]);
    assert_eq!(
        std::fs::read(h.mods_dir().join("bad.jar")).unwrap(),
        b"bad1"
    );
    // 更新の確認の結果は、タスクの後には使い回さない
    let hits = h.hits("/version_files/update").await;
    h.launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();
    assert_eq!(h.hits("/version_files/update").await, hits + 1);
}

#[tokio::test]
async fn adding_an_enabled_installed_project_is_refused() {
    let h = Harness::new().await;
    h.seed("pa.jar", b"a0", "pa", "pa-v0");
    publish(&h, "pa", b"a", None).await;

    // 確認の段階で断る
    let err = h
        .launcher
        .preview_content_install(&h.id, "pa")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "content.already_installed");
    let payload = err.to_payload();
    assert_eq!(
        payload.params.get("name"),
        Some(&nexora_core::error::ParamValue::Str("pa.jar".into()))
    );

    // タスクでも断り、2 つ目は入らない
    let task = h.launcher.install_content(&h.id, "pa", false).unwrap();
    let snap = h.wait(&task).await;
    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "content.already_installed");
    assert_eq!(h.mods(), ["pa.jar"]);
    assert_eq!(h.hits("/data/pa/pa-v1/pa.jar").await, 0);
}

#[tokio::test]
async fn adding_a_disabled_installed_project_enables_it() {
    let h = Harness::new().await;
    h.seed("pa.jar.disabled", b"a0", "pa", "pa-v0");
    publish(&h, "pa", b"a", None).await;

    let preview = h
        .launcher
        .preview_content_install(&h.id, "pa")
        .await
        .unwrap();
    assert!(preview.enables_installed, "{preview:?}");
    assert_eq!(preview.dependency_count, 0);
    assert!(preview.incompatible.is_empty());

    let task = h.launcher.install_content(&h.id, "pa", false).unwrap();
    let snap = h.wait(&task).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    // ダウンロードせず、無効の jar を有効にするだけ
    assert_eq!(h.mods(), ["pa.jar"]);
    assert_eq!(std::fs::read(h.mods_dir().join("pa.jar")).unwrap(), b"a0");
    assert_eq!(h.hits("/data/pa/pa-v1/pa.jar").await, 0);
    let report = h.launcher.content_task_report(&task).unwrap();
    assert_eq!(report.installed, 1);
}

#[tokio::test]
async fn install_into_an_instance_without_a_loader_is_refused() {
    let h = Harness::new().await;
    publish(&h, "pa", b"a", None).await;
    let vanilla = nexora_core::instance::create(
        h.launcher.db(),
        &h.paths,
        &nexora_core::instance::NewInstance::vanilla("v", "1.99-test"),
        0,
    )
    .unwrap();

    let err = h
        .launcher
        .preview_content_install(&vanilla.id, "pa")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "content.no_loader");
    let err = h
        .launcher
        .install_content(&vanilla.id, "pa", false)
        .unwrap_err();
    assert_eq!(err.code(), "content.no_loader");
    assert_eq!(h.hits("/project/pa/version").await, 0);
}

#[tokio::test]
async fn invalid_project_id_is_refused_before_any_request() {
    let h = Harness::new().await;
    let err = h
        .launcher
        .preview_content_install(&h.id, "../pa")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "meta.invalid");
}

#[tokio::test]
async fn update_of_a_missing_row_reports_not_found() {
    let h = Harness::new().await;
    h.serve_updates(Vec::new()).await;

    let task = h
        .launcher
        .update_content(&h.id, vec!["mods/missing.jar".into()])
        .unwrap();
    let snap = h.wait(&task).await;

    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    let report = h.launcher.content_task_report(&task).unwrap();
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].name, "missing.jar");
    assert_eq!(report.failures[0].error.code, "content.not_found");
}

#[tokio::test]
async fn unresolved_dependency_error_names_the_project() {
    let h = Harness::new().await;
    let mut version = h.version_json("pa", "pa-v1", "pa.jar", b"a");
    version["dependencies"] = json!([{ "project_id": "lib", "dependency_type": "required" }]);
    h.serve_project_versions("pa", vec![version]).await;
    // 依存には合う版が無い
    h.serve_project_versions("lib", Vec::new()).await;
    Mock::given(method("GET"))
        .and(path("/projects"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([{ "id": "lib", "title": "Lib Name" }])),
        )
        .mount(&h.server)
        .await;

    let task = h.launcher.install_content(&h.id, "pa", false).unwrap();
    let snap = h.wait(&task).await;

    assert_eq!(snap.state, TaskState::Failed);
    let error = snap.error.unwrap();
    assert_eq!(error.code, "content.dependency");
    assert_eq!(
        error.params.get("name"),
        Some(&nexora_core::error::ParamValue::Str("Lib Name".into()))
    );
    assert!(h.mods().is_empty());
}
