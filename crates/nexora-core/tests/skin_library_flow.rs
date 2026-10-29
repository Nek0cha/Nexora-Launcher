//! 保存したスキンの適用と取り込みの結合テスト（`Db` と `AuthService`、wiremock）。

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use common::{MS_ACCESS, Ownership, REFRESH_1, auth_urls, hits_on, mount_chain};
use nexora_core::auth::http::AuthHttp;
use nexora_core::auth::microsoft::MsTokens;
use nexora_core::auth::minecraft::SkinVariant;
use nexora_core::auth::service::AuthService;
use nexora_core::auth::store::{MemoryStore, SecretStore};
use nexora_core::db::Db;
use nexora_core::error::CoreError;
use nexora_core::paths::AppPaths;
use nexora_core::skins::library;
use nexora_core::skins::ops;
use secrecy::SecretString;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const T0: i64 = 1_800_000_000_000;

struct Env {
    server: MockServer,
    db: Db,
    auth: AuthService,
    _temp: tempfile::TempDir,
}

async fn env() -> Env {
    let server = MockServer::start().await;
    let db = Db::open_in_memory().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    let clock = Arc::new(AtomicI64::new(T0));
    let auth = AuthService::new(
        db.clone(),
        AppPaths::from_root(temp.path()),
        AuthHttp::new().unwrap(),
        auth_urls(&server),
        Some(store),
        Some("cid".into()),
    )
    .with_clock(Arc::new(move || clock.load(Ordering::SeqCst)));
    Env {
        server,
        db,
        auth,
        _temp: temp,
    }
}

impl Env {
    async fn login(&self) -> Uuid {
        let tokens = MsTokens {
            access: SecretString::from(MS_ACCESS.to_string()),
            refresh: Some(SecretString::from(REFRESH_1.to_string())),
            expires_at_ms: T0 + 3_600_000,
        };
        self.auth
            .complete_login(tokens, &|_| {}, &CancellationToken::new())
            .await
            .unwrap()
    }
}

/// 署名と IHDR だけの PNG（検査は通る）。`seed` で中身が変わる。
fn skin_png(w: u32, h: u32, seed: u8) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(&13u32.to_be_bytes());
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0, seed, 0, 0, 0]);
    v
}

fn has(body: &[u8], needle: &[u8]) -> bool {
    body.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn add_saved_skin_from_path_rejects_an_oversized_file_without_reading_it() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open_in_memory().unwrap();
    // 大きさだけで断る。中身が正しい PNG でも、64 KiB を超えれば読まない
    let mut big = skin_png(64, 64, 0);
    big.resize(64 * 1024 + 1, 0);
    let file = dir.path().join("big.png");
    std::fs::write(&file, &big).unwrap();
    let err = ops::add_from_path(&db, &file, 1).unwrap_err();
    assert!(
        matches!(&err, CoreError::InvalidSkin { reason } if reason == "too_large"),
        "{err:?}"
    );
    assert!(library::list(&db).unwrap().is_empty());
    // ちょうど上限なら読む
    let mut max = skin_png(64, 64, 0);
    max.resize(64 * 1024, 0);
    let file = dir.path().join("max.png");
    std::fs::write(&file, &max).unwrap();
    assert_eq!(ops::add_from_path(&db, &file, 1).unwrap().name, "max");
}

#[test]
fn add_saved_skin_from_path_uses_the_file_stem_and_refuses_non_files_and_non_png() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open_in_memory().unwrap();
    let file = dir.path().join("My Skin.v2.png");
    std::fs::write(&file, skin_png(64, 64, 1)).unwrap();
    assert_eq!(
        ops::add_from_path(&db, &file, 1).unwrap().name,
        "My Skin.v2"
    );
    // ディレクトリ
    let sub = dir.path().join("folder.png");
    std::fs::create_dir(&sub).unwrap();
    assert!(matches!(
        ops::add_from_path(&db, &sub, 1),
        Err(CoreError::InvalidSkin { .. })
    ));
    // 存在しない
    assert!(matches!(
        ops::add_from_path(&db, &dir.path().join("none.png"), 1),
        Err(CoreError::Io { .. })
    ));
    // PNG ではない
    let text = dir.path().join("note.png");
    std::fs::write(&text, b"hello, not a png at all").unwrap();
    assert!(matches!(
        ops::add_from_path(&db, &text, 1),
        Err(CoreError::InvalidSkin { .. })
    ));
    assert_eq!(library::list(&db).unwrap().len(), 1);
}

#[tokio::test]
async fn apply_saved_skin_posts_the_stored_png_and_variant() {
    let e = env().await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let id = e.login().await;
    let png = skin_png(64, 64, 7);
    let saved = library::add(&e.db, "Slimmy", Some(SkinVariant::Slim), &png, 1).unwrap();
    Mock::given(method("POST"))
        .and(path("/mc/profile/skins"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&e.server)
        .await;
    ops::apply_saved(&e.db, &e.auth, id, &saved.id)
        .await
        .unwrap();
    e.server.verify().await;
    let requests = e.server.received_requests().await.unwrap();
    let upload = requests
        .iter()
        .find(|r| r.url.path() == "/mc/profile/skins")
        .unwrap();
    assert!(has(&upload.body, b"name=\"variant\"\r\n\r\nslim\r\n"));
    assert!(has(&upload.body, &png));
}

#[tokio::test]
async fn apply_saved_skin_for_an_unknown_id_fails_without_a_request() {
    let e = env().await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let id = e.login().await;
    let before = e.server.received_requests().await.unwrap().len();
    let err = ops::apply_saved(&e.db, &e.auth, id, "nope")
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::SkinNotFound), "{err:?}");
    assert_eq!(e.server.received_requests().await.unwrap().len(), before);
    assert_eq!(hits_on(&e.server, "/mc/profile/skins").await, 0);
}

#[tokio::test]
async fn import_current_skin_returns_none_and_logs_nothing_when_there_is_no_skin() {
    let e = env().await;
    // 既定のプロフィールにはスキンがない
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let id = e.login().await;
    assert!(
        ops::import_current_skin(&e.db, &e.auth, id, 1)
            .await
            .unwrap()
            .is_none()
    );
    let logged: i64 = e
        .db
        .with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM skin_import_log", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(logged, 0);
    assert!(library::list(&e.db).unwrap().is_empty());
}
