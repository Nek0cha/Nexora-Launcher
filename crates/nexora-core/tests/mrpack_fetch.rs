//! `.mrpack` のファイルを、許可したホストからだけ取得して検証する。
mod common;

use std::time::Duration;

use nexora_core::mrpack::fetch::{download_pack_files, staged_path};
use nexora_core::mrpack::manifest::PackFile;
use nexora_core::net::hash::{hex, sha1_hex};
use nexora_core::net::{HostPolicy, Http};
use sha2::{Digest, Sha512};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const LIMIT: u64 = 1024 * 1024;

fn sha512_hex(bytes: &[u8]) -> String {
    hex(&Sha512::digest(bytes))
}

fn pack_file(name: &str, body: &[u8], downloads: Vec<String>) -> PackFile {
    PackFile {
        path: format!("mods/{name}"),
        sha1: sha1_hex(body),
        sha512: sha512_hex(body),
        env_client: None,
        env_server: None,
        downloads,
        file_size: body.len() as u64,
    }
}

/// 127.0.0.1 だけを許す、テスト用の方針の Http。
fn test_http(max_redirects: usize) -> (Http, HostPolicy) {
    let policy = HostPolicy::lenient_for_tests(&["127.0.0.1"]);
    let http = Http::restricted(&policy, max_redirects)
        .unwrap()
        .with_retry_base(Duration::from_millis(1));
    (http, policy)
}

async fn serve_bytes(server: &MockServer, route: &str, body: &[u8]) {
    Mock::given(method("GET"))
        .and(path(route.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(server)
        .await;
}

async fn run(
    server_files: &[PackFile],
    http: &Http,
    policy: &HostPolicy,
    staging: &std::path::Path,
) -> Result<(), nexora_core::error::CoreError> {
    let files: Vec<&PackFile> = server_files.iter().collect();
    download_pack_files(
        http,
        policy,
        &files,
        staging,
        LIMIT,
        &CancellationToken::new(),
        |_, _| {},
    )
    .await
}

#[tokio::test]
async fn downloads_and_verifies_both_hashes() {
    let server = MockServer::start().await;
    serve_bytes(&server, "/a.jar", b"hello pack").await;
    let dir = tempfile::tempdir().unwrap();
    let (http, policy) = test_http(3);
    let files = [pack_file(
        "a.jar",
        b"hello pack",
        vec![format!("{}/a.jar", server.uri())],
    )];

    run(&files, &http, &policy, dir.path()).await.unwrap();

    assert_eq!(
        std::fs::read(staged_path(dir.path(), 0)).unwrap(),
        b"hello pack"
    );
}

#[tokio::test]
async fn sha512_mismatch_is_rejected_even_when_sha1_matches() {
    let server = MockServer::start().await;
    serve_bytes(&server, "/a.jar", b"hello pack").await;
    let dir = tempfile::tempdir().unwrap();
    let (http, policy) = test_http(3);
    let mut file = pack_file(
        "a.jar",
        b"hello pack",
        vec![format!("{}/a.jar", server.uri())],
    );
    file.sha512 = "0".repeat(128);

    let err = run(&[file], &http, &policy, dir.path()).await.unwrap_err();

    assert_eq!(err.code(), "net.hash_mismatch");
    assert!(!staged_path(dir.path(), 0).exists());
}

#[tokio::test]
async fn a_declared_size_that_does_not_match_the_body_is_rejected() {
    let server = MockServer::start().await;
    serve_bytes(&server, "/a.jar", b"hello pack, longer than declared").await;
    let dir = tempfile::tempdir().unwrap();
    let (http, policy) = test_http(3);
    let body = b"hello pack, longer than declared";
    let mut file = pack_file("a.jar", body, vec![format!("{}/a.jar", server.uri())]);
    file.file_size = 3;

    let err = run(&[file], &http, &policy, dir.path()).await.unwrap_err();

    assert_eq!(err.code(), "net.hash_mismatch");
    assert!(!staged_path(dir.path(), 0).exists());
}

#[tokio::test]
async fn a_file_above_the_size_limit_is_refused_without_any_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let (http, policy) = test_http(3);
    let mut file = pack_file("a.jar", b"x", vec![format!("{}/a.jar", server.uri())]);
    file.file_size = LIMIT + 1;

    let err = run(&[file], &http, &policy, dir.path()).await.unwrap_err();

    assert_eq!(err.code(), "mrpack.limit");
}

#[tokio::test]
async fn falls_back_to_the_next_download_url() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/missing.jar"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    serve_bytes(&server, "/ok.jar", b"hello pack").await;
    let dir = tempfile::tempdir().unwrap();
    let (http, policy) = test_http(3);
    let files = [pack_file(
        "a.jar",
        b"hello pack",
        vec![
            format!("{}/missing.jar", server.uri()),
            format!("{}/ok.jar", server.uri()),
        ],
    )];

    run(&files, &http, &policy, dir.path()).await.unwrap();

    assert!(staged_path(dir.path(), 0).exists());
}

