//! インスタンスのバージョンアップグレード。偽の Mojang とローダー、Modrinth を Launcher に通して確かめる。
mod common;

use common::content::Harness;
use common::{FABRIC_VERSION, FakeMojang};
use nexora_core::instance::{self, NewInstance};
use nexora_core::meta::loader::LoaderKind;
use nexora_core::net::hash::sha1_hex;

/// 偽物が導入できる新しい版。
const NEW: &str = "1.99-test";
/// 偽物のマニフェストにある古い版（導入はできない）。
const OLD: &str = "1.8.9";

async fn harness() -> (FakeMojang, Harness) {
    let fake = FakeMojang::start().await;
    let h = Harness::with_mojang(&fake, OLD).await;
    (fake, h)
}

fn version_of(h: &Harness) -> (String, Option<String>) {
    let inst = instance::get(h.launcher.db(), &h.id).unwrap();
    (inst.mc_version, inst.loader_version)
}

#[tokio::test]
async fn preview_rejects_the_same_older_and_unknown_versions() {
    let (_fake, h) = harness().await;
    instance::set_version(h.launcher.db(), &h.id, NEW, Some("0.15.0")).unwrap();
    // 同じ版、古い版、マニフェストに無い版（スナップショットなど）
    for target in [NEW, OLD, "24w01a"] {
        let err = h.launcher.preview_upgrade(&h.id, target).await.unwrap_err();
        assert_eq!(err.code(), "upgrade.unavailable", "{target}");
    }
    assert_eq!(
        version_of(&h),
        (NEW.to_string(), Some("0.15.0".to_string()))
    );
}

#[tokio::test]
async fn preview_for_vanilla_has_no_loader_version() {
    let (_fake, h) = harness().await;
    let vanilla = instance::create(
        h.launcher.db(),
        &h.paths,
        &NewInstance::vanilla("v", OLD),
        1,
    )
    .unwrap();
    let preview = h.launcher.preview_upgrade(&vanilla.id, NEW).await.unwrap();
    assert_eq!(preview.loader_version, None);
}

#[tokio::test]
async fn preview_picks_the_stable_loader_version_and_rejects_an_unsupported_loader() {
    let (_fake, h) = harness().await;
    let preview = h.launcher.preview_upgrade(&h.id, NEW).await.unwrap();
    assert_eq!(preview.loader_version.as_deref(), Some(FABRIC_VERSION));

    // Quilt は偽物の対応表に上げ先が無い
    let quilt = instance::create(
        h.launcher.db(),
        &h.paths,
        &NewInstance {
            loader: LoaderKind::Quilt,
            loader_version: Some("0.1.0".into()),
            ..NewInstance::vanilla("q", OLD)
        },
        2,
    )
    .unwrap();
    let err = h
        .launcher
        .preview_upgrade(&quilt.id, NEW)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "upgrade.unavailable");
}

#[tokio::test]
async fn preview_classifies_mods_against_the_target_version() {
    let (_fake, h) = harness().await;
    h.seed("ok.jar", b"o1", "po", "o1");
    h.seed("same.jar", b"s1", "ps", "s1");
    h.seed("gone.jar", b"g1", "pg", "g1");
    h.seed("pinned.jar", b"p1", "pp", "p1");
    h.pin("pinned.jar");
    h.seed_local("mine.jar", b"m1");
    h.serve_updates(vec![
        (sha1_hex(b"o1"), h.version_json("po", "o2", "o2.jar", b"o2")),
        // 入っている版がそのまま合う
        (sha1_hex(b"s1"), h.version_json("ps", "s1", "s1.jar", b"s1")),
    ])
    .await;

    let preview = h.launcher.preview_upgrade(&h.id, NEW).await.unwrap();

    let mut compatible = preview.compatible.clone();
    compatible.sort();
    assert_eq!(compatible, ["mods/ok.jar", "mods/same.jar"]);
    assert_eq!(preview.incompatible, ["mods/gone.jar"]);
    let mut unknown = preview.unknown.clone();
    unknown.sort();
    assert_eq!(unknown, ["mods/mine.jar", "mods/pinned.jar"]);

    // 問い合わせは上げ先のバージョンで行う
    let requests = h.server.received_requests().await.unwrap();
    let body = requests
        .iter()
        .find(|r| r.url.path() == "/version_files/update")
        .map(|r| String::from_utf8_lossy(&r.body).into_owned())
        .unwrap();
    assert!(body.contains(NEW), "{body}");
    assert!(!body.contains(OLD), "{body}");
}

