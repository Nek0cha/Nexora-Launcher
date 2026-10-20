//! Fabric と Quilt のメタデータの取得と、プロファイルの固定。
mod common;

use common::{http, serve};
use nexora_core::meta::loader::{
    LoaderKind, LoaderMeta, LoaderVersionEntry, load_pinned_profile, profile_id, profile_path,
};
use nexora_core::paths::AppPaths;
use wiremock::MockServer;

const FABRIC_GAME: &[u8] = include_bytes!("fixtures/loaders/fabric-game.json");
const QUILT_GAME: &[u8] = include_bytes!("fixtures/loaders/quilt-game.json");
const FABRIC_LOADERS: &[u8] = include_bytes!("fixtures/loaders/fabric-loader-1.20.1.json");
const QUILT_LOADERS: &[u8] = include_bytes!("fixtures/loaders/quilt-loader-1.20.1.json");
const FABRIC_PROFILE: &[u8] = include_bytes!("fixtures/loaders/fabric-profile-1.20.1-0.16.10.json");

const FABRIC_PROFILE_ROUTE: &str = "/v2/versions/loader/1.20.1/0.16.10/profile/json";

struct Harness {
    _temp: tempfile::TempDir,
    paths: AppPaths,
    meta: LoaderMeta,
}

/// Fabric と Quilt の両方を server に向けた LoaderMeta。
fn harness(server: &MockServer) -> Harness {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    paths.ensure_dirs().unwrap();
    let meta = LoaderMeta::new(http(), paths.clone(), server.uri(), server.uri());
    Harness {
        _temp: temp,
        paths,
        meta,
    }
}

async fn requests_to(server: &MockServer, route: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == route)
        .count()
}

fn entry(version: &str, stable: bool) -> LoaderVersionEntry {
    LoaderVersionEntry {
        version: version.into(),
        stable,
    }
}

#[tokio::test]
async fn lists_game_versions_per_loader() {
    let server = MockServer::start().await;
    serve(&server, "/v2/versions/game", FABRIC_GAME).await;
    serve(&server, "/v3/versions/game", QUILT_GAME).await;
    let h = harness(&server);

    let fabric = h.meta.game_versions(LoaderKind::Fabric).await.unwrap();
    assert_eq!(
        fabric,
        vec![
            "26.4-snapshot-4",
            "26.4-snapshot-3",
            "26.4-snapshot-2",
            "26.4-snapshot-1",
            "26.3",
            "26.3-rc-3",
        ]
    );
    let quilt = h.meta.game_versions(LoaderKind::Quilt).await.unwrap();
    assert_eq!(
        quilt,
        vec![
            "26.3",
            "26.3-rc-3",
            "26.3-rc-2",
            "26.3-rc-1",
            "26.3-pre-3",
            "26.3-pre-2",
        ]
    );
}

#[tokio::test]
async fn lists_loader_versions_with_stability() {
    let server = MockServer::start().await;
    serve(&server, "/v2/versions/loader/1.20.1", FABRIC_LOADERS).await;
    serve(&server, "/v3/versions/loader/1.20.1", QUILT_LOADERS).await;
    let h = harness(&server);

    let fabric = h
        .meta
        .loader_versions(LoaderKind::Fabric, "1.20.1")
        .await
        .unwrap();
    assert_eq!(
        fabric,
        vec![
            entry("0.20.1", false),
            entry("0.20.0", false),
            entry("0.19.5", true),
            entry("0.19.4", false),
        ]
    );
    let quilt = h
        .meta
        .loader_versions(LoaderKind::Quilt, "1.20.1")
        .await
        .unwrap();
    assert_eq!(quilt.len(), 9);
    assert_eq!(quilt[0], entry("0.31.0-beta.4", false));
    assert_eq!(quilt[1], entry("0.28.1", true));
    for e in &quilt {
        assert_eq!(e.stable, !e.version.contains('-'), "{}", e.version);
    }
}

