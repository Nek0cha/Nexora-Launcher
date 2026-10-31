//! `.mrpack` の検査とインポート。Launcher を通して確かめる。
mod common;

use common::mrpack::{PackBuilder, PackHarness};
use nexora_core::meta::loader::LoaderKind;
use serde_json::json;

#[tokio::test]
async fn inspect_returns_the_pack_summary() {
    let h = PackHarness::new().await;
    let a = h.file_entry("mods/a.jar", b"a", "/a.jar", None);
    let server_only = h.file_entry(
        "mods/s.jar",
        b"s",
        "/s.jar",
        Some(json!({ "client": "unsupported", "server": "required" })),
    );
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(PackBuilder::index(
            "1.99-test",
            Some(("fabric-loader", "0.16.10")),
            vec![a, server_only],
        )),
    );

    let info = h.launcher.inspect_mrpack(path).await.unwrap();

    assert_eq!(info.name, "Test pack");
    assert_eq!(info.version_id, "1.0.0");
    assert_eq!(info.mc_version, "1.99-test");
    assert_eq!(info.loader, LoaderKind::Fabric);
    assert_eq!(info.loader_version.as_deref(), Some("0.16.10"));
    assert_eq!(info.file_count, 1);
}

#[tokio::test]
async fn inspect_accepts_a_pack_without_a_loader() {
    let h = PackHarness::new().await;
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(PackBuilder::index("1.99-test", None, vec![])),
    );

    let info = h.launcher.inspect_mrpack(path).await.unwrap();

    assert_eq!(info.loader, LoaderKind::Vanilla);
    assert_eq!(info.loader_version, None);
}

#[tokio::test]
async fn inspect_refuses_a_loader_version_the_installer_does_not_know() {
    let h = PackHarness::new().await;
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(PackBuilder::index(
            "1.99-test",
            Some(("fabric-loader", "9.9.9")),
            vec![],
        )),
    );

    let err = h.launcher.inspect_mrpack(path).await.unwrap_err();

    assert_eq!(err.code(), "mrpack.unsupported");
}

#[tokio::test]
async fn inspect_refuses_a_broken_zip() {
    let h = PackHarness::new().await;
    let path = h.packs.join("broken.mrpack");
    std::fs::write(&path, b"not a zip").unwrap();

    let err = h.launcher.inspect_mrpack(path).await.unwrap_err();

    assert_eq!(err.code(), "mrpack.invalid");
}

// ---- インポート ----

use std::path::PathBuf;
use std::time::Duration;

use nexora_core::content::ContentKind;
use nexora_core::instance::{self, InstanceState};
use nexora_core::task::TaskState;

fn game_dir(h: &PackHarness, id: &str) -> PathBuf {
    let inst = instance::get(h.launcher.db(), id).unwrap();
    instance::game_dir(&h.paths, &inst)
}

fn fabric_index(files: Vec<serde_json::Value>) -> serde_json::Value {
    PackBuilder::index("1.99-test", Some(("fabric-loader", "0.16.10")), files)
}

/// インスタンスの行も、インスタンスのフォルダーも残っていないこと。
fn assert_nothing_left(h: &PackHarness) {
    assert!(h.launcher.list_instances().unwrap().is_empty());
    let left: Vec<_> = std::fs::read_dir(h.paths.instances_dir())
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "left behind: {left:?}");
}

#[tokio::test]
async fn import_creates_a_ready_instance_with_files_and_overrides() {
    let h = PackHarness::new().await;
    h.serve_file("/a.jar", b"mod-a").await;
    let entry = h.file_entry("mods/a.jar", b"mod-a", "/a.jar", None);
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new()
            .manifest_json(fabric_index(vec![entry]))
            .file("overrides/config/x.toml", b"x = 1"),
    );

    let created = h
        .launcher
        .import_mrpack(path, "My pack".into())
        .await
        .unwrap();
    let snap = h.wait(&created.task_id).await;

    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    let inst = instance::get(h.launcher.db(), &created.instance.id).unwrap();
    assert_eq!(inst.state, InstanceState::Ready);
    assert_eq!(inst.name, "My pack");
    assert_eq!(inst.loader_version.as_deref(), Some("0.16.10"));
    let dir = game_dir(&h, &inst.id);
    assert_eq!(std::fs::read(dir.join("mods/a.jar")).unwrap(), b"mod-a");
    assert_eq!(std::fs::read(dir.join("config/x.toml")).unwrap(), b"x = 1");
    assert!(!dir.join(".mrpack-staging").exists());
    let items = h
        .launcher
        .list_content(&inst.id, ContentKind::Mod)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].file_name, "a.jar");
    assert!(items[0].enabled);
}

#[tokio::test]
async fn client_overrides_win_over_overrides_and_over_downloaded_files() {
    let h = PackHarness::new().await;
    h.serve_file("/c.txt", b"from-files").await;
    let entry = h.file_entry("config/a.txt", b"from-files", "/c.txt", None);
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new()
            .manifest_json(fabric_index(vec![entry]))
            .file("overrides/config/a.txt", b"common")
            .file("client-overrides/config/a.txt", b"client"),
    );

    let created = h.launcher.import_mrpack(path, "P".into()).await.unwrap();
    assert_eq!(h.wait(&created.task_id).await.state, TaskState::Succeeded);

    let dir = game_dir(&h, &created.instance.id);
    assert_eq!(std::fs::read(dir.join("config/a.txt")).unwrap(), b"client");
}

