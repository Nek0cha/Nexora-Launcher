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
    let id = profile_id(LoaderKind::Fabric, "0.16.10", "1.20.1").unwrap();

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
    let id = profile_id(LoaderKind::Fabric, "0.16.10", "1.20.1").unwrap();
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
    let id = profile_id(LoaderKind::Fabric, "0.16.10", "1.20.1").unwrap();

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
    // Forge と NeoForge は一覧は取れるが、Fabric 型のプロファイルは固定できない
    for kind in [LoaderKind::Vanilla, LoaderKind::Forge, LoaderKind::Neoforge] {
        if kind == LoaderKind::Vanilla {
            let err = h.meta.game_versions(kind).await.unwrap_err();
            assert_eq!(err.code(), "install.unsupported_profile");
            let err = h.meta.loader_versions(kind, "1.20.1").await.unwrap_err();
            assert_eq!(err.code(), "install.unsupported_profile");
        }
        let err = h
            .meta
            .pin_profile(kind, "1.20.1", "1.0.0")
            .await
            .unwrap_err();
        assert_eq!(err.code(), "install.unsupported_profile");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// パスや URL に入る値に、区切り文字や `..` が含まれていれば、ネットワークにもディスクにも触れずに弾く。
#[tokio::test]
async fn unsafe_versions_are_rejected_before_any_request() {
    let server = MockServer::start().await;
    let h = harness(&server);
    for bad in ["", "..", "../x", "a/b", r"a\b", r"..\..\x", "a:b", "1..2"] {
        let err = h
            .meta
            .pin_profile(LoaderKind::Fabric, "1.20.1", bad)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "meta.invalid", "loader {bad:?}");
        let err = h
            .meta
            .pin_profile(LoaderKind::Quilt, bad, "0.28.1")
            .await
            .unwrap_err();
        assert_eq!(err.code(), "meta.invalid", "mc {bad:?}");
        let err = h
            .meta
            .loader_versions(LoaderKind::Fabric, bad)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "meta.invalid", "list {bad:?}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
    // プロファイルもキャッシュも書かれていない（フォルダが無ければ空とみなす）
    for dir in [h.paths.versions_dir(), h.paths.meta_cache_dir()] {
        let entries = std::fs::read_dir(&dir).map_or(0, |d| d.count());
        assert_eq!(entries, 0, "{}", dir.display());
    }
}

// ---- Forge と NeoForge ----

const FORGE_MAVEN: &[u8] = include_bytes!("fixtures/forge/forge-maven-metadata.xml");
const FORGE_PROMOTIONS: &[u8] = include_bytes!("fixtures/forge/forge-promotions_slim.json");
const NEOFORGE_MAVEN: &[u8] = include_bytes!("fixtures/forge/neoforge-maven-metadata.xml");
const NEOFORGE_FORGE_MAVEN: &[u8] =
    include_bytes!("fixtures/forge/neoforge-forge-maven-metadata.xml");

const FORGE_MAVEN_ROUTE: &str = "/net/minecraftforge/forge/maven-metadata.xml";
const PROMOTIONS_ROUTE: &str = "/promotions_slim.json";
const NEOFORGE_MAVEN_ROUTE: &str = "/net/neoforged/neoforge/maven-metadata.xml";
const NEOFORGE_FORGE_MAVEN_ROUTE: &str = "/net/neoforged/forge/maven-metadata.xml";

/// Forge と NeoForge のメタデータを server に向けた LoaderMeta。
fn forge_harness(server: &MockServer) -> Harness {
    let mut h = harness(server);
    h.meta = h.meta.with_forge_urls(
        server.uri(),
        format!("{}{PROMOTIONS_ROUTE}", server.uri()),
        server.uri(),
    );
    h
}

fn flags(entries: &[LoaderVersionEntry]) -> Vec<(&str, bool)> {
    entries
        .iter()
        .map(|e| (e.version.as_str(), e.stable))
        .collect()
}

#[tokio::test]
async fn lists_forge_game_versions() {
    let server = MockServer::start().await;
    serve(&server, FORGE_MAVEN_ROUTE, FORGE_MAVEN).await;
    let h = forge_harness(&server);
    // 順序の当てにならない Maven から、重複なし、新しい順、対応範囲（1.16.5 以降と 1.8.9〜1.12.2、26.x を含む）
    assert_eq!(
        h.meta.game_versions(LoaderKind::Forge).await.unwrap(),
        vec![
            "26.3", "26.2", "26.1.2", "26.1", "1.21.11", "1.21.10", "1.21.8", "1.21.5", "1.21.4",
            "1.21.3", "1.21.1", "1.21", "1.20.6", "1.20.4", "1.20.1", "1.16.5", "1.12.2", "1.11",
            "1.10.2"
        ]
    );
}

#[tokio::test]
async fn lists_forge_versions_newest_first_with_recommended_stable() {
    let server = MockServer::start().await;
    serve(&server, FORGE_MAVEN_ROUTE, FORGE_MAVEN).await;
    // 1.21 には recommended、1.21.4 には latest だけがある
    serve(
        &server,
        PROMOTIONS_ROUTE,
        br#"{"promos":{"1.21-recommended":"51.0.30","1.21-latest":"51.0.33","1.21.4-latest":"54.1.22"}}"#,
    )
    .await;
    let h = forge_harness(&server);

    let v121 = h
        .meta
        .loader_versions(LoaderKind::Forge, "1.21")
        .await
        .unwrap();
    assert_eq!(v121.len(), 12);
    // 数で並べる（51.0.9 のような桁の違いに惑わされない）
    assert_eq!(v121[0].version, "1.21-51.0.33");
    assert_eq!(v121[11].version, "1.21-51.0.21");
    let stable: Vec<_> = v121.iter().filter(|e| e.stable).collect();
    assert_eq!(stable.len(), 1);
    assert_eq!(stable[0].version, "1.21-51.0.30");

    // recommended が無ければ latest
    let v1214 = h
        .meta
        .loader_versions(LoaderKind::Forge, "1.21.4")
        .await
        .unwrap();
    assert_eq!(flags(&v1214), vec![("1.21.4-54.1.22", true)]);
}

#[tokio::test]
async fn forge_versions_with_the_real_promotions_fixture() {
    let server = MockServer::start().await;
    serve(&server, FORGE_MAVEN_ROUTE, FORGE_MAVEN).await;
    serve(&server, PROMOTIONS_ROUTE, FORGE_PROMOTIONS).await;
    let h = forge_harness(&server);
    // 実物の recommended（47.4.10）は切り詰めた Maven に無いので、どれも stable ではない
    let v = h
        .meta
        .loader_versions(LoaderKind::Forge, "1.20.1")
        .await
        .unwrap();
    assert_eq!(flags(&v), vec![("1.20.1-47.3.0", false)]);
    // 旧形式の 1.12.2 も一覧に出る（実物の recommended の 2859 は切り詰めた Maven に無い）。ビルド番号の新しい順
    let v = h
        .meta
        .loader_versions(LoaderKind::Forge, "1.12.2")
        .await
        .unwrap();
    assert_eq!(
        flags(&v),
        vec![
            ("1.12.2-14.23.5.2847", false),
            ("1.12.2-14.23.4.2720-4627", false)
        ]
    );
    // 対応範囲の外（1.13〜1.16.4）は空
    let v = h
        .meta
        .loader_versions(LoaderKind::Forge, "1.14.4")
        .await
        .unwrap();
    assert!(v.is_empty(), "{v:?}");
}

#[tokio::test]
async fn forge_promotions_failure_does_not_hide_versions() {
    let server = MockServer::start().await;
    serve(&server, FORGE_MAVEN_ROUTE, FORGE_MAVEN).await;
    // promotions は 404（未登録）
    let h = forge_harness(&server);
    let v = h
        .meta
        .loader_versions(LoaderKind::Forge, "1.21")
        .await
        .unwrap();
    assert_eq!(v.len(), 12);
    assert!(v.iter().all(|e| !e.stable));
}

#[tokio::test]
async fn neoforge_merges_both_maven_families() {
    let server = MockServer::start().await;
    serve(&server, NEOFORGE_MAVEN_ROUTE, NEOFORGE_MAVEN).await;
    serve(&server, NEOFORGE_FORGE_MAVEN_ROUTE, NEOFORGE_FORGE_MAVEN).await;
    let h = forge_harness(&server);

    // 26.x を含み、21.1.263 のように後ろに並ぶ版も数で並べる
    assert_eq!(
        h.meta.game_versions(LoaderKind::Neoforge).await.unwrap(),
        vec![
            "26.3", "26.2", "26.1.2", "26.1.1", "26.1", "1.21.1", "1.21", "1.20.4", "1.20.2",
            "1.20.1"
        ]
    );

    let old = h
        .meta
        .loader_versions(LoaderKind::Neoforge, "1.20.1")
        .await
        .unwrap();
    assert_eq!(old.len(), 24);
    assert_eq!(old[0].version, "1.20.1-47.1.106");
    assert_eq!(old[23].version, "1.20.1-47.1.5");
    assert!(old.iter().all(|e| e.stable));

    let new = h
        .meta
        .loader_versions(LoaderKind::Neoforge, "1.21.1")
        .await
        .unwrap();
    assert_eq!(
        flags(&new),
        vec![("21.1.264", true), ("21.1.263", true), ("21.1.172", true)]
    );
}

#[tokio::test]
async fn neoforge_26_versions_map_to_their_minecraft_version() {
    let server = MockServer::start().await;
    serve(&server, NEOFORGE_MAVEN_ROUTE, NEOFORGE_MAVEN).await;
    serve(&server, NEOFORGE_FORGE_MAVEN_ROUTE, NEOFORGE_FORGE_MAVEN).await;
    let h = forge_harness(&server);
    let list = |mc: &'static str| {
        let meta = h.meta.clone();
        async move {
            meta.loader_versions(LoaderKind::Neoforge, mc)
                .await
                .unwrap()
        }
    };
    let v = list("26.3").await;
    assert_eq!(v.len(), 10);
    assert_eq!(flags(&v)[0], ("26.3.0.95", true));
    assert_eq!(flags(&list("26.2").await), vec![("26.2.0.93", true)]);
    assert_eq!(flags(&list("26.1.2").await), vec![("26.1.2.122", true)]);
    assert_eq!(
        flags(&list("26.1.1").await),
        vec![("26.1.1.15-beta", false)]
    );
    let v = list("26.1").await;
    assert_eq!(v[0].version, "26.1.0.19-beta");
    assert!(v.iter().all(|e| !e.stable));
    assert!(v.iter().any(|e| e.version == "26.1.0.0-alpha.1+snapshot-1"));
}

