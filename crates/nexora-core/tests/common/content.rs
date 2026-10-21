//! Modrinth の導入と更新の結合テストで使う部品。API と CDN は同じモックのサーバーが受ける。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use nexora_core::auth::http::AuthHttp;
use nexora_core::auth::service::AuthService;
use nexora_core::auth::store::{MemoryStore, SecretStore};
use nexora_core::content::ContentKind;
use nexora_core::content::scan::scan;
use nexora_core::db::Db;
use nexora_core::install::Urls;
use nexora_core::instance::{self, NewInstance};
use nexora_core::launcher::Launcher;
use nexora_core::meta::loader::LoaderKind;
use nexora_core::net::hash::sha1_hex;
use nexora_core::paths::AppPaths;
use nexora_core::task::{TaskManager, TaskSnapshot};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{FixedProbe, RecordingGame, SnapshotRecorder, auth_urls, http};

pub struct Harness {
    pub _temp: tempfile::TempDir,
    pub launcher: Launcher,
    pub tasks: TaskManager,
    pub paths: AppPaths,
    pub id: String,
    pub server: MockServer,
    /// 更新の確認のキャッシュの時計を進めた秒数。
    pub clock: Arc<AtomicU64>,
}

impl Harness {
    /// Fabric の 1.99-test のインスタンスと、API と CDN を兼ねるモックのサーバー。
    pub async fn new() -> Self {
        Self::with_api(None).await
    }

    /// `api` を指定すると、Modrinth の API だけ別の場所（落ちたサーバーなど）にする。
    pub async fn with_api(api: Option<String>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path());
        paths.ensure_dirs().unwrap();
        let server = MockServer::start().await;
        let urls = Urls {
            modrinth_api: api.unwrap_or_else(|| server.uri()),
            modrinth_cdn: server.uri(),
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
        let clock = Arc::new(AtomicU64::new(0));
        let base = Instant::now();
        let launcher = Launcher::with(
            paths.clone(),
            db.clone(),
            tasks.clone(),
            Arc::new(RecordingGame::default()),
            http(),
            urls,
            Arc::new(FixedProbe(17)),
            auth,
        )
        .with_update_clock({
            let clock = clock.clone();
            Arc::new(move || base + Duration::from_secs(clock.load(Ordering::SeqCst)))
        });
        let new = NewInstance {
            loader: LoaderKind::Fabric,
            loader_version: Some("0.15.0".into()),
            ..NewInstance::vanilla("x", "1.99-test")
        };
        let inst = instance::create(&db, &paths, &new, 0).unwrap();
        let h = Harness {
            _temp: temp,
            launcher,
            tasks,
            paths,
            id: inst.id,
            server,
            clock,
        };
        std::fs::create_dir_all(h.mods_dir()).unwrap();
        h
    }

    pub fn advance_clock(&self, secs: u64) {
        self.clock.fetch_add(secs, Ordering::SeqCst);
    }

    pub fn game_dir(&self) -> PathBuf {
        let inst = instance::get(self.launcher.db(), &self.id).unwrap();
        instance::game_dir(&self.paths, &inst)
    }

    pub fn mods_dir(&self) -> PathBuf {
        self.game_dir().join("mods")
    }

