mod common;

use nexora_core::meta::mojang::manifest::{MojangMeta, version_json_path};
use nexora_core::net::hash::sha1_hex;
use nexora_core::paths::AppPaths;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use common::{http, serve};

const VERSION: &str = r#"{"id":"1.99-test","type":"release","mainClass":"Main",
"assetIndex":{"id":"t","sha1":"0000000000000000000000000000000000000000","size":2,"totalSize":0,"url":"http://x/i.json"},
"downloads":{"client":{"sha1":"0000000000000000000000000000000000000000","size":1,"url":"http://x/c.jar"}},
"libraries":[]}"#;

fn manifest(server: &MockServer, version_sha1: &str) -> String {
    format!(
        r#"{{"latest":{{"release":"1.99-test","snapshot":"1.99-test"}},"versions":[
{{"id":"1.99-test","type":"release","url":"{u}/v.json","time":"t","releaseTime":"2030-01-01T00:00:00+00:00","sha1":"{version_sha1}","complianceLevel":1}},
{{"id":"1.8.9","type":"release","url":"{u}/old.json","time":"t","releaseTime":"2015-12-03T09:24:39+00:00","sha1":"{version_sha1}","complianceLevel":0}}]}}"#,
        u = server.uri()
    )
}

fn setup() -> (tempfile::TempDir, AppPaths) {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    paths.ensure_dirs().unwrap();
    (temp, paths)
}

#[tokio::test]
async fn manifest_uses_etag_on_second_fetch() {
    let server = MockServer::start().await;
    let (_t, paths) = setup();
    let body = manifest(&server, &sha1_hex(VERSION.as_bytes()));
    Mock::given(method("GET"))
        .and(path("/manifest.json"))
        .and(header("if-none-match", "\"abc\""))
        .respond_with(ResponseTemplate::new(304))
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/manifest.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(body)
                .insert_header("ETag", "\"abc\""),
        )
        .expect(1)
        .mount(&server)
        .await;
    let meta = MojangMeta::new(
        http(),
        paths.clone(),
        format!("{}/manifest.json", server.uri()),
    );
    let first = meta.manifest().await.unwrap();
    let second = meta.manifest().await.unwrap();
    assert_eq!(first.versions.len(), 2);
    assert_eq!(second.versions.len(), 2);
}

#[tokio::test]
async fn manifest_falls_back_to_cache_when_offline() {
    let server = MockServer::start().await;
    let (_t, paths) = setup();
    serve(&server, "/manifest.json", manifest(&server, "x").as_bytes()).await;
    MojangMeta::new(
        http(),
        paths.clone(),
        format!("{}/manifest.json", server.uri()),
    )
    .manifest()
    .await
    .unwrap();
    let offline = MojangMeta::new(http(), paths.clone(), "http://127.0.0.1:9/manifest.json");
    assert_eq!(offline.manifest().await.unwrap().versions.len(), 2);
}

#[tokio::test]
async fn manifest_without_cache_reports_offline() {
    let (_t, paths) = setup();
    let offline = MojangMeta::new(http(), paths, "http://127.0.0.1:9/manifest.json");
    assert_eq!(offline.manifest().await.unwrap_err().code(), "net.offline");
}

#[tokio::test]
async fn corrupt_manifest_is_invalid_metadata() {
    let server = MockServer::start().await;
    let (_t, paths) = setup();
    serve(&server, "/manifest.json", b"{not json").await;
    let meta = MojangMeta::new(http(), paths, format!("{}/manifest.json", server.uri()));
    assert_eq!(meta.manifest().await.unwrap_err().code(), "meta.invalid");
}

