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

/// dir の下にあるファイルを、すべて返す。
fn files_under(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out
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
    assert!(installer.path.starts_with(s.paths.installers_dir()));
    assert!(installer.path.ends_with(JAR_NAME));
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
    let left = files_under(&s.paths.installers_dir());
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
    let left = files_under(&s.paths.installers_dir());
    assert!(left.is_empty(), "{left:?}");
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
        format!("{}/a//x.jar", s.server.uri()),
        format!("{}/a%2Fb/x.jar", s.server.uri()),
        format!("http://[::1]:{}/x.jar", s.server.address().port()),
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

#[tokio::test]
async fn fetch_installer_keeps_same_named_installers_apart() {
    // Forge と NeoForge の 1.20.1 系は、ファイル名が同じで取得元だけが違う
    const NEO_ROUTE: &str =
        "/releases/net/neoforged/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar";
    let s = setup().await;
    let forge = jar_bytes();
    let neo = b"neoforge installer".to_vec();
    for (route, body) in [(JAR_ROUTE, &forge), (NEO_ROUTE, &neo)] {
        serve(&s.server, route, body).await;
        serve(
            &s.server,
            &format!("{route}.sha1"),
            sha1_hex(body).as_bytes(),
        )
        .await;
    }
    let neo_url = format!("{}{NEO_ROUTE}", s.server.uri());
    let token = CancellationToken::new();

    let a = fetch_installer(&http(), &s.paths, &s.url, &token)
        .await
        .unwrap();
    let b = fetch_installer(&http(), &s.paths, &neo_url, &token)
        .await
        .unwrap();
    assert_ne!(a.path, b.path);
    assert_eq!(std::fs::read(&a.path).unwrap(), forge);
    assert_eq!(std::fs::read(&b.path).unwrap(), neo);
    assert_eq!(b.sha1, sha1_hex(&neo));
    assert_eq!(requests_to(&s.server, JAR_ROUTE).await, 1);
    assert_eq!(requests_to(&s.server, NEO_ROUTE).await, 1);

    // 2 回目はどちらも手元のものを使い、取り違えない
    let a2 = fetch_installer(&http(), &s.paths, &s.url, &token)
        .await
        .unwrap();
    let b2 = fetch_installer(&http(), &s.paths, &neo_url, &token)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&a2.path).unwrap(), forge);
    assert_eq!(std::fs::read(&b2.path).unwrap(), neo);
    assert_eq!(requests_to(&s.server, JAR_ROUTE).await, 1);
    assert_eq!(requests_to(&s.server, NEO_ROUTE).await, 1);
}