#[tokio::test]
async fn env_client_unsupported_files_are_not_downloaded() {
    let h = PackHarness::new().await;
    let server_only = h.file_entry(
        "mods/s.jar",
        b"s",
        "/s.jar",
        Some(json!({ "client": "unsupported", "server": "required" })),
    );
    // 配信しない。取りに行けば 404 で失敗する
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(fabric_index(vec![server_only])),
    );

    let created = h.launcher.import_mrpack(path, "P".into()).await.unwrap();
    let snap = h.wait(&created.task_id).await;

    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert!(
        !game_dir(&h, &created.instance.id)
            .join("mods/s.jar")
            .exists()
    );
}

#[tokio::test]
async fn optional_files_are_installed_enabled() {
    let h = PackHarness::new().await;
    h.serve_file("/o.jar", b"o").await;
    let optional = h.file_entry(
        "mods/o.jar",
        b"o",
        "/o.jar",
        Some(json!({ "client": "optional", "server": "optional" })),
    );
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(fabric_index(vec![optional])),
    );

    let created = h.launcher.import_mrpack(path, "P".into()).await.unwrap();
    assert_eq!(h.wait(&created.task_id).await.state, TaskState::Succeeded);

    assert!(
        game_dir(&h, &created.instance.id)
            .join("mods/o.jar")
            .exists()
    );
}

#[tokio::test]
async fn a_hash_mismatch_removes_the_instance_and_the_staging() {
    let h = PackHarness::new().await;
    h.serve_file("/a.jar", b"tampered").await;
    let entry = h.file_entry("mods/a.jar", b"mod-a", "/a.jar", None);
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(fabric_index(vec![entry])),
    );

    let created = h.launcher.import_mrpack(path, "P".into()).await.unwrap();
    let snap = h.wait(&created.task_id).await;

    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "net.hash_mismatch");
    assert_nothing_left(&h);
}

#[tokio::test]
async fn cancelling_removes_the_instance() {
    let h = PackHarness::new().await;
    h.serve_file_slow("/a.jar", b"mod-a", Duration::from_millis(1500))
        .await;
    let entry = h.file_entry("mods/a.jar", b"mod-a", "/a.jar", None);
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(fabric_index(vec![entry])),
    );

    let created = h.launcher.import_mrpack(path, "P".into()).await.unwrap();
    h.wait_for_phase(&created.task_id, nexora_core::task::TaskPhase::Downloading)
        .await;
    h.tasks.cancel(&created.task_id).unwrap();
    let snap = h.wait(&created.task_id).await;

    assert_eq!(snap.state, TaskState::Cancelled);
    assert_nothing_left(&h);
}

#[tokio::test]
async fn offline_import_leaves_nothing_behind() {
    let h = PackHarness::new().await;
    let mut entry = h.file_entry("mods/a.jar", b"mod-a", "/a.jar", None);
    entry["downloads"] = json!(["http://127.0.0.1:1/a.jar"]);
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(fabric_index(vec![entry])),
    );

    let created = h.launcher.import_mrpack(path, "P".into()).await.unwrap();
    let snap = h.wait(&created.task_id).await;

    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "net.offline");
    assert_nothing_left(&h);
}

#[tokio::test]
async fn an_empty_pack_still_makes_a_ready_instance() {
    let h = PackHarness::new().await;
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(fabric_index(vec![])),
    );

    let created = h.launcher.import_mrpack(path, "P".into()).await.unwrap();
    let snap = h.wait(&created.task_id).await;

    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    let inst = instance::get(h.launcher.db(), &created.instance.id).unwrap();
    assert_eq!(inst.state, InstanceState::Ready);
}

#[tokio::test]
async fn the_instance_is_busy_while_the_pack_is_being_applied() {
    let h = PackHarness::new().await;
    h.serve_file_slow("/a.jar", b"mod-a", Duration::from_millis(600))
        .await;
    let entry = h.file_entry("mods/a.jar", b"mod-a", "/a.jar", None);
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(fabric_index(vec![entry])),
    );

    let created = h.launcher.import_mrpack(path, "P".into()).await.unwrap();
    h.wait_for_phase(&created.task_id, nexora_core::task::TaskPhase::Downloading)
        .await;

    let err = h
        .launcher
        .delete_instance(&created.instance.id)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    assert_eq!(h.wait(&created.task_id).await.state, TaskState::Succeeded);
}

#[tokio::test]
async fn unsafe_paths_are_refused_before_any_instance_is_created() {
    let h = PackHarness::new().await;
    let entry = h.file_entry("mods/../../evil.jar", b"x", "/x.jar", None);
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(fabric_index(vec![entry])),
    );

    let err = h
        .launcher
        .import_mrpack(path, "P".into())
        .await
        .unwrap_err();

    assert_eq!(err.code(), "mrpack.unsafe_path");
    assert_nothing_left(&h);
}

#[tokio::test]
async fn a_blank_name_falls_back_to_the_pack_name() {
    let h = PackHarness::new().await;
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(fabric_index(vec![])),
    );

    let created = h.launcher.import_mrpack(path, "   ".into()).await.unwrap();
    h.wait(&created.task_id).await;

    assert_eq!(created.instance.name, "Test pack");
}
