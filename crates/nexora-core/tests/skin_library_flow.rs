//! 保存したスキンの適用と取り込みの結合テスト（`Db` と `AuthService`、wiremock）。

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use common::{
    MS_ACCESS, Ownership, PROFILE_ID, PROFILE_NAME, REFRESH_1, auth_urls, hits_on, mount_chain,
};
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
    temp_path: std::path::PathBuf,
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
        temp_path: temp.path().to_path_buf(),
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

const SKIN_URL: &str = "https://textures.minecraft.net/texture/lib-skin";

/// 本物の PNG（全画素が rgba の 64×64）。
fn real_png(rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, 64, 64);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    let data: Vec<u8> = (0..64 * 64).flat_map(|_| rgba).collect();
    writer.write_image_data(&data).unwrap();
    writer.finish().unwrap();
    out
}

/// 画像のキャッシュに置く（実際の textures.minecraft.net へは通信しない）。
fn seed_image(e: &Env, url: &str, bytes: &[u8]) {
    use sha2::{Digest, Sha256};
    let dir = e.temp_path.join("cache").join("skins");
    std::fs::create_dir_all(&dir).unwrap();
    let name = nexora_core::net::hash::hex(&Sha256::digest(url.as_bytes()));
    std::fs::write(dir.join(format!("{name}.png")), bytes).unwrap();
}

async fn mount_profile_with_skin(e: &Env, url: &str, variant: &str) {
    Mock::given(method("GET"))
        .and(path("/mc/profile"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"id":"{PROFILE_ID}","name":"{PROFILE_NAME}","skins":[{{"state":"ACTIVE","url":"{url}","variant":"{variant}"}}]}}"#
        )))
        .with_priority(1)
        .mount(&e.server)
        .await;
}

fn log_count(e: &Env) -> i64 {
    e.db.with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM skin_import_log", [], |r| r.get(0))?))
        .unwrap()
}

#[tokio::test]
async fn import_current_skin_imports_once_with_profile_variant_and_username() {
    let e = env().await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let id = e.login().await;
    let bytes = real_png([10, 20, 30, 255]);
    seed_image(&e, SKIN_URL, &bytes);
    mount_profile_with_skin(&e, SKIN_URL, "SLIM").await;
    let s = ops::import_current_skin(&e.db, &e.auth, id, 5)
        .await
        .unwrap()
        .expect("imported");
    assert_eq!(s.variant, SkinVariant::Slim);
    assert_eq!(s.name, PROFILE_NAME);
    assert_eq!(s.sha256, nexora_core::skins::pixel_hash(&bytes));
    assert_eq!(log_count(&e), 1);
    // 二度目は取り込まない
    assert!(
        ops::import_current_skin(&e.db, &e.auth, id, 6)
            .await
            .unwrap()
            .is_none()
    );
    // 削除しても戻らない
    library::delete(&e.db, &s.id).unwrap();
    assert!(
        ops::import_current_skin(&e.db, &e.auth, id, 7)
            .await
            .unwrap()
            .is_none()
    );
    assert!(library::list(&e.db).unwrap().is_empty());
    assert_eq!(log_count(&e), 1);
}

#[tokio::test]
async fn import_current_skin_with_an_unfetchable_texture_logs_nothing_and_retries_later() {
    let e = env().await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let id = e.login().await;
    // 許可されたホストだが、キャッシュにも無く、取得もできない（存在しない画像）
    let url = "https://textures.minecraft.net/texture/nexora-test-never-uploaded-0001";
    mount_profile_with_skin(&e, url, "CLASSIC").await;
    assert!(
        ops::import_current_skin(&e.db, &e.auth, id, 1)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(log_count(&e), 0);
    assert!(library::list(&e.db).unwrap().is_empty());
    // 画像が手に入れば、次の呼び出しで取り込める
    let bytes = real_png([1, 2, 3, 255]);
    seed_image(&e, url, &bytes);
    let s = ops::import_current_skin(&e.db, &e.auth, id, 2)
        .await
        .unwrap()
        .expect("imported later");
    assert_eq!(s.variant, SkinVariant::Classic);
    assert_eq!(log_count(&e), 1);
}