    /// `mods/` のファイル名（並べ替え済み）。
    pub fn mods(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.mods_dir())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    /// 起動中の行を入れる。
    pub fn run(&self) {
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

    /// 戻しきれずに残った prepared の操作を入れる。
    pub fn leave_prepared_operation(&self) {
        let id = self.id.clone();
        self.launcher
            .db()
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO operations (id, kind, instance_id, state, targets, created_at)
                     VALUES ('op1', 'update', ?1, 'prepared', '[]', 1)",
                    [id],
                )?;
                Ok(())
            })
            .unwrap();
    }

    /// `mods/<name>` を置いて走査し、Modrinth で特定済みの行に見せかける。
    pub fn seed(&self, name: &str, body: &[u8], project: &str, version: &str) {
        std::fs::write(self.mods_dir().join(name), body).unwrap();
        self.rescan();
        self.update_row(
            &format!("mods/{name}"),
            "provider = 'modrinth', project_id = ?3, version_id = ?4, identified_at = 1",
            (project, version),
        );
    }

    /// `mods/<name>` を置いて走査するだけ（入手元は local のまま）。
    pub fn seed_local(&self, name: &str, body: &[u8]) {
        std::fs::write(self.mods_dir().join(name), body).unwrap();
        self.rescan();
    }

    fn rescan(&self) {
        scan(
            self.launcher.db(),
            &self.game_dir(),
            &self.id,
            ContentKind::Mod,
            &CancellationToken::new(),
        )
        .unwrap();
    }

    fn update_row(&self, rel_path: &str, set: &str, args: (&str, &str)) {
        let sql =
            format!("UPDATE instance_contents SET {set} WHERE instance_id = ?1 AND rel_path = ?2");
        self.launcher
            .db()
            .with_conn(|conn| {
                conn.execute(&sql, rusqlite::params![self.id, rel_path, args.0, args.1])?;
                Ok(())
            })
            .unwrap();
    }

    pub fn pin(&self, name: &str) {
        self.launcher
            .db()
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE instance_contents SET pinned = 1 WHERE instance_id = ?1 AND rel_path = ?2",
                    rusqlite::params![self.id, format!("mods/{name}")],
                )?;
                Ok(())
            })
            .unwrap();
    }

    /// `mods/<name>` の行に、特定で書かれるプロジェクトの情報（名前、説明、アイコン、環境、カテゴリー）を書く。
    pub fn describe(&self, name: &str, title: &str, icon_url: Option<&str>) {
        self.launcher
            .db()
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE instance_contents
                     SET name = ?3, description = ?4, icon_url = ?5, client_side = 'required',
                         server_side = 'optional', categories = '[\"optimization\"]'
                     WHERE instance_id = ?1 AND rel_path = ?2",
                    rusqlite::params![
                        self.id,
                        format!("mods/{name}"),
                        title,
                        format!("{title} description"),
                        icon_url
                    ],
                )?;
                Ok(())
            })
            .unwrap();
    }

    /// 版の JSON。ファイルは CDN の `/data/<project>/<version>/<filename>` から返す。
    pub fn version_json(&self, project: &str, id: &str, filename: &str, body: &[u8]) -> Value {
        json!({
            "id": id,
            "project_id": project,
            "version_number": format!("{id}-number"),
            "version_type": "release",
            "game_versions": ["1.99-test"],
            "loaders": ["fabric"],
            "files": [{
                "url": format!("{}/data/{project}/{id}/{filename}", self.server.uri()),
                "filename": filename,
                "hashes": { "sha1": sha1_hex(body) },
                "size": body.len(),
                "primary": true,
            }],
            "dependencies": [],
        })
    }

    /// CDN にファイルを置く。`delay` があれば、その時間だけ返事を待たせる。
    pub async fn serve_file(
        &self,
        project: &str,
        version: &str,
        filename: &str,
        body: &[u8],
        delay: Option<Duration>,
    ) {
        let mut response = ResponseTemplate::new(200).set_body_bytes(body.to_vec());
        if let Some(delay) = delay {
            response = response.set_delay(delay);
        }
        Mock::given(method("GET"))
            .and(path(format!("/data/{project}/{version}/{filename}")))
            .respond_with(response)
            .mount(&self.server)
            .await;
    }

    /// `POST /version_files/update` の応答。キーは SHA-1。
    pub async fn serve_updates(&self, entries: Vec<(String, Value)>) {
        let map: serde_json::Map<String, Value> = entries.into_iter().collect();
        Mock::given(method("POST"))
            .and(path("/version_files/update"))
            .respond_with(ResponseTemplate::new(200).set_body_json(Value::Object(map)))
            .mount(&self.server)
            .await;
    }

    /// `GET /project/<id>/version` の応答（新しい順）。
    pub async fn serve_project_versions(&self, project: &str, versions: Vec<Value>) {
        Mock::given(method("GET"))
            .and(path(format!("/project/{project}/version")))
            .respond_with(ResponseTemplate::new(200).set_body_json(Value::Array(versions)))
            .mount(&self.server)
            .await;
    }

    /// 経路への要求の数。
    pub async fn hits(&self, route: &str) -> usize {
        super::hits_on(&self.server, route).await
    }

    pub async fn wait(&self, task_id: &str) -> TaskSnapshot {
        for _ in 0..1500 {
            let snap = self.tasks.get(task_id).unwrap();
            if snap.state.is_terminal() {
                return snap;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("task {task_id} did not finish");
    }
}
