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
