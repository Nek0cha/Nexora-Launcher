//! 中身の一覧、切り替え、削除、追加、特定を Launcher から通す。
mod common;

use std::path::PathBuf;
use std::sync::Arc;

use common::{FixedProbe, RecordingGame, SnapshotRecorder, auth_urls, http};
use nexora_core::auth::http::AuthHttp;
use nexora_core::auth::service::AuthService;
use nexora_core::auth::store::{MemoryStore, SecretStore};
use nexora_core::content::{ContentKind, Provider};
use nexora_core::db::Db;
use nexora_core::install::Urls;
use nexora_core::instance::{self, NewInstance};
use nexora_core::launcher::Launcher;
use nexora_core::net::hash::sha1_hex;
use nexora_core::paths::AppPaths;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct Harness {
    _temp: tempfile::TempDir,
    launcher: Launcher,
    paths: AppPaths,
    id: String,
    modrinth: MockServer,
}

impl Harness {
    fn game_dir(&self) -> PathBuf {
        let inst = instance::get(self.launcher.db(), &self.id).unwrap();
        instance::game_dir(&self.paths, &inst)
    }

    /// 起動中の行を入れる。
    fn run(&self) {
        let id = self.id.clone();
        self.launcher
            .db()
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO running_sessions VALUES (?1, 1, 1, 1, 'acc', 'x.log')",
                    [id],
                )?;
                Ok(())
            })
            .unwrap();
    }
}

async fn harness() -> Harness {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    paths.ensure_dirs().unwrap();
    let modrinth = MockServer::start().await;
    let urls = Urls {
        modrinth_api: modrinth.uri(),
        ..Urls::default()
    };
    let (tasks, _) = SnapshotRecorder::manager();
    let db = Db::open_in_memory().unwrap();
    let auth_server = MockServer::start().await;
    let auth = AuthService::new(
        db.clone(),
        paths.clone(),
        AuthHttp::new().unwrap(),
        auth_urls(&auth_server),
        Some(Arc::new(MemoryStore::default()) as Arc<dyn SecretStore>),
        Some("cid".into()),
    );
    let launcher = Launcher::with(
        paths.clone(),
        db.clone(),
        tasks,
        Arc::new(RecordingGame::default()),
        http(),
        urls,
        Arc::new(FixedProbe(17)),
        auth,
    );
    let inst = instance::create(&db, &paths, &NewInstance::vanilla("x", "1.99-test"), 0).unwrap();
    let h = Harness {
        _temp: temp,
        launcher,
        paths,
        id: inst.id,
        modrinth,
    };
    std::fs::create_dir_all(h.game_dir().join("mods")).unwrap();
    h
}

fn put_mod(h: &Harness, name: &str, body: &[u8]) {
    std::fs::write(h.game_dir().join("mods").join(name), body).unwrap();
}

#[tokio::test]
async fn list_content_scans_the_disk_while_the_game_is_running() {
    let h = harness().await;
    put_mod(&h, "a.jar", b"aaa");
    h.run();
    let items = h
        .launcher
        .list_content(&h.id, ContentKind::Mod)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].rel_path, "mods/a.jar");
    assert!(items[0].enabled);
}

#[tokio::test]
async fn mutations_are_refused_with_instance_busy_while_running() {
    let h = harness().await;
    put_mod(&h, "a.jar", b"aaa");
    h.launcher
        .list_content(&h.id, ContentKind::Mod)
        .await
        .unwrap();
    h.run();
    let src = h._temp.path().join("new.jar");
    std::fs::write(&src, b"new").unwrap();

    let err = h
        .launcher
        .set_content_enabled(&h.id, ContentKind::Mod, "mods/a.jar", false)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    let err = h
        .launcher
        .remove_content(&h.id, ContentKind::Mod, "mods/a.jar")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    let err = h
        .launcher
        .add_content_files(&h.id, ContentKind::Mod, vec![src], false)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "instance.busy");

    let mut names: Vec<_> = std::fs::read_dir(h.game_dir().join("mods"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["a.jar"]);
}

#[tokio::test]
async fn add_then_identify_then_list_shows_modrinth_metadata() {
    let h = harness().await;
    let body = b"some-jar-bytes";
    let hash = sha1_hex(body);
    let src = h._temp.path().join("fabric-api.jar");
    std::fs::write(&src, body).unwrap();

    let report = h
        .launcher
        .add_content_files(&h.id, ContentKind::Mod, vec![src], false)
        .await
        .unwrap();
    assert_eq!(report.added.len(), 1);
    assert!(!report.added[0].identified);

    Mock::given(method("POST"))
        .and(path("/version_files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            hash.clone(): {
                "id": "VER1", "project_id": "PROJ1", "version_number": "1.2.3",
                "dependencies": []
            }
        })))
        .mount(&h.modrinth)
        .await;
    Mock::given(method("GET"))
        .and(path("/projects"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "id": "PROJ1", "title": "Fabric API", "description": "d",
                "categories": ["library"], "additional_categories": [],
                "client_side": "required", "server_side": "required", "icon_url": null
            }])),
        )
        .mount(&h.modrinth)
        .await;

    let r = h.launcher.identify_content(&h.id).await.unwrap();
    assert_eq!(r.identified, 1);
    let items = h
        .launcher
        .list_content(&h.id, ContentKind::Mod)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert!(item.identified);
    assert_eq!(item.provider, Provider::Modrinth);
    assert_eq!(item.name.as_deref(), Some("Fabric API"));
    assert_eq!(item.version_number.as_deref(), Some("1.2.3"));
    assert_eq!(item.categories, vec!["library"]);
}

#[tokio::test]
async fn list_content_for_a_missing_instance_is_not_found() {
    let h = harness().await;
    let err = h
        .launcher
        .list_content("missing", ContentKind::Mod)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "instance.not_found");
}

#[tokio::test]
async fn content_icon_rejects_a_key_that_is_not_hex() {
    let h = harness().await;
    let dir = h.paths.content_icons_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("x.png"), b"png").unwrap();
    let good = "a".repeat(40);
    std::fs::write(dir.join(format!("{good}.png")), b"png").unwrap();

    for key in ["../x", "x", "", &"g".repeat(40), &"a".repeat(39)] {
        assert_eq!(h.launcher.content_icon(key).unwrap(), None, "{key}");
    }
    assert_eq!(
        h.launcher.content_icon(&good).unwrap(),
        Some(b"png".to_vec())
    );
    assert_eq!(h.launcher.content_icon(&"b".repeat(40)).unwrap(), None);
}

#[tokio::test]
async fn content_categories_filters_by_kind() {
    let h = harness().await;
    Mock::given(method("GET"))
        .and(path("/tag/category"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                include_str!("fixtures/modrinth/categories.json")
                    .as_bytes()
                    .to_vec(),
                "application/json",
            ),
        )
        .mount(&h.modrinth)
        .await;
    let mods = h
        .launcher
        .content_categories(ContentKind::Mod)
        .await
        .unwrap();
    assert_eq!(mods, vec!["adventure", "fabric"]);
    let packs = h
        .launcher
        .content_categories(ContentKind::ResourcePack)
        .await
        .unwrap();
    assert_eq!(packs, vec!["realistic"]);
    let shaders = h
        .launcher
        .content_categories(ContentKind::Shader)
        .await
        .unwrap();
    assert_eq!(shaders, vec!["cartoon"]);
}