#[tokio::test]
async fn a_url_outside_the_allowed_hosts_is_never_requested() {
    let dir = tempfile::tempdir().unwrap();
    let (http, policy) = test_http(3);
    let files = [pack_file(
        "a.jar",
        b"x",
        vec!["http://evil.invalid/a.jar".to_string()],
    )];

    let err = run(&files, &http, &policy, dir.path()).await.unwrap_err();

    assert_eq!(err.code(), "meta.invalid");
}

async fn redirect_chain(server: &MockServer, hops: usize, last: &str) {
    for i in 0..hops {
        let next = if i + 1 == hops {
            last.to_string()
        } else {
            format!("{}/r{}", server.uri(), i + 1)
        };
        Mock::given(method("GET"))
            .and(path(format!("/r{i}")))
            .respond_with(ResponseTemplate::new(302).insert_header("location", next.as_str()))
            .mount(server)
            .await;
    }
}

#[tokio::test]
async fn redirects_are_bounded() {
    let server = MockServer::start().await;
    serve_bytes(&server, "/final.jar", b"hello pack").await;
    let last = format!("{}/final.jar", server.uri());
    let dir = tempfile::tempdir().unwrap();
    let (http, policy) = test_http(3);

    // 3 回のリダイレクトは通る
    redirect_chain(&server, 3, &last).await;
    let ok = [pack_file(
        "a.jar",
        b"hello pack",
        vec![format!("{}/r0", server.uri())],
    )];
    run(&ok, &http, &policy, dir.path()).await.unwrap();

    // 4 回目は止める
    server.reset().await;
    serve_bytes(&server, "/final.jar", b"hello pack").await;
    redirect_chain(&server, 4, &last).await;
    let dir2 = tempfile::tempdir().unwrap();
    let too_many = [pack_file(
        "a.jar",
        b"hello pack",
        vec![format!("{}/r0", server.uri())],
    )];
    assert!(run(&too_many, &http, &policy, dir2.path()).await.is_err());
    assert!(!staged_path(dir2.path(), 0).exists());
}

#[tokio::test]
async fn a_redirect_to_a_disallowed_host_is_not_followed() {
    let server = MockServer::start().await;
    serve_bytes(&server, "/final.jar", b"hello pack").await;
    // 127.0.0.1 のサーバーを `localhost` の名前で指す。許可は 127.0.0.1 だけ
    let port = server.address().port();
    let elsewhere = format!("http://localhost:{port}/final.jar");
    redirect_chain(&server, 1, &elsewhere).await;
    let dir = tempfile::tempdir().unwrap();
    let (http, policy) = test_http(3);
    let files = [pack_file(
        "a.jar",
        b"hello pack",
        vec![format!("{}/r0", server.uri())],
    )];

    assert!(run(&files, &http, &policy, dir.path()).await.is_err());
    assert!(!staged_path(dir.path(), 0).exists());
}

#[tokio::test]
async fn cancelling_stops_the_download() {
    let server = MockServer::start().await;
    serve_bytes(&server, "/a.jar", b"hello pack").await;
    let dir = tempfile::tempdir().unwrap();
    let (http, policy) = test_http(3);
    let files = [pack_file(
        "a.jar",
        b"hello pack",
        vec![format!("{}/a.jar", server.uri())],
    )];
    let token = CancellationToken::new();
    token.cancel();
    let refs: Vec<&PackFile> = files.iter().collect();

    let err = download_pack_files(&http, &policy, &refs, dir.path(), LIMIT, &token, |_, _| {})
        .await
        .unwrap_err();

    assert_eq!(err.code(), "task.cancelled");
}
