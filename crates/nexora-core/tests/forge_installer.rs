//! Forge と NeoForge のインストーラの取得。ネットワークには出ず、wiremock だけを使う。

mod common;

use std::io::Write;

use nexora_core::install::forge::installer::{fetch_installer, read_entry};
use nexora_core::net::hash::sha1_hex;
use nexora_core::paths::AppPaths;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use zip::write::SimpleFileOptions;

use common::{http, serve};

const JAR_ROUTE: &str = "/net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar";
const JAR_NAME: &str = "forge-1.20.1-47.3.0-installer.jar";

/// install_profile.json と version.json を持つ小さい jar の中身。
fn jar_bytes() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in [
        ("install_profile.json", &b"{}"[..]),
        ("version.json", &b"{\"id\":\"x\"}"[..]),
    ] {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(body).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

struct Setup {
    _temp: tempfile::TempDir,
    paths: AppPaths,
    server: MockServer,
    url: String,
}

async fn setup() -> Setup {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    let server = MockServer::start().await;
    let url = format!("{}{JAR_ROUTE}", server.uri());
    Setup {
        _temp: temp,
        paths,
        server,
        url,
    }
}

/// 指定のパスへのリクエストの数。
async fn requests_to(server: &MockServer, route: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == route)
        .count()
}

#[tokio::test]
async fn fetch_installer_verifies_sha1() {
    let s = setup().await;
    let jar = jar_bytes();
    let sha1 = sha1_hex(&jar);
    serve(&s.server, JAR_ROUTE, &jar).await;
    // 大文字、前後の空白、ハッシュの後ろのファイル名を許す
    let sidecar = format!("  {}  {JAR_NAME}\n", sha1.to_uppercase());
    serve(&s.server, &format!("{JAR_ROUTE}.sha1"), sidecar.as_bytes()).await;

    let installer = fetch_installer(&http(), &s.paths, &s.url, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(installer.path, s.paths.installers_dir().join(JAR_NAME));
    assert_eq!(installer.sha1, sha1);
    assert_eq!(std::fs::read(&installer.path).unwrap(), jar);
    assert_eq!(
        read_entry(&installer.path, "/version.json").unwrap(),
        b"{\"id\":\"x\"}"
    );
}

#[tokio::test]
async fn fetch_installer_rejects_sha1_mismatch_and_leaves_no_file() {
    let s = setup().await;
    serve(&s.server, JAR_ROUTE, &jar_bytes()).await;
    serve(
        &s.server,
        &format!("{JAR_ROUTE}.sha1"),
        sha1_hex(b"something else").as_bytes(),
    )
    .await;

    let err = fetch_installer(&http(), &s.paths, &s.url, &CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "net.hash_mismatch");
    let dir = s.paths.installers_dir();
    let left: Vec<_> = std::fs::read_dir(&dir)
        .map(|it| it.map(|e| e.unwrap().file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "{left:?}");
}

#[tokio::test]
async fn fetch_installer_fails_without_sha1_sidecar() {
    let s = setup().await;
    serve(&s.server, JAR_ROUTE, &jar_bytes()).await;
    Mock::given(method("GET"))
        .and(path(format!("{JAR_ROUTE}.sha1")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&s.server)
        .await;

    let err = fetch_installer(&http(), &s.paths, &s.url, &CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "net.http_status");
    assert!(!s.paths.installers_dir().join(JAR_NAME).exists());
}

#[tokio::test]
async fn fetch_installer_uses_cache() {
    let s = setup().await;
    let jar = jar_bytes();
    serve(&s.server, JAR_ROUTE, &jar).await;
    serve(
        &s.server,
        &format!("{JAR_ROUTE}.sha1"),
        sha1_hex(&jar).as_bytes(),
    )
    .await;

    let token = CancellationToken::new();
    let first = fetch_installer(&http(), &s.paths, &s.url, &token)
        .await
        .unwrap();
    assert_eq!(requests_to(&s.server, JAR_ROUTE).await, 1);
    let before = s.server.received_requests().await.unwrap().len();

    let second = fetch_installer(&http(), &s.paths, &s.url, &token)
        .await
        .unwrap();
    assert_eq!(second.path, first.path);
    assert_eq!(second.sha1, first.sha1);
    assert_eq!(requests_to(&s.server, JAR_ROUTE).await, 1);
    // 照合済みなら .sha1 も取りに行かない
    assert_eq!(s.server.received_requests().await.unwrap().len(), before);
}

#[tokio::test]
async fn fetch_installer_refetches_a_corrupted_cache() {
    let s = setup().await;
    let jar = jar_bytes();
    serve(&s.server, JAR_ROUTE, &jar).await;
    serve(
        &s.server,
        &format!("{JAR_ROUTE}.sha1"),
        sha1_hex(&jar).as_bytes(),
    )
    .await;
    let token = CancellationToken::new();
    let first = fetch_installer(&http(), &s.paths, &s.url, &token)
        .await
        .unwrap();
    std::fs::write(&first.path, b"broken").unwrap();

    let second = fetch_installer(&http(), &s.paths, &s.url, &token)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&second.path).unwrap(), jar);
    assert_eq!(requests_to(&s.server, JAR_ROUTE).await, 2);
}

#[tokio::test]
async fn fetch_installer_rejects_unsafe_file_names() {
    let s = setup().await;
    for url in [
        format!("{}/", s.server.uri()),
        format!("{}/a/..", s.server.uri()),
        format!("{}/a/%2e%2e", s.server.uri()),
        format!("{}/a/x%5C..%5Cevil.jar", s.server.uri()),
        "not a url".to_string(),
    ] {
        let err = fetch_installer(&http(), &s.paths, &url, &CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(err.code(), "meta.invalid", "{url}");
    }
    assert!(s.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn fetch_installer_honours_cancellation() {
    let s = setup().await;
    let token = CancellationToken::new();
    token.cancel();
    let err = fetch_installer(&http(), &s.paths, &s.url, &token)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "task.cancelled");
    assert!(s.server.received_requests().await.unwrap().is_empty());
}