#[tokio::test]
async fn pin_profile_writes_once_and_reuses() {
    let server = MockServer::start().await;
    serve(&server, FABRIC_PROFILE_ROUTE, FABRIC_PROFILE).await;
    let h = harness(&server);
    let id = profile_id(LoaderKind::Fabric, "0.16.10", "1.20.1");

    let first = h
        .meta
        .pin_profile(LoaderKind::Fabric, "1.20.1", "0.16.10")
        .await
        .unwrap();
    assert_eq!(first.id, id);
    assert_eq!(
        std::fs::read(profile_path(&h.paths, &id)).unwrap(),
        FABRIC_PROFILE
    );
    assert_eq!(requests_to(&server, FABRIC_PROFILE_ROUTE).await, 1);

    let second = h
        .meta
        .pin_profile(LoaderKind::Fabric, "1.20.1", "0.16.10")
        .await
        .unwrap();
    assert_eq!(second.id, id);
    assert_eq!(requests_to(&server, FABRIC_PROFILE_ROUTE).await, 1);
    assert!(load_pinned_profile(&h.paths, &id).unwrap().is_some());
}

#[tokio::test]
async fn pin_profile_refetches_a_corrupt_file() {
    let server = MockServer::start().await;
    serve(&server, FABRIC_PROFILE_ROUTE, FABRIC_PROFILE).await;
    let h = harness(&server);
    let id = profile_id(LoaderKind::Fabric, "0.16.10", "1.20.1");
    let path = profile_path(&h.paths, &id);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, &FABRIC_PROFILE[..FABRIC_PROFILE.len() / 2]).unwrap();

    let profile = h
        .meta
        .pin_profile(LoaderKind::Fabric, "1.20.1", "0.16.10")
        .await
        .unwrap();
    assert_eq!(profile.inherits_from, "1.20.1");
    assert_eq!(std::fs::read(&path).unwrap(), FABRIC_PROFILE);
    assert_eq!(requests_to(&server, FABRIC_PROFILE_ROUTE).await, 1);
    assert!(common::part_files(&h.paths.versions_dir()).is_empty());
}

#[tokio::test]
async fn pin_profile_does_not_write_a_body_that_is_not_a_profile() {
    let server = MockServer::start().await;
    serve(&server, FABRIC_PROFILE_ROUTE, b"{\"id\":").await;
    let h = harness(&server);
    let id = profile_id(LoaderKind::Fabric, "0.16.10", "1.20.1");

    let err = h
        .meta
        .pin_profile(LoaderKind::Fabric, "1.20.1", "0.16.10")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "meta.invalid");
    assert!(!profile_path(&h.paths, &id).exists());
}

#[tokio::test]
async fn loader_lists_use_the_cache_when_offline() {
    let server = MockServer::start().await;
    serve(&server, "/v3/versions/loader/1.20.1", QUILT_LOADERS).await;
    let h = harness(&server);
    let online = h
        .meta
        .loader_versions(LoaderKind::Quilt, "1.20.1")
        .await
        .unwrap();
    drop(server);

    let offline = h
        .meta
        .loader_versions(LoaderKind::Quilt, "1.20.1")
        .await
        .unwrap();
    assert_eq!(offline, online);
}

#[tokio::test]
async fn unsupported_kind_is_rejected() {
    let server = MockServer::start().await;
    let h = harness(&server);
    for kind in [LoaderKind::Vanilla, LoaderKind::Forge, LoaderKind::Neoforge] {
        let err = h.meta.game_versions(kind).await.unwrap_err();
        assert_eq!(err.code(), "install.unsupported_profile");
        let err = h.meta.loader_versions(kind, "1.20.1").await.unwrap_err();
        assert_eq!(err.code(), "install.unsupported_profile");
        let err = h
            .meta
            .pin_profile(kind, "1.20.1", "1.0.0")
            .await
            .unwrap_err();
        assert_eq!(err.code(), "install.unsupported_profile");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}