#[tokio::test]
async fn forge_26_versions_are_listed() {
    let server = MockServer::start().await;
    serve(&server, FORGE_MAVEN_ROUTE, FORGE_MAVEN).await;
    let h = forge_harness(&server);
    let v = h
        .meta
        .loader_versions(LoaderKind::Forge, "26.3")
        .await
        .unwrap();
    assert_eq!(flags(&v), vec![("26.3-66.0.13", false)]);
    let v = h
        .meta
        .loader_versions(LoaderKind::Forge, "26.1.2")
        .await
        .unwrap();
    assert_eq!(flags(&v), vec![("26.1.2-64.1.5", false)]);
}

#[tokio::test]
async fn neoforge_prerelease_is_not_stable() {
    let server = MockServer::start().await;
    serve(&server, NEOFORGE_MAVEN_ROUTE, NEOFORGE_MAVEN).await;
    serve(&server, NEOFORGE_FORGE_MAVEN_ROUTE, NEOFORGE_FORGE_MAVEN).await;
    let h = forge_harness(&server);
    let v = h
        .meta
        .loader_versions(LoaderKind::Neoforge, "1.20.2")
        .await
        .unwrap();
    assert_eq!(flags(&v), vec![("20.2.3-beta", false)]);
    let v = h
        .meta
        .loader_versions(LoaderKind::Neoforge, "1.21")
        .await
        .unwrap();
    assert_eq!(flags(&v), vec![("21.0.0-beta", false)]);
}
