mod common;

use std::sync::{Arc, Mutex};

use nexora_core::net::{Depth, FetchPhase, fetch_all, fetch_all_with};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use common::{http, item, part_files, serve};

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
async fn reports_verification_before_downloading() {
    let server = MockServer::start().await;
    let temp = tempfile::tempdir().unwrap();
    serve(&server, "/a.jar", b"alpha").await;
    // b は手元に正しいものがあり、確認だけで済む
    std::fs::write(temp.path().join("b.jar"), b"bravo!").unwrap();
    let items = vec![
        item(&server, "/a.jar", temp.path().join("a.jar"), b"alpha"),
        item(&server, "/b.jar", temp.path().join("b.jar"), b"bravo!"),
    ];
    #[derive(Debug, PartialEq)]
    enum Event {
        Phase(FetchPhase),
        Bytes(u64, u64),
    }
    let events = Mutex::new(Vec::new());
    fetch_all_with(
        &http(),
        items,
        Depth::Full,
        8,
        &CancellationToken::new(),
        |p| events.lock().unwrap().push(Event::Phase(p)),
        |d, t| events.lock().unwrap().push(Event::Bytes(d, t)),
    )
    .await
    .unwrap();
    let events = events.into_inner().unwrap();
    assert_eq!(
        events.first(),
        Some(&Event::Phase(FetchPhase::Verifying {
            checked: 0,
            total: 2
        }))
    );
    let switch = events
        .iter()
        .position(|e| *e == Event::Phase(FetchPhase::Downloading))
        .expect("switches to downloading");
    // 確認の最後は全件で、取得の進捗はその後にだけ来る（b の分は数えない）
    assert_eq!(
        events[switch - 1],
        Event::Phase(FetchPhase::Verifying {
            checked: 2,
            total: 2
        })
    );
    assert!(
        events[..switch]
            .iter()
            .all(|e| matches!(e, Event::Phase(FetchPhase::Verifying { .. })))
    );
    assert_eq!(events[switch + 1], Event::Bytes(0, 5));
    assert_eq!(events.last(), Some(&Event::Bytes(5, 5)));
}

#[tokio::test]
async fn nothing_to_verify_still_switches_to_downloading() {
    let phases = Mutex::new(Vec::new());
    fetch_all_with(
        &http(),
        Vec::new(),
        Depth::Full,
        8,
        &CancellationToken::new(),
        |p| phases.lock().unwrap().push(p),
        |_, _| {},
    )
    .await
    .unwrap();
    assert_eq!(
        phases.into_inner().unwrap(),
        vec![
            FetchPhase::Verifying {
                checked: 0,
                total: 0
            },
            FetchPhase::Downloading
        ]
    );
}

