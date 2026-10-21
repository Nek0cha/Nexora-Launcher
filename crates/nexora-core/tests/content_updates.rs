//! MOD の更新の確認（対象の選び方、キャッシュ、通信の回数、オフライン）。
mod common;

use common::content::Harness;
use nexora_core::net::hash::sha1_hex;

const CHECK_ROUTE: &str = "/version_files/update";

#[tokio::test]
async fn updates_excludes_pinned_and_same_version_rows() {
    let h = Harness::new().await;
    h.seed("a.jar", b"a1", "pa", "a1");
    h.seed("pinned.jar", b"p1", "pp", "p1");
    h.seed("same.jar", b"s1", "ps", "s1");
    h.seed("none.jar", b"n1", "pn", "n1");
    h.pin("pinned.jar");
    h.serve_updates(vec![
        (sha1_hex(b"a1"), h.version_json("pa", "a2", "a2.jar", b"a2")),
        (sha1_hex(b"p1"), h.version_json("pp", "p2", "p2.jar", b"p2")),
        // 応答の版が今の版と同じなら、更新ではない
        (sha1_hex(b"s1"), h.version_json("ps", "s1", "s1.jar", b"s1")),
    ])
    .await;

    let check = h
        .launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();

    assert_eq!(check.updates.len(), 1, "{check:?}");
    assert_eq!(check.updates[0].rel_path, "mods/a.jar");
    assert_eq!(check.updates[0].version_id, "a2");
    assert_eq!(check.updates[0].version_number, "a2-number");
    // 合う版が無かった Modrinth の MOD は別に知らせる。固定した行は問い合わせない
    assert_eq!(check.no_compatible, vec!["mods/none.jar".to_string()]);
}

#[tokio::test]
async fn updates_include_disabled_and_hash_identified_local_rows() {
    let h = Harness::new().await;
    h.seed("off.jar.disabled", b"o1", "po", "o1");
    // 入手元が local のままでも、ハッシュで版が分かれば対象になる
    h.seed_local("local.jar", b"l1");
    h.seed_local("unknown.jar", b"u1");
    h.serve_updates(vec![
        (sha1_hex(b"o1"), h.version_json("po", "o2", "o2.jar", b"o2")),
        (sha1_hex(b"l1"), h.version_json("pl", "l2", "l2.jar", b"l2")),
    ])
    .await;

    let check = h
        .launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();

    let mut paths: Vec<&str> = check.updates.iter().map(|u| u.rel_path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, ["mods/local.jar", "mods/off.jar.disabled"]);
    // 入手元の分からない行は、合う版が無いとも言えない
    assert!(check.no_compatible.is_empty(), "{check:?}");
}

#[tokio::test]
async fn check_content_updates_is_cached_for_15_minutes_and_force_bypasses() {
    let h = Harness::new().await;
    h.seed("a.jar", b"a1", "pa", "a1");
    h.serve_updates(vec![(
        sha1_hex(b"a1"),
        h.version_json("pa", "a2", "a2.jar", b"a2"),
    )])
    .await;

    let first = h
        .launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();
    assert_eq!(h.hits(CHECK_ROUTE).await, 1);
    h.advance_clock(14 * 60);
    let second = h
        .launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();
    assert_eq!(second, first);
    assert_eq!(h.hits(CHECK_ROUTE).await, 1, "served from the cache");

    h.launcher.check_content_updates(&h.id, true).await.unwrap();
    assert_eq!(h.hits(CHECK_ROUTE).await, 2, "force bypasses the cache");

    h.advance_clock(15 * 60);
    h.launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();
    assert_eq!(h.hits(CHECK_ROUTE).await, 3, "the entry expired");
}

#[tokio::test]
async fn check_is_one_request_per_batch_regardless_of_mod_count() {
    let h = Harness::new().await;
    for i in 0..12 {
        h.seed(
            &format!("m{i}.jar"),
            format!("body{i}").as_bytes(),
            &format!("p{i}"),
            "v1",
        );
    }
    h.serve_updates(Vec::new()).await;

    h.launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();

    assert_eq!(h.hits(CHECK_ROUTE).await, 1);
}

#[tokio::test]
async fn offline_check_returns_offline_error() {
    // 何も待ち受けていないポートを使う
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let h = Harness::with_api(Some(format!("http://127.0.0.1:{port}"))).await;
    h.seed("a.jar", b"a1", "pa", "a1");

    let err = h
        .launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap_err();

    assert_eq!(err.code(), "net.offline");
}

#[tokio::test]
async fn removing_a_mod_drops_the_cached_check() {
    let h = Harness::new().await;
    h.seed("a.jar", b"a1", "pa", "a1");
    h.serve_updates(vec![(
        sha1_hex(b"a1"),
        h.version_json("pa", "a2", "a2.jar", b"a2"),
    )])
    .await;
    h.launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();

    h.launcher
        .remove_content(&h.id, nexora_core::content::ContentKind::Mod, "mods/a.jar")
        .await
        .unwrap();

    let check = h
        .launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();
    assert!(check.updates.is_empty(), "{check:?}");
}

#[tokio::test]
async fn local_row_whose_file_is_the_latest_version_is_not_an_update() {
    let h = Harness::new().await;
    // 特定の前（version_id が無い）でも、応答の版のファイルが同じハッシュなら同じ版
    h.seed_local("same.jar", b"s1");
    h.seed_local("old.jar", b"o1");
    h.serve_updates(vec![
        (sha1_hex(b"s1"), h.version_json("ps", "s1", "s1.jar", b"s1")),
        (sha1_hex(b"o1"), h.version_json("po", "o2", "o2.jar", b"o2")),
    ])
    .await;

    let check = h
        .launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();

    let paths: Vec<&str> = check.updates.iter().map(|u| u.rel_path.as_str()).collect();
    assert_eq!(paths, ["mods/old.jar"], "{check:?}");
}

#[tokio::test]
async fn identifying_content_drops_the_cached_check() {
    let h = Harness::new().await;
    h.seed("a.jar", b"a1", "pa", "a1");
    h.serve_updates(vec![(
        sha1_hex(b"a1"),
        h.version_json("pa", "a2", "a2.jar", b"a2"),
    )])
    .await;
    h.launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();
    assert_eq!(h.hits(CHECK_ROUTE).await, 1);

    // 特定の成否にかかわらず、行が変わりうるので結果を捨てる
    let _ = h.launcher.identify_content(&h.id).await;

    h.launcher
        .check_content_updates(&h.id, false)
        .await
        .unwrap();
    assert_eq!(h.hits(CHECK_ROUTE).await, 2);
}
