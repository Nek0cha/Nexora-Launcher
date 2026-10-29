//! デフォルトスキンの取得とキャッシュ、適用の結合テスト（wiremock のみ。本物の Mojang へは出ない）。
//! 取得元は FakeMojang（manifest、version JSON、クライアント jar）で、jar はテストの中で作る。

mod common;

use std::collections::HashSet;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use common::{
    FakeMojang, MS_ACCESS, Ownership, PROFILE_ID, PROFILE_NAME, REFRESH_1, auth_urls, hits_on,
    http, mount_chain,
};
use nexora_core::auth::http::AuthHttp;
use nexora_core::auth::microsoft::MsTokens;
use nexora_core::auth::minecraft::SkinVariant;
use nexora_core::auth::service::AuthService;
use nexora_core::auth::store::{MemoryStore, SecretStore};
use nexora_core::db::Db;
use nexora_core::error::CoreError;
use nexora_core::meta::mojang::manifest::client_jar_path;
use nexora_core::net::hash::sha1_hex;
use nexora_core::paths::AppPaths;
use nexora_core::skins::defaults::{self, DefaultSkin};
use nexora_core::skins::{library, ops};
use secrecy::SecretString;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const T0: i64 = 1_800_000_000_000;
const PREFIX: &str = "assets/minecraft/textures/entity/player";

/// 署名と IHDR だけの PNG（`validate_skin_png` は通る）。`seed` で中身が変わる。
fn skin_png(seed: u8) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(&13u32.to_be_bytes());
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&64u32.to_be_bytes());
    v.extend_from_slice(&64u32.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0, seed, 0, 0, 0]);
    v
}

/// (jar の中の名前, 中身) から jar を作る。
fn jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, body) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(body).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// 本物と同じ配置（wide と slim）で、steve と alex を持つ jar。ほかの画像も混ぜる。
fn standard_jar() -> Vec<u8> {
    jar(&[
        ("assets/minecraft/textures/entity/steve.png", &skin_png(90)),
        (&format!("{PREFIX}/wide/steve.png"), &skin_png(1)),
        (&format!("{PREFIX}/wide/alex.png"), &skin_png(2)),
        (&format!("{PREFIX}/slim/steve.png"), &skin_png(3)),
        (&format!("{PREFIX}/slim/alex.png"), &skin_png(4)),
        (&format!("{PREFIX}/wide/readme.txt"), b"not an image"),
        ("assets/minecraft/textures/block/stone.png", &skin_png(91)),
    ])
}

fn cache_dir(paths: &AppPaths) -> std::path::PathBuf {
    paths.cache_dir().join("default-skins")
}

async fn list(fake: &FakeMojang, paths: &AppPaths) -> Result<Vec<DefaultSkin>, CoreError> {
    defaults::list_defaults(&http(), paths, &fake.manifest_url()).await
}

fn summary(skins: &[DefaultSkin]) -> Vec<(String, String, SkinVariant)> {
    skins
        .iter()
        .map(|s| (s.key.clone(), s.name.clone(), s.variant))
        .collect()
}

fn decode(image: &str) -> Vec<u8> {
    let b64 = image.strip_prefix("data:image/png;base64,").unwrap();
    STANDARD.decode(b64).unwrap()
}

#[tokio::test]
async fn lists_every_default_skin_the_game_jar_holds_as_data_urls() {
    let fake = FakeMojang::start_with_client(&standard_jar()).await;
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());

    let skins = list(&fake, &paths).await.unwrap();

    // wide が classic、slim が slim。classic を先に、名前順
    assert_eq!(
        summary(&skins),
        vec![
            ("classic-alex".into(), "Alex".into(), SkinVariant::Classic),
            ("classic-steve".into(), "Steve".into(), SkinVariant::Classic),
            ("slim-alex".into(), "Alex".into(), SkinVariant::Slim),
            ("slim-steve".into(), "Steve".into(), SkinVariant::Slim),
        ]
    );
    assert_eq!(decode(&skins[0].image), skin_png(2));
    assert_eq!(decode(&skins[1].image), skin_png(1));
    assert_eq!(decode(&skins[2].image), skin_png(4));
    assert_eq!(decode(&skins[3].image), skin_png(3));
    // sha256 は元の画像の画素のハッシュ（保存したスキンと同じ値）
    assert_eq!(
        skins[0].sha256,
        nexora_core::skins::pixel_hash(&skin_png(2))
    );
    assert_ne!(skins[0].sha256, skins[1].sha256);

    // 取得に使った jar は残さない（共通の場所にも置かない）
    let leftovers: Vec<_> = std::fs::read_dir(cache_dir(&paths))
        .unwrap()
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x == "jar" || x == "part")
        })
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert!(!client_jar_path(&paths, "1.99-test").exists());
}