#[tokio::test]
async fn preview_does_not_fill_the_update_cache() {
    let (_fake, h) = harness().await;
    h.seed("a.jar", b"a1", "pa", "a1");
    h.serve_updates(vec![]).await;
    h.launcher.preview_upgrade(&h.id, NEW).await.unwrap();
    assert_eq!(h.hits("/version_files/update").await, 1);
    // 確認の結果を使い回していないので、通常の確認はもう一度通信する
    h.launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();
    assert_eq!(h.hits("/version_files/update").await, 2);
}

// ---- 実行 ----

use nexora_core::instance::InstanceState;
use nexora_core::task::TaskState;

fn put(path: &std::path::Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn ready(h: &Harness) {
    instance::set_state(h.launcher.db(), &h.id, InstanceState::Ready).unwrap();
}

fn state_of(h: &Harness) -> InstanceState {
    instance::get(h.launcher.db(), &h.id).unwrap().state
}

fn upgrade_backups(h: &Harness) -> Vec<std::path::PathBuf> {
    let root = h.paths.backups_dir().join(&h.id);
    let Ok(entries) = std::fs::read_dir(root) else {
        return vec![];
    };
    entries
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("upgrade-")
        })
        .collect()
}

#[tokio::test]
async fn upgrade_updates_the_row_installs_and_backs_up_the_saves() {
    let (_fake, h) = harness().await;
    ready(&h);
    put(&h.game_dir().join("saves/world/level.dat"), b"world");
    put(&h.mods_dir().join("a.jar"), b"mod");

    let task = h.launcher.upgrade_instance(&h.id, NEW.into()).unwrap();
    let snap = h.wait(&task).await;

    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(
        version_of(&h),
        (NEW.to_string(), Some(FABRIC_VERSION.to_string()))
    );
    assert_eq!(state_of(&h), InstanceState::Ready);
    let backups = upgrade_backups(&h);
    assert_eq!(backups.len(), 1);
    assert_eq!(
        std::fs::read(backups[0].join("saves/world/level.dat")).unwrap(),
        b"world"
    );
    // 元のワールドと MOD には触れない
    assert_eq!(
        std::fs::read(h.game_dir().join("saves/world/level.dat")).unwrap(),
        b"world"
    );
    assert_eq!(h.mods(), ["a.jar"]);
}

#[tokio::test]
async fn an_older_backup_is_removed_only_after_the_new_one_is_made() {
    let (_fake, h) = harness().await;
    ready(&h);
    put(&h.game_dir().join("saves/world/level.dat"), b"world");
    let root = h.paths.backups_dir().join(&h.id);
    put(&root.join("upgrade-1/saves/old.dat"), b"old");
    put(&root.join("other-op-file.jar"), b"mod backup");

    let task = h.launcher.upgrade_instance(&h.id, NEW.into()).unwrap();
    assert_eq!(h.wait(&task).await.state, TaskState::Succeeded);

    let backups = upgrade_backups(&h);
    assert_eq!(backups.len(), 1);
    assert!(!root.join("upgrade-1").exists());
    // MOD の退避など、アップグレードの退避でないものには触れない
    assert!(root.join("other-op-file.jar").exists());
}

