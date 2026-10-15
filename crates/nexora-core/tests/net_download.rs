mod common;

use std::sync::{Arc, Mutex};

use nexora_core::net::{Depth, fetch_all, part_path};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use common::{http, item, serve};

#[tokio::test]
async fn concurrency_is_limited_to_sixteen() {
    let server = MockServer::start().await;
    let temp = tempfile::tempdir().unwrap();
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(b"x".to_vec())
                .set_delay(std::time::Duration::from_secs(5)),
        )
        .mount(&server)
        .await;
    let items = (0..32)
        .map(|i| {
            item(
                &server,
                &format!("/{i}"),
                temp.path().join(i.to_string()),
                b"x",
            )
        })
        .collect();
    let token = CancellationToken::new();
    let cancel = token.clone();
    let observe = async {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while server.received_requests().await.unwrap().len() < 16 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let count = server.received_requests().await.unwrap().len();
        cancel.cancel();
        count
    };
    let client = http();
    let (result, count) = tokio::join!(
        fetch_all(&client, items, Depth::Full, 32, &token, |_, _| {}),
        observe
    );
    assert_eq!(result.unwrap_err().code(), "task.cancelled");
    assert_eq!(count, 16);
}

#[tokio::test]
async fn conditional_get_preserves_etag_and_handles_not_modified() {
    use nexora_core::net::Conditional;
    use wiremock::matchers::header;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/manifest"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304))
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/manifest"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_bytes(b"manifest".to_vec()),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = http();
    let url = format!("{}/manifest", server.uri());
    let Conditional::Fresh { body, etag } = client.get_conditional(&url, None).await.unwrap()
    else {
        panic!("expected fresh metadata");
    };
    assert_eq!(body, b"manifest");
    assert_eq!(etag.as_deref(), Some("\"v1\""));
    assert!(matches!(
        client.get_conditional(&url, etag.as_deref()).await.unwrap(),
        Conditional::NotModified
    ));
}

#[tokio::test]
async fn downloads_and_verifies_files() {
    let server = MockServer::start().await;
    let temp = tempfile::tempdir().unwrap();
    serve(&server, "/a.jar", b"alpha").await;
    serve(&server, "/b.jar", b"bravo!").await;
    let items = vec![
        item(&server, "/a.jar", temp.path().join("x/a.jar"), b"alpha"),
        item(&server, "/b.jar", temp.path().join("y/b.jar"), b"bravo!"),
    ];
    let progress = Arc::new(Mutex::new(Vec::new()));
    let recorder = progress.clone();
    fetch_all(
        &http(),
        items,
        Depth::Full,
        8,
        &CancellationToken::new(),
        move |d, t| recorder.lock().unwrap().push((d, t)),
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(temp.path().join("x/a.jar")).unwrap(),
        b"alpha"
    );
    assert_eq!(
        std::fs::read(temp.path().join("y/b.jar")).unwrap(),
        b"bravo!"
    );
    let progress = progress.lock().unwrap();
    assert_eq!(progress.first(), Some(&(0, 11)));
    assert_eq!(progress.last(), Some(&(11, 11)));
}

#[tokio::test]
async fn retries_transient_server_errors() {
    let server = MockServer::start().await;
    let temp = tempfile::tempdir().unwrap();
    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .with_priority(1)
        .mount(&server)
        .await;
    serve(&server, "/flaky", b"ok").await;
    let items = vec![item(&server, "/flaky", temp.path().join("f"), b"ok")];
    fetch_all(
        &http(),
        items,
        Depth::Full,
        1,
        &CancellationToken::new(),
        |_, _| {},
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(temp.path().join("f")).unwrap(), b"ok");
}

