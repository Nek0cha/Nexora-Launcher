//! テスト用の `.mrpack` を組み立てる。

use std::io::Write;
use std::path::Path;

use serde_json::{Value, json};
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

enum Entry {
    File(Vec<u8>),
    Symlink(String),
}

#[derive(Default)]
pub struct PackBuilder {
    entries: Vec<(String, Entry)>,
}

impl PackBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// `modrinth.index.json` を入れる。
    pub fn manifest_json(self, value: Value) -> Self {
        self.file("modrinth.index.json", &serde_json::to_vec(&value).unwrap())
    }

    /// 名前はそのまま ZIP に書く（`overrides/../x` のような危険な名前も書ける）。
    pub fn file(mut self, zip_name: &str, bytes: &[u8]) -> Self {
        self.entries
            .push((zip_name.to_string(), Entry::File(bytes.to_vec())));
        self
    }

    pub fn symlink(mut self, zip_name: &str, target: &str) -> Self {
        self.entries
            .push((zip_name.to_string(), Entry::Symlink(target.to_string())));
        self
    }

    pub fn write(self, path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, entry) in self.entries {
            match entry {
                Entry::File(bytes) => {
                    zip.start_file(name, options).unwrap();
                    zip.write_all(&bytes).unwrap();
                }
                Entry::Symlink(target) => {
                    zip.add_symlink(name, target, options).unwrap();
                }
            }
        }
        zip.finish().unwrap();
    }

    /// 索引の JSON。`loader` は `("fabric-loader", "0.16.10")` のような組。
    pub fn index(mc: &str, loader: Option<(&str, &str)>, files: Vec<Value>) -> Value {
        let mut deps = serde_json::Map::new();
        deps.insert("minecraft".into(), json!(mc));
        if let Some((key, version)) = loader {
            deps.insert(key.into(), json!(version));
        }
        json!({
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1.0.0",
            "name": "Test pack",
            "files": files,
            "dependencies": deps,
        })
    }
}

// ---- Launcher を通すテストの環境 ----

use std::path::PathBuf;
use std::sync::Arc;

use nexora_core::auth::http::AuthHttp;
use nexora_core::auth::service::AuthService;
use nexora_core::auth::store::{MemoryStore, SecretStore};
use nexora_core::db::Db;
use nexora_core::install::Urls;
use nexora_core::launcher::Launcher;
use nexora_core::net::HostPolicy;
use nexora_core::net::hash::{hex, sha1_hex};
use nexora_core::paths::AppPaths;
use nexora_core::task::TaskManager;
use sha2::{Digest, Sha512};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{
    FABRIC_VERSION, FakeLoaderMeta, FakeMojang, FixedProbe, RecordingGame, SnapshotRecorder,
    auth_urls, http, serve,
};

/// Minecraft `1.99-test` と Fabric を配る偽の配信元と、パックのファイルを配る Modrinth の偽物を持つ環境。
pub struct PackHarness {
    pub _temp: tempfile::TempDir,
    pub launcher: Launcher,
    pub tasks: TaskManager,
    pub recorder: Arc<SnapshotRecorder>,
    pub paths: AppPaths,
    pub fake: FakeMojang,
    /// Modrinth の API とパックのファイルの配信を兼ねる。
    pub modrinth: MockServer,
    /// `.mrpack` を置く場所。
    pub packs: PathBuf,
}

impl PackHarness {
    pub async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("data"));
        paths.ensure_dirs().unwrap();
        let packs = temp.path().join("packs");
        std::fs::create_dir_all(&packs).unwrap();
        let fake = FakeMojang::start().await;
        let loaders = FakeLoaderMeta::start(&fake).await;
        let list = format!(r#"[{{"loader":{{"version":"{FABRIC_VERSION}","stable":true}}}}]"#);
        serve(
            &fake.server,
            "/fabric/v2/versions/loader/1.99-test",
            list.as_bytes(),
        )
        .await;
        let modrinth = MockServer::start().await;
        let urls = Urls {
            manifest: fake.manifest_url(),
            runtime_index: fake.runtime_index_url(),
            resources: fake.resources_url(),
            fabric_meta: loaders.fabric_url(),
            quilt_meta: loaders.quilt_url(),
            forge_maven: "http://127.0.0.1:1".into(),
            forge_promotions: "http://127.0.0.1:1".into(),
            neoforge_maven: "http://127.0.0.1:1".into(),
            modrinth_api: modrinth.uri(),
            modrinth_cdn: modrinth.uri(),
        };
        let (tasks, recorder) = SnapshotRecorder::manager();
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
            db,
            tasks.clone(),
            Arc::new(RecordingGame::default()),
            http(),
            urls,
            Arc::new(FixedProbe(17)),
            auth,
        )
        .with_pack_policy(HostPolicy::lenient_for_tests(&["127.0.0.1"]));
        Self {
            _temp: temp,
            launcher,
            tasks,
            recorder,
            paths,
            fake,
            modrinth,
            packs,
        }
    }

    /// `name` という `.mrpack` を `packs` に書き、そのパスを返す。
    pub fn write_pack(&self, name: &str, builder: PackBuilder) -> PathBuf {
        let path = self.packs.join(name);
        builder.write(&path);
        path
    }

    /// `route` で `body` を配る（パックのファイルのダウンロード元）。
    pub async fn serve_file(&self, route: &str, body: &[u8]) {
        Mock::given(method("GET"))
            .and(path(route.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
            .mount(&self.modrinth)
            .await;
    }

    /// 索引の `files[]` の 1 件。本文のハッシュとサイズ、`route` のダウンロード URL を持つ。
    pub fn file_entry(
        &self,
        pack_path: &str,
        body: &[u8],
        route: &str,
        env: Option<Value>,
    ) -> Value {
        let mut v = json!({
            "path": pack_path,
            "hashes": { "sha1": sha1_hex(body), "sha512": hex(&Sha512::digest(body)) },
            "downloads": [format!("{}{route}", self.modrinth.uri())],
            "fileSize": body.len(),
        });
        if let Some(env) = env {
            v["env"] = env;
        }
        v
    }
}

impl PackHarness {
    /// `route` で、`delay` だけ待ってから `body` を配る。
    pub async fn serve_file_slow(&self, route: &str, body: &[u8], delay: std::time::Duration) {
        Mock::given(method("GET"))
            .and(path(route.to_string()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(body.to_vec())
                    .set_delay(delay),
            )
            .mount(&self.modrinth)
            .await;
    }

    /// タスクが終わるまで待つ。
    pub async fn wait(&self, task_id: &str) -> nexora_core::task::TaskSnapshot {
        for _ in 0..1500 {
            let snap = self.tasks.get(task_id).unwrap();
            if snap.state.is_terminal() {
                return snap;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("task {task_id} did not finish");
    }

    /// タスクが `phase` に入るまで待つ。
    pub async fn wait_for_phase(&self, task_id: &str, phase: nexora_core::task::TaskPhase) {
        for _ in 0..1500 {
            let snap = self.tasks.get(task_id).unwrap();
            if snap.phase == Some(phase) {
                return;
            }
            assert!(!snap.state.is_terminal(), "task ended before {phase:?}");
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("task {task_id} never reached {phase:?}");
    }
}