#[tokio::test]
async fn the_second_call_reads_the_cache_and_makes_no_request() {
    let fake = FakeMojang::start_with_client(&standard_jar()).await;
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());

    let first = list(&fake, &paths).await.unwrap();
    assert_eq!(fake.hits("/client.jar").await, 1);
    let before = fake.server.received_requests().await.unwrap().len();

    let second = list(&fake, &paths).await.unwrap();

    assert_eq!(summary(&first), summary(&second));
    assert_eq!(first[0].image, second[0].image);
    assert_eq!(first[0].sha256, second[0].sha256);
    assert_eq!(
        fake.server.received_requests().await.unwrap().len(),
        before,
        "the cached call must not touch the network"
    );
    assert_eq!(hits_on(&fake.server, "/client.jar").await, 1);
}

#[tokio::test]
async fn an_invalid_image_drops_only_that_entry() {
    let bad_dims = {
        let mut v = skin_png(5);
        v[19] = 100; // 幅を 64 から変える
        v
    };
    let jar = jar(&[
        (&format!("{PREFIX}/wide/steve.png"), &skin_png(1)),
        (
            &format!("{PREFIX}/wide/broken.png"),
            b"definitely not a png",
        ),
        (&format!("{PREFIX}/slim/odd.png"), &bad_dims),
        (&format!("{PREFIX}/slim/alex.png"), &skin_png(4)),
    ]);
    let fake = FakeMojang::start_with_client(&jar).await;
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());

    let skins = list(&fake, &paths).await.unwrap();

    assert_eq!(
        skins.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(),
        vec!["classic-steve", "slim-alex"]
    );
    // 捨てたものはキャッシュにも残らない（2 回目も同じ 2 件）
    assert_eq!(list(&fake, &paths).await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_jar_without_usable_skins_fails_and_caches_nothing() {
    let jar = jar(&[
        (&format!("{PREFIX}/wide/steve.png"), b"junk"),
        ("assets/minecraft/textures/entity/steve.png", &skin_png(1)),
    ]);
    let fake = FakeMojang::start_with_client(&jar).await;
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());

    let err = list(&fake, &paths).await.unwrap_err();
    assert!(matches!(err, CoreError::InvalidMetadata { .. }), "{err:?}");

    // 取れていない状態を「取得済み」として覚えない（次の呼び出しは取りに行く）
    list(&fake, &paths).await.unwrap_err();
    assert_eq!(fake.hits("/client.jar").await, 2);
}

#[tokio::test]
async fn an_unreachable_source_fails_with_offline() {
    // 予約済みの `.invalid` は名前が引けないので、実在のどのホストにも届かない
    let manifest_url = "http://mojang-meta.invalid/manifest.json".to_string();
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());

    let err = defaults::list_defaults(&http(), &paths, &manifest_url)
        .await
        .unwrap_err();

    assert!(matches!(err, CoreError::Offline { .. }), "{err:?}");
}

#[tokio::test]
async fn a_valid_jar_already_in_the_shared_versions_is_read_without_downloading() {
    let jar_bytes = standard_jar();
    let fake = FakeMojang::start_with_client(&jar_bytes).await;
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    let shared = client_jar_path(&paths, "1.99-test");
    std::fs::create_dir_all(shared.parent().unwrap()).unwrap();
    std::fs::write(&shared, &jar_bytes).unwrap();

    let skins = list(&fake, &paths).await.unwrap();

    assert_eq!(skins.len(), 4);
    assert_eq!(fake.hits("/client.jar").await, 0);
    // 共通の jar はそのまま残る
    assert_eq!(
        sha1_hex(&std::fs::read(&shared).unwrap()),
        sha1_hex(&jar_bytes)
    );
}

// ---- 適用 ----

struct Env {
    server: MockServer,
    db: Db,
    auth: AuthService,
    paths: AppPaths,
    _temp: tempfile::TempDir,
}