#[tokio::test]
async fn upgrade_without_a_saves_folder_still_succeeds() {
    let (_fake, h) = harness().await;
    ready(&h);
    let task = h.launcher.upgrade_instance(&h.id, NEW.into()).unwrap();
    assert_eq!(h.wait(&task).await.state, TaskState::Succeeded);
    assert_eq!(version_of(&h).0, NEW);
    assert!(upgrade_backups(&h).is_empty());
}

#[tokio::test]
async fn a_failed_install_restores_the_row_and_the_state() {
    // ライブラリが 404 になり、導入が失敗する
    let fake = FakeMojang::start_with(false).await;
    let h = Harness::with_mojang(&fake, OLD).await;
    ready(&h);
    put(&h.game_dir().join("saves/world/level.dat"), b"world");

    let task = h.launcher.upgrade_instance(&h.id, NEW.into()).unwrap();
    let snap = h.wait(&task).await;

    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(
        version_of(&h),
        (OLD.to_string(), Some("0.15.0".to_string()))
    );
    // install は失敗で broken を書くが、遊べる元の状態に戻す
    assert_eq!(state_of(&h), InstanceState::Ready);
}

#[tokio::test]
async fn cancelling_restores_the_row_and_the_state() {
    let (_fake, h) = harness().await;
    ready(&h);
    put(&h.game_dir().join("saves/world/level.dat"), b"world");

    let task = h.launcher.upgrade_instance(&h.id, NEW.into()).unwrap();
    h.tasks.cancel(&task).unwrap();
    let snap = h.wait(&task).await;

    assert_eq!(snap.state, TaskState::Cancelled);
    assert_eq!(
        version_of(&h),
        (OLD.to_string(), Some("0.15.0".to_string()))
    );
    assert_eq!(state_of(&h), InstanceState::Ready);
}

#[tokio::test]
async fn an_unavailable_target_fails_the_task_and_changes_nothing() {
    let (_fake, h) = harness().await;
    ready(&h);
    put(&h.game_dir().join("saves/world/level.dat"), b"world");

    let task = h.launcher.upgrade_instance(&h.id, "24w01a".into()).unwrap();
    let snap = h.wait(&task).await;

    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(
        version_of(&h),
        (OLD.to_string(), Some("0.15.0".to_string()))
    );
    assert_eq!(state_of(&h), InstanceState::Ready);
    assert!(upgrade_backups(&h).is_empty());
}

#[tokio::test]
async fn upgrade_is_refused_while_the_game_runs() {
    let (_fake, h) = harness().await;
    h.run();
    let err = h.launcher.upgrade_instance(&h.id, NEW.into()).unwrap_err();
    assert_eq!(err.code(), "instance.busy");
}

#[tokio::test]
async fn an_upgrade_task_blocks_launch_and_a_second_upgrade() {
    let (_fake, h) = harness().await;
    ready(&h);
    let task = h.launcher.upgrade_instance(&h.id, NEW.into()).unwrap();

    let launch = h.launcher.launch_instance(&h.id, false).unwrap_err();
    assert_eq!(launch.code(), "instance.busy");
    let again = h.launcher.upgrade_instance(&h.id, NEW.into()).unwrap_err();
    assert_eq!(again.code(), "instance.busy");

    h.wait(&task).await;
}

#[tokio::test]
async fn upgrade_invalidates_the_update_cache() {
    let (_fake, h) = harness().await;
    ready(&h);
    h.seed("a.jar", b"a1", "pa", "a1");
    h.serve_updates(vec![]).await;
    h.launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();
    assert_eq!(h.hits("/version_files/update").await, 1);

    let task = h.launcher.upgrade_instance(&h.id, NEW.into()).unwrap();
    assert_eq!(h.wait(&task).await.state, TaskState::Succeeded);

    // 古いバージョンについての結果は使い回さない
    h.launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();
    assert_eq!(h.hits("/version_files/update").await, 2);
}