#[tokio::test]
async fn hash_mismatch_gives_up_after_three_retries_and_cleans_up() {
    let server = MockServer::start().await;
    let temp = tempfile::tempdir().unwrap();
    serve(&server, "/bad", b"wrong").await;
    let mut bad = item(&server, "/bad", temp.path().join("bad"), b"right");
    bad.size = Some(5);
    let err = fetch_all(
        &http(),
        vec![bad],
        Depth::Full,
        1,
        &CancellationToken::new(),
        |_, _| {},
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "net.hash_mismatch");
    assert_eq!(server.received_requests().await.unwrap().len(), 4);
    assert!(!temp.path().join("bad").exists());
    assert!(!part_path(&temp.path().join("bad")).exists());
}

#[tokio::test]
async fn not_found_is_not_retried() {
    let server = MockServer::start().await;
    let temp = tempfile::tempdir().unwrap();
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let items = vec![item(&server, "/missing", temp.path().join("m"), b"x")];
    let err = fetch_all(
        &http(),
        items,
        Depth::Full,
        1,
        &CancellationToken::new(),
        |_, _| {},
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "net.http_status");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn quick_depth_trusts_size_but_full_depth_checks_hash() {
    let server = MockServer::start().await;
    let temp = tempfile::tempdir().unwrap();
    serve(&server, "/lib.jar", b"genuine").await;
    let dest = temp.path().join("lib.jar");
    std::fs::write(&dest, b"corrupt").unwrap(); // 同じ 7 バイト
    let it = item(&server, "/lib.jar", dest.clone(), b"genuine");

    fetch_all(
        &http(),
        vec![it.clone()],
        Depth::Quick,
        1,
        &CancellationToken::new(),
        |_, _| {},
    )
    .await
    .unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 0);
    assert_eq!(std::fs::read(&dest).unwrap(), b"corrupt");

    fetch_all(
        &http(),
        vec![it],
        Depth::Full,
        1,
        &CancellationToken::new(),
        |_, _| {},
    )
    .await
    .unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert_eq!(std::fs::read(&dest).unwrap(), b"genuine");
}

#[tokio::test]
async fn cancelled_download_leaves_no_partial_file() {
    let server = MockServer::start().await;
    let temp = tempfile::tempdir().unwrap();
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(b"slow".to_vec())
                .set_delay(std::time::Duration::from_secs(5)),
        )
        .mount(&server)
        .await;
    let token = CancellationToken::new();
    let cancel = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        cancel.cancel();
    });
    let dest = temp.path().join("slow");
    let items = vec![item(&server, "/slow", dest.clone(), b"slow")];
    let err = fetch_all(&http(), items, Depth::Full, 1, &token, |_, _| {})
        .await
        .unwrap_err();
    assert_eq!(err.code(), "task.cancelled");
    assert!(!dest.exists());
    assert!(!part_path(&dest).exists());
}

#[tokio::test]
async fn unreachable_host_is_offline() {
    let temp = tempfile::tempdir().unwrap();
    let items = vec![nexora_core::net::DownloadItem {
        url: "http://127.0.0.1:9/nothing".into(),
        path: temp.path().join("n"),
        size: Some(1),
        sha1: None,
        executable: false,
    }];
    let err = fetch_all(
        &http(),
        items,
        Depth::Full,
        1,
        &CancellationToken::new(),
        |_, _| {},
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "net.offline");
}

#[cfg(unix)]
#[tokio::test]
async fn executable_items_get_the_execute_bit() {
    use std::os::unix::fs::PermissionsExt;
    let server = MockServer::start().await;
    let temp = tempfile::tempdir().unwrap();
    serve(&server, "/java", b"#!/bin/sh\n").await;
    let mut it = item(
        &server,
        "/java",
        temp.path().join("bin/java"),
        b"#!/bin/sh\n",
    );
    it.executable = true;
    fetch_all(
        &http(),
        vec![it],
        Depth::Full,
        1,
        &CancellationToken::new(),
        |_, _| {},
    )
    .await
    .unwrap();
    let mode = std::fs::metadata(temp.path().join("bin/java"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o111, 0o111);
}