async fn env() -> Env {
    let server = MockServer::start().await;
    let db = Db::open_in_memory().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    let clock = Arc::new(AtomicI64::new(T0));
    // 画像の取得先もモックにする（実際の textures.minecraft.net へは通信しない）
    let mut urls = auth_urls(&server);
    urls.texture_origin = Some(format!("{}/", server.uri()));
    let auth = AuthService::new(
        db.clone(),
        paths.clone(),
        AuthHttp::new().unwrap(),
        urls,
        Some(store),
        Some("cid".into()),
    )
    .with_clock(Arc::new(move || clock.load(Ordering::SeqCst)));
    Env {
        server,
        db,
        auth,
        paths,
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

fn has(body: &[u8], needle: &[u8]) -> bool {
    body.windows(needle.len()).any(|w| w == needle)
}

#[tokio::test]
async fn apply_default_posts_that_skins_png_and_variant_and_leaves_the_library_alone() {
    let fake = FakeMojang::start_with_client(&standard_jar()).await;
    let e = env().await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let id = e.login().await;
    Mock::given(method("POST"))
        .and(path("/mc/profile/skins"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&e.server)
        .await;

    defaults::apply_default(
        &http(),
        &e.paths,
        &fake.manifest_url(),
        &e.auth,
        id,
        "slim-alex",
    )
    .await
    .unwrap();

    e.server.verify().await;
    let requests = e.server.received_requests().await.unwrap();
    let upload = requests
        .iter()
        .find(|r| r.url.path() == "/mc/profile/skins")
        .unwrap();
    assert!(has(&upload.body, b"name=\"variant\"\r\n\r\nslim\r\n"));
    assert!(has(&upload.body, &skin_png(4)));
    // デフォルトのスキンはライブラリに足さない
    assert!(library::list(&e.db).unwrap().is_empty());
}

#[tokio::test]
async fn apply_default_with_an_unknown_key_fails_without_uploading() {
    let fake = FakeMojang::start_with_client(&standard_jar()).await;
    let e = env().await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let id = e.login().await;

    let err = defaults::apply_default(
        &http(),
        &e.paths,
        &fake.manifest_url(),
        &e.auth,
        id,
        "classic-nobody",
    )
    .await
    .unwrap_err();

    assert!(matches!(err, CoreError::SkinNotFound), "{err:?}");
    assert_eq!(hits_on(&e.server, "/mc/profile/skins").await, 0);
}

/// 適用したデフォルトスキンは現在のスキンになるが、次に画面を開いたときの取り込みでライブラリへ入れない（記録だけ残す）。
#[tokio::test]
async fn an_applied_default_skin_is_not_imported_as_the_current_skin() {
    let fake = FakeMojang::start_with_client(&standard_jar()).await;
    let e = env().await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let id = e.login().await;
    Mock::given(method("POST"))
        .and(path("/mc/profile/skins"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&e.server)
        .await;
    defaults::apply_default(
        &http(),
        &e.paths,
        &fake.manifest_url(),
        &e.auth,
        id,
        "slim-alex",
    )
    .await
    .unwrap();

    // Mojang はいま、そのデフォルトスキンを現在のスキンとして返す
    let url = format!("{}/texture/applied", e.server.uri());
    Mock::given(method("GET"))
        .and(path("/mc/profile"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"id":"{PROFILE_ID}","name":"{PROFILE_NAME}","skins":[{{"state":"ACTIVE","url":"{url}","variant":"SLIM"}}]}}"#
        )))
        .with_priority(1)
        .mount(&e.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/texture/applied"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(skin_png(4)))
        .mount(&e.server)
        .await;

    let hashes = defaults::cached_hashes(&e.paths).await;
    assert_eq!(hashes.len(), 4);
    let imported = ops::import_current_skin(&e.db, &e.auth, hashes, id, T0)
        .await
        .unwrap();
    assert!(imported.is_none());
    assert!(library::list(&e.db).unwrap().is_empty());
    // 記録は残り、あとでキャッシュが空でも二度と取り込まれない
    let again = ops::import_current_skin(&e.db, &e.auth, HashSet::new(), id, T0 + 1)
        .await
        .unwrap();
    assert!(again.is_none());
    assert!(library::list(&e.db).unwrap().is_empty());
}

/// デフォルトスキンのキャッシュが無ければ、通信せず空を返す。
#[tokio::test]
async fn cached_hashes_is_empty_without_a_cache() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    assert!(defaults::cached_hashes(&paths).await.is_empty());
}