#[tokio::test]
async fn pin_version_stores_verified_json() {
    let server = MockServer::start().await;
    let (_t, paths) = setup();
    serve(
        &server,
        "/manifest.json",
        manifest(&server, &sha1_hex(VERSION.as_bytes())).as_bytes(),
    )
    .await;
    serve(&server, "/v.json", VERSION.as_bytes()).await;
    let meta = MojangMeta::new(
        http(),
        paths.clone(),
        format!("{}/manifest.json", server.uri()),
    );
    let v = meta
        .pin_version("1.99-test", &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(v.id, "1.99-test");
    assert!(version_json_path(&paths, "1.99-test").is_file());
}

#[tokio::test]
async fn pin_unknown_version_is_not_found() {
    let server = MockServer::start().await;
    let (_t, paths) = setup();
    serve(&server, "/manifest.json", manifest(&server, "x").as_bytes()).await;
    let meta = MojangMeta::new(http(), paths, format!("{}/manifest.json", server.uri()));
    let err = meta
        .pin_version("9.9.9", &CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "meta.version_not_found");
}

#[tokio::test]
async fn cached_manifest_is_used_for_5xx_but_not_4xx() {
    let server = MockServer::start().await;
    let (_t, paths) = setup();
    serve(&server, "/manifest.json", manifest(&server, "x").as_bytes()).await;
    let meta = MojangMeta::new(http(), paths, format!("{}/manifest.json", server.uri()));
    meta.manifest().await.unwrap();
    server.reset().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    assert_eq!(meta.manifest().await.unwrap().versions.len(), 2);
    server.reset().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    assert_eq!(meta.manifest().await.unwrap_err().code(), "net.http_status");
}

#[tokio::test]
async fn fresh_manifest_without_etag_removes_old_validator() {
    let server = MockServer::start().await;
    let (_t, paths) = setup();
    let body = manifest(&server, "x");
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(body.clone())
                .insert_header("ETag", "\"old\""),
        )
        .mount(&server)
        .await;
    let meta = MojangMeta::new(
        http(),
        paths.clone(),
        format!("{}/manifest.json", server.uri()),
    );
    meta.manifest().await.unwrap();
    let etag_path = paths.meta_cache_dir().join("version_manifest_v2.json.etag");
    assert!(etag_path.is_file());
    server.reset().await;
    serve(&server, "/manifest.json", body.as_bytes()).await;
    meta.manifest().await.unwrap();
    assert!(!etag_path.exists());
}

#[tokio::test]
async fn pin_version_rejects_hash_mismatch_without_leaving_files() {
    let server = MockServer::start().await;
    let (_t, paths) = setup();
    serve(
        &server,
        "/manifest.json",
        manifest(&server, &"0".repeat(40)).as_bytes(),
    )
    .await;
    serve(&server, "/v.json", VERSION.as_bytes()).await;
    let meta = MojangMeta::new(
        http(),
        paths.clone(),
        format!("{}/manifest.json", server.uri()),
    );
    let err = meta
        .pin_version("1.99-test", &CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "net.hash_mismatch");
    let path = version_json_path(&paths, "1.99-test");
    assert!(!path.exists());
    assert!(!nexora_core::net::part_path(&path).exists());
}

#[tokio::test]
async fn pin_asset_index_verifies_and_reuses_local_copy() {
    use nexora_core::meta::mojang::manifest::{
        asset_index_path, load_pinned_asset_index, load_pinned_version,
    };
    use nexora_core::meta::mojang::version::VersionJson;

    let server = MockServer::start().await;
    let (_t, paths) = setup();
    assert!(load_pinned_version(&paths, "missing").unwrap().is_none());
    assert!(load_pinned_asset_index(&paths, "t").unwrap().is_none());
    let body = br#"{"objects":{}}"#;
    let mut version: VersionJson = serde_json::from_str(VERSION).unwrap();
    version.asset_index.url = format!("{}/index.json", server.uri());
    version.asset_index.sha1 = sha1_hex(body);
    version.asset_index.size = body.len() as u64;
    Mock::given(method("GET"))
        .and(path("/index.json"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .expect(1)
        .mount(&server)
        .await;
    let meta = MojangMeta::new(http(), paths.clone(), "http://127.0.0.1:9/unused");
    assert!(
        meta.pin_asset_index(&version, &CancellationToken::new())
            .await
            .unwrap()
            .objects
            .is_empty()
    );
    assert_eq!(std::fs::read(asset_index_path(&paths, "t")).unwrap(), body);
    assert!(load_pinned_asset_index(&paths, "t").unwrap().is_some());
    assert!(
        meta.pin_asset_index(&version, &CancellationToken::new())
            .await
            .unwrap()
            .objects
            .is_empty()
    );
}