/// 取り消しは、手元のファイルの確認（sha1 を読み直す）の途中でもすぐに効く。
#[tokio::test]
async fn cancel_during_verification_returns_promptly() {
    use nexora_core::net::{DownloadItem, hash::sha1_hex};
    const FILES: usize = 4000;
    let temp = tempfile::tempdir().unwrap();
    let mut items = Vec::with_capacity(FILES);
    for i in 0..FILES {
        let body = format!("file {i:08}").repeat(1024).into_bytes();
        let path = temp.path().join(format!("{:02x}/{i}", i % 64));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &body).unwrap();
        items.push(DownloadItem {
            // 確認で済むため、取得には出ない
            url: "http://127.0.0.1:9/never".into(),
            path,
            size: Some(body.len() as u64),
            sha1: Some(sha1_hex(&body)),
            executable: false,
        });
    }
    let token = CancellationToken::new();
    let last = Mutex::new(None);
    let cancelled_at = Mutex::new(None);
    let cancel = token.clone();
    let client = http();
    // 確認を始める最初の通知の中から取り消す。速い環境では確認が数十 ms で終わるため、途中の通知を待つと取り消しが間に合わない
    let result = fetch_all_with(
        &client,
        items,
        Depth::Full,
        8,
        &token,
        |p| {
            if let FetchPhase::Verifying { checked, total } = p {
                *last.lock().unwrap() = Some((checked, total));
                if total > 0 && cancelled_at.lock().unwrap().is_none() {
                    *cancelled_at.lock().unwrap() = Some(std::time::Instant::now());
                    cancel.cancel();
                }
            }
        },
        |_, _| {},
    )
    .await;
    let returned_at = std::time::Instant::now();
    assert_eq!(result.unwrap_err().code(), "task.cancelled");
    let cancelled_at = cancelled_at
        .into_inner()
        .unwrap()
        .expect("verification reported progress before it finished");
    let waited = returned_at.saturating_duration_since(cancelled_at);
    assert!(
        waited < std::time::Duration::from_secs(1),
        "cancel took {waited:?}"
    );
    let (_, total) = last.into_inner().unwrap().unwrap();
    assert_eq!(total, FILES as u64);
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
    assert!(part_files(temp.path()).is_empty());
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
    assert!(part_files(temp.path()).is_empty());
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_fetches_of_the_same_files_do_not_collide() {
    let server = MockServer::start().await;
    let temp = tempfile::tempdir().unwrap();
    // 書き込みが重なるように、大きめの本文を少し遅らせて返す
    let bodies: Vec<Vec<u8>> = (0..6u8)
        .map(|i| {
            (0..512 * 1024)
                .map(|j| (j as u8).wrapping_mul(i + 1))
                .collect()
        })
        .collect();
    for (i, body) in bodies.iter().enumerate() {
        Mock::given(method("GET"))
            .and(path(format!("/f{i}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(body.clone())
                    .set_delay(std::time::Duration::from_millis(50)),
            )
            .mount(&server)
            .await;
    }
    let items = |range: std::ops::Range<usize>| {
        range
            .map(|i| {
                item(
                    &server,
                    &format!("/f{i}"),
                    temp.path().join(format!("shared/f{i}")),
                    &bodies[i],
                )
            })
            .collect::<Vec<_>>()
    };
    let client = http();
    let token = CancellationToken::new();
    // 0..4 と 2..6 が重なる。加えて、まったく同じ組み合わせも同時に取得する
    let (a, b, c) = tokio::join!(
        fetch_all(&client, items(0..4), Depth::Full, 8, &token, |_, _| {}),
        fetch_all(&client, items(2..6), Depth::Full, 8, &token, |_, _| {}),
        fetch_all(&client, items(0..6), Depth::Full, 8, &token, |_, _| {}),
    );
    a.unwrap();
    b.unwrap();
    c.unwrap();
    for (i, body) in bodies.iter().enumerate() {
        let path = temp.path().join(format!("shared/f{i}"));
        assert_eq!(
            nexora_core::net::hash::sha1_file(&path).unwrap(),
            nexora_core::net::hash::sha1_hex(body),
            "f{i}"
        );
    }
    assert_eq!(part_files(temp.path()), Vec::<std::path::PathBuf>::new());
    // ロックを待った側は確かめ直して取得を省くので、各ファイルの取得は 1 回だけ
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        bodies.len()
    );
}

/// 計測用（通常は実行しない）。利用者の `shared/` に近い数と大きさ（既定は 6,600 ファイル・計 600 MB）の
/// ファイルを作り、Depth::Full と Depth::Quick の確認にかかる時間を出す。
/// `NEXORA_BENCH_FILES` と `NEXORA_BENCH_MB` で数と大きさを変えられる。
/// 実行例: `cargo test -p nexora-core --test net_download -- --ignored --nocapture verification_timing`
#[test]
#[ignore = "benchmark; run by hand"]
fn verification_timing() {
    use nexora_core::net::{DownloadItem, hash::sha1_hex, needs_download};
    let files: usize = std::env::var("NEXORA_BENCH_FILES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6600);
    let mb: usize = std::env::var("NEXORA_BENCH_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600);
    let temp = tempfile::tempdir().unwrap();
    let each = mb * 1024 * 1024 / files;
    let mut items = Vec::with_capacity(files);
    for i in 0..files {
        let mut body = vec![0u8; each];
        body[..8].copy_from_slice(&(i as u64).to_le_bytes());
        let path = temp.path().join(format!("{:02x}/{i}", i % 256));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &body).unwrap();
        items.push(DownloadItem {
            url: String::new(),
            path,
            size: Some(each as u64),
            sha1: Some(sha1_hex(&body)),
            executable: false,
        });
    }
    for depth in [Depth::Full, Depth::Full, Depth::Quick] {
        let started = std::time::Instant::now();
        for item in &items {
            assert!(!needs_download(item, depth).unwrap());
        }
        println!(
            "{files} files, {mb} MB, {depth:?}: {} ms (profile: {})",
            started.elapsed().as_millis(),
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
        );
    }
}

/// 1 件の失敗で止めるとき、ほかの取得の書き込みや作成が終わってから返る。
/// 返った直後に呼び出し側が置き場所を消しても、消せて、あとから作り直されない。
#[tokio::test]
async fn failure_waits_for_in_flight_downloads_before_returning() {
    let server = MockServer::start().await;
    let big: Vec<u8> = (0..2_000_000u32).map(|i| (i % 251) as u8).collect();
    serve(&server, "/big", &big).await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404).set_delay(std::time::Duration::from_millis(5)))
        .mount(&server)
        .await;
    let client = http();
    for round in 0..20 {
        let temp = tempfile::tempdir().unwrap();
        let staging = temp.path().join("staging").join("op");
        let mut items: Vec<_> = (0..6)
            .map(|i| {
                item(
                    &server,
                    "/big",
                    staging.join("new").join(format!("{i}.jar")),
                    &big,
                )
            })
            .collect();
        items.push(item(
            &server,
            "/missing",
            staging.join("new").join("m.jar"),
            b"m",
        ));
        let token = CancellationToken::new();
        let err = fetch_all(&client, items, Depth::Quick, 4, &token, |_, _| {})
            .await
            .unwrap_err();
        assert_eq!(err.code(), "net.http_status", "round {round}");
        std::fs::remove_dir_all(&staging)
            .unwrap_or_else(|e| panic!("round {round}: staging could not be removed: {e}"));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!staging.exists(), "round {round}: staging was recreated");
        // 取り消しで token そのものは取り消されない（呼び出し側のもの）
        assert!(!token.is_cancelled());
    }
}
