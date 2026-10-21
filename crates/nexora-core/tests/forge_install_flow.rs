//! Forge の導入と起動の流れ。Mojang の配信、Forge の Maven、Java の runtime をすべて wiremock の偽物にする。
//!
//! 偽のインストーラは、processor を 1 つ（client 用）と server 用の 1 つ（実行されない）、
//! `version.json`、`maven/` の 2 ファイル、`/data/client.lzma` を持つ。
//! processor の Java は、Mojang の runtime として配る偽の Java（`tests/support/fake_java.rs`）で、
//! `shared/fake-calls.txt` に呼び出しを記録し、`shared/fake-control.txt` の中身で動作を変える。

mod common;

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nexora_core::auth::accounts::{self, EntitlementState, ProfileRecord};
use nexora_core::auth::http::AuthHttp;
use nexora_core::auth::service::AuthService;
use nexora_core::auth::store::{MemoryStore, SecretStore};
use nexora_core::db::Db;
use nexora_core::install::{Urls, now_ms};
use nexora_core::instance::{self, InstanceState, NewInstance};
use nexora_core::launcher::Launcher;
use nexora_core::meta::loader::LoaderKind;
use nexora_core::net::hash::sha1_hex;
use nexora_core::paths::AppPaths;
use nexora_core::task::{TaskManager, TaskPhase, TaskSnapshot, TaskState};
use secrecy::SecretString;
use uuid::Uuid;
use wiremock::MockServer;
use zip::write::SimpleFileOptions;

use common::{
    FakeMojang, FixedProbe, PROFILE_ID, PROFILE_NAME, REFRESH_1, RecordingGame, SnapshotRecorder,
    XUID, auth_urls, hits_on, http, serve,
};

const MC: &str = "1.16.5";
const FORGE: &str = "1.16.5-36.2.39";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

const INSTALLER_ROUTE: &str =
    "/forge/net/minecraftforge/forge/1.16.5-36.2.39/forge-1.16.5-36.2.39-installer.jar";
const PROCESSOR_ROUTE: &str =
    "/maven/net/minecraftforge/binarypatcher/1.0.12/binarypatcher-1.0.12.jar";
const DEP_ROUTE: &str = "/maven/com/example/forge-dep/1.0/forge-dep-1.0.jar";
const DEP: &[u8] = b"forge-dep-jar";
const MAVEN_FORGE: &[u8] = b"forge-from-maven-dir";
const MAVEN_UNIVERSAL: &[u8] = b"forge-universal-from-maven-dir";
const LEGACY_UNIVERSAL: &[u8] = b"legacy-universal-jar";
const LEGACY_ENTRY: &str = "forge-1.16.5-36.2.39-universal.jar";

/// Forge のライブラリの、`shared/libraries` からの相対パス。
fn forge_lib(classifier: Option<&str>) -> String {
    let file = match classifier {
        Some(c) => format!("forge-{FORGE}-{c}.jar"),
        None => format!("forge-{FORGE}.jar"),
    };
    format!("net/minecraftforge/forge/{FORGE}/{file}")
}

fn fake_java_bytes() -> Vec<u8> {
    std::fs::read(env!("CARGO_BIN_EXE_nexora-fake-java")).unwrap()
}

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(body).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// install_profile.json。minecraft の値だけを変えられる。
fn install_profile(base: &str, minecraft: &str, processor_jar: &[u8]) -> String {
    serde_json::json!({
        "spec": 0,
        "profile": "forge",
        "version": format!("{MC}-forge-36.2.39"),
        "minecraft": minecraft,
        "json": "/version.json",
        "data": {
            "BINPATCH": { "client": "/data/client.lzma", "server": "/data/server.lzma" },
            "PATCHED": {
                "client": format!("[net.minecraftforge:forge:{FORGE}:client]"),
                "server": format!("[net.minecraftforge:forge:{FORGE}:server]")
            }
        },
        "processors": [
            {
                "jar": "net.minecraftforge:binarypatcher:1.0.12",
                "classpath": [],
                "args": [
                    "{ROOT}/fake-calls.txt",
                    "{ROOT}/fake-control.txt",
                    "{PATCHED}",
                    "{BINPATCH}",
                    "{MINECRAFT_JAR}",
                    format!("[net.minecraftforge:forge:{FORGE}:universal]")
                ]
            },
            {
                "sides": ["server"],
                "jar": "net.minecraftforge:server-only:1.0",
                "args": []
            }
        ],
        "libraries": [
            {
                "name": "net.minecraftforge:binarypatcher:1.0.12",
                "downloads": { "artifact": {
                    "path": "net/minecraftforge/binarypatcher/1.0.12/binarypatcher-1.0.12.jar",
                    "url": format!("{base}{PROCESSOR_ROUTE}"),
                    "sha1": sha1_hex(processor_jar),
                    "size": processor_jar.len()
                }}
            },
            {
                // 実物の 1.16.5 と同じく URL が空。インストーラの `maven/` から置く
                "name": format!("net.minecraftforge:forge:{FORGE}:universal"),
                "downloads": { "artifact": {
                    "path": forge_lib(Some("universal")),
                    "url": "",
                    "sha1": sha1_hex(MAVEN_UNIVERSAL),
                    "size": MAVEN_UNIVERSAL.len()
                }}
            }
        ]
    })
    .to_string()
}

/// 入れ子の旧形式の install_profile.json（実物の 1.8.9 と同じ形。Forge 本体は URL を持たず、
/// `forge-dep` は URL だけを持ち、サーバー専用のものは `clientreq` が偽）。URL を持たないライブラリが
/// Mojang の取得元になることは、実ネットワークに出ないため `legacy.rs` の単体テストだけで確かめる。
fn legacy_install_profile(base: &str, file_path: &str) -> String {
    serde_json::json!({
        "install": {
            "profileName": "forge",
            "target": format!("{MC}-forge{FORGE}"),
            "path": format!("net.minecraftforge:forge:{FORGE}"),
            "version": format!("forge {FORGE}"),
            "filePath": file_path,
            "minecraft": MC
        },
        "versionInfo": {
            "id": format!("{MC}-forge{FORGE}"),
            "type": "release",
            "mainClass": "net.minecraft.launchwrapper.Launch",
            "inheritsFrom": MC,
            "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} --accessToken ${auth_access_token} --userType ${user_type} --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker",
            "libraries": [
                { "name": format!("net.minecraftforge:forge:{FORGE}"), "url": "https://maven.minecraftforge.net/" },
                { "name": "com.example:forge-dep:1.0", "url": format!("{base}/maven/"), "clientreq": true, "serverreq": true },
                { "name": "com.example:server-only:1.0", "url": format!("{base}/maven/"), "serverreq": true, "clientreq": false }
            ]
        }
    })
    .to_string()
}

fn version_json(base: &str) -> String {
    serde_json::json!({
        "id": format!("{MC}-forge-36.2.39"),
        "inheritsFrom": MC,
        "type": "release",
        "mainClass": "cpw.mods.modlauncher.Launcher",
        "arguments": { "game": ["--launchTarget", "fmlclient"], "jvm": [] },
        "libraries": [
            {
                "name": format!("net.minecraftforge:forge:{FORGE}"),
                "downloads": { "artifact": {
                    "path": forge_lib(None),
                    "url": "",
                    "sha1": sha1_hex(MAVEN_FORGE),
                    "size": MAVEN_FORGE.len()
                }}
            },
            {
                "name": format!("net.minecraftforge:forge:{FORGE}:client"),
                "downloads": { "artifact": {
                    "path": forge_lib(Some("client")),
                    "url": "",
                    "sha1": sha1_hex(b"patched"),
                    "size": 7
                }}
            },
            {
                "name": "com.example:forge-dep:1.0",
                "downloads": { "artifact": {
                    "path": "com/example/forge-dep/1.0/forge-dep-1.0.jar",
                    "url": format!("{base}{DEP_ROUTE}"),
                    "sha1": sha1_hex(DEP),
                    "size": DEP.len()
                }}
            }
        ]
    })
    .to_string()
}

/// 偽のインストーラ。profile が None なら、正しい install_profile.json を入れる。
/// `version` が Some なら、`version.json` をそれにする。
fn installer(base: &str, profile: Option<&str>, version: Option<&str>) -> Vec<u8> {
    let processor_jar = processor_jar();
    let good = install_profile(base, MC, &processor_jar);
    let profile = profile.unwrap_or(&good);
    let version = version.map_or_else(|| version_json(base), str::to_string);
    let forge = format!("maven/{}", forge_lib(None));
    let universal = format!("maven/{}", forge_lib(Some("universal")));
    zip_bytes(&[
        ("install_profile.json", profile.as_bytes()),
        ("version.json", version.as_bytes()),
        (&forge, MAVEN_FORGE),
        (&universal, MAVEN_UNIVERSAL),
        ("data/client.lzma", b"client-patches"),
        ("data/server.lzma", b"server-patches"),
        (LEGACY_ENTRY, LEGACY_UNIVERSAL),
    ])
}

fn processor_jar() -> Vec<u8> {
    zip_bytes(&[(
        "META-INF/MANIFEST.MF",
        b"Manifest-Version: 1.0\r\nMain-Class: fake.BinaryPatcher\r\n\r\n",
    )])
}

struct Harness {
    _temp: tempfile::TempDir,
    /// 止めた後は None。
    fake: Option<FakeMojang>,
    paths: AppPaths,
    launcher: Launcher,
    tasks: TaskManager,
    recorder: Arc<SnapshotRecorder>,
    store: Arc<MemoryStore>,
}

/// profile が None なら正しいインストーラを配る。
async fn harness(profile: Option<&str>) -> Harness {
    harness_with(|_| (profile.map(str::to_string), None)).await
}

/// サーバーの URL から install_profile.json を作る版。
async fn harness_with(profile: impl FnOnce(&str) -> (Option<String>, Option<String>)) -> Harness {
    let fake = FakeMojang::start_version(MC, &fake_java_bytes()).await;
    let u = fake.server.uri();
    let (profile, version) = profile(&u);
    let jar = installer(&u, profile.as_deref(), version.as_deref());
    serve(&fake.server, INSTALLER_ROUTE, &jar).await;
    serve(
        &fake.server,
        &format!("{INSTALLER_ROUTE}.sha1"),
        sha1_hex(&jar).as_bytes(),
    )
    .await;
    serve(&fake.server, PROCESSOR_ROUTE, &processor_jar()).await;
    serve(&fake.server, DEP_ROUTE, DEP).await;

    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    paths.ensure_dirs().unwrap();
    let (tasks, recorder) = SnapshotRecorder::manager();
    let urls = Urls {
        manifest: fake.manifest_url(),
        runtime_index: fake.runtime_index_url(),
        resources: fake.resources_url(),
        fabric_meta: "http://127.0.0.1:1".into(),
        quilt_meta: "http://127.0.0.1:1".into(),
        forge_maven: format!("{u}/forge"),
        forge_promotions: "http://127.0.0.1:1".into(),
        neoforge_maven: "http://127.0.0.1:1".into(),
        modrinth_api: "http://127.0.0.1:1".into(),
        modrinth_cdn: "http://127.0.0.1:1".into(),
    };
    let db = Db::open_in_memory().unwrap();
    let auth_server = MockServer::start().await;
    let store = Arc::new(MemoryStore::default());
    let auth = AuthService::new(
        db.clone(),
        paths.clone(),
        AuthHttp::new().unwrap(),
        auth_urls(&auth_server),
        Some(store.clone() as Arc<dyn SecretStore>),
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
    );
    Harness {
        _temp: temp,
        fake: Some(fake),
        paths,
        launcher,
        tasks,
        recorder,
        store,
    }
}

/// 入れ子の旧形式のインストーラを配る harness。`file_path` は `install.filePath`。
async fn harness_legacy(file_path: &str) -> Harness {
    harness_with(|base| (Some(legacy_install_profile(base, file_path)), None)).await
}

/// 1.12.2 の 2859 以降と同じ形（`spec: 0`、processor も data も無い）のインストーラを配る harness。
async fn harness_spec0_without_processors() -> Harness {
    harness_with(|base| {
        let profile = serde_json::json!({
            "spec": 0,
            "profile": "forge",
            "version": format!("{MC}-forge-36.2.39"),
            "minecraft": MC,
            "json": "/version.json",
            "data": {},
            "processors": [],
            "libraries": []
        });
        // `client` の最終 jar を作る processor が無いので、version.json は Forge 本体と依存だけを持つ
        let version = serde_json::json!({
            "id": format!("{MC}-forge-36.2.39"),
            "inheritsFrom": MC,
            "type": "release",
            "mainClass": "net.minecraft.launchwrapper.Launch",
            "minecraftArguments": "--tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker",
            "libraries": [
                { "name": format!("net.minecraftforge:forge:{FORGE}"),
                  "downloads": { "artifact": {
                      "path": forge_lib(None), "url": "",
                      "sha1": sha1_hex(MAVEN_FORGE), "size": MAVEN_FORGE.len() } } },
                { "name": "com.example:forge-dep:1.0",
                  "downloads": { "artifact": {
                      "path": "com/example/forge-dep/1.0/forge-dep-1.0.jar",
                      "url": format!("{base}{DEP_ROUTE}"),
                      "sha1": sha1_hex(DEP), "size": DEP.len() } } }
            ]
        });
        (Some(profile.to_string()), Some(version.to_string()))
    })
    .await
}

fn forge_instance(name: &str) -> NewInstance {
    NewInstance {
        loader: LoaderKind::Forge,
        loader_version: Some(FORGE.into()),
        ..NewInstance::vanilla(name, MC)
    }
}

async fn wait(tasks: &TaskManager, id: &str) -> TaskSnapshot {
    for _ in 0..1500 {
        let snap = tasks.get(id).unwrap();
        if snap.state.is_terminal() {
            return snap;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("task {id} did not finish");
}

impl Harness {
    fn shared(&self) -> PathBuf {
        self.paths.shared_dir()
    }

    fn profile_dir(&self) -> PathBuf {
        self.shared().join(format!("versions/forge-{FORGE}"))
    }

    fn record(&self) -> PathBuf {
        self.profile_dir().join("nexora-loader.json")
    }

    fn calls(&self) -> usize {
        std::fs::read_to_string(self.shared().join("fake-calls.txt"))
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count()
    }

    fn set_control(&self, mode: &str) {
        std::fs::write(self.shared().join("fake-control.txt"), mode).unwrap();
    }

    /// インスタンスを作り、インストールのタスクが終わるまで待つ。
    async fn create(&self, name: &str) -> (String, TaskSnapshot) {
        let created = self
            .launcher
            .create_instance(&forge_instance(name))
            .unwrap();
        let snap = wait(&self.tasks, &created.task_id).await;
        (created.instance.id, snap)
    }

    fn state(&self, id: &str) -> InstanceState {
        instance::get(self.launcher.db(), id).unwrap().state
    }

    /// Microsoft のアカウントを 1 件入れて使用中にする（オフラインの起動に使う）。
    fn seed(&self) {
        let id = Uuid::parse_str(PROFILE_ID).unwrap();
        accounts::save_profile(
            self.launcher.db(),
            &ProfileRecord {
                id,
                username: PROFILE_NAME.into(),
                xuid: Some(XUID.into()),
                skin_url: None,
            },
            EntitlementState::Verified,
            now_ms() - DAY_MS,
        )
        .unwrap();
        accounts::set_active(self.launcher.db(), id).unwrap();
        self.store
            .set(
                &id.simple().to_string(),
                &SecretString::from(REFRESH_1.to_string()),
            )
            .unwrap();
    }

    /// 通信せずに起動するタスクを動かし、終わるまで待つ。
    async fn launch_offline(&self, id: &str) -> TaskSnapshot {
        let task = self.launcher.launch_instance(id, true).unwrap();
        wait(&self.tasks, &task).await
    }

    /// staging の下に残っているもの。
    fn staging_leftovers(&self) -> Vec<PathBuf> {
        match std::fs::read_dir(self.paths.staging_dir()) {
            Ok(entries) => entries.flatten().map(|e| e.path()).collect(),
            Err(_) => Vec::new(),
        }
    }
}

#[tokio::test]
async fn forge_install_runs_processors_and_pins_profile() {
    let h = harness(None).await;
    let (id, snap) = h.create("forge").await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(h.state(&id), InstanceState::Ready);
    let dir = h.profile_dir();
    assert!(dir.join(format!("forge-{FORGE}.json")).is_file());
    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(h.record()).unwrap()).unwrap();
    assert_eq!(record["javaMajor"], 17);
    assert_eq!(record["minecraftJarSha1"], sha1_hex(b"client-jar"));
    assert_eq!(record["installerSha1"].as_str().unwrap().len(), 40);
    assert!(record["completedAt"].as_i64().unwrap() > 0);
    // 導入で置いたファイル（`maven/` の 2 つと、data の PATCHED）を shared からの相対で記録する
    let files: Vec<&str> = record["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for lib in [
        forge_lib(None),
        forge_lib(Some("universal")),
        forge_lib(Some("client")),
    ] {
        let rel = format!("libraries/{lib}");
        assert!(files.contains(&rel.as_str()), "{rel} not in {files:?}");
    }
    let libs = h.shared().join("libraries");
    // processor の出力と、`maven/` から置いたもの
    assert_eq!(
        std::fs::read(libs.join(forge_lib(Some("client")))).unwrap(),
        b"patched"
    );
    assert_eq!(
        std::fs::read(libs.join(forge_lib(None))).unwrap(),
        MAVEN_FORGE
    );
    assert!(libs.join(forge_lib(Some("universal"))).is_file());
    // ゲームのライブラリと、processor の jar は取得する
    assert!(
        libs.join("com/example/forge-dep/1.0/forge-dep-1.0.jar")
            .is_file()
    );
    assert!(
        libs.join("net/minecraftforge/binarypatcher/1.0.12/binarypatcher-1.0.12.jar")
            .is_file()
    );
    // client の processor だけが 1 回動き、資源の作業場所は残らない
    assert_eq!(h.calls(), 1);
    assert!(
        h.staging_leftovers().is_empty(),
        "{:?}",
        h.staging_leftovers()
    );
}

#[tokio::test]
async fn forge_install_reports_loader_phases() {
    let h = harness(None).await;
    let (_, snap) = h.create("forge").await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    // ゲームのファイルの後に、ローダーの情報、ライブラリの確認と取得、processor の実行、記録と進む
    assert_eq!(
        h.recorder.phases(&snap.id),
        vec![
            TaskPhase::Resolving,
            TaskPhase::Verifying,
            TaskPhase::Downloading,
            TaskPhase::Resolving,
            TaskPhase::Verifying,
            TaskPhase::Downloading,
            TaskPhase::BuildingLoader,
            TaskPhase::Finishing,
        ]
    );
    // processor の数は始める前から分かる（この導入では 1 つ）。済んだ数の通知は間引かれうるため、最初だけを確かめる
    let building = h
        .recorder
        .snapshots(&snap.id)
        .into_iter()
        .find(|s| s.phase == Some(TaskPhase::BuildingLoader) && s.progress.total > 0)
        .map(|s| (s.progress.current, s.progress.total));
    assert_eq!(building, Some((0, 1)));
}

#[tokio::test]
async fn second_instance_of_same_version_skips_processors() {
    let h = harness(None).await;
    let (_, first) = h.create("one").await;
    assert_eq!(first.state, TaskState::Succeeded, "{:?}", first.error);
    assert_eq!(h.calls(), 1);
    let (id, second) = h.create("two").await;
    assert_eq!(second.state, TaskState::Succeeded, "{:?}", second.error);
    assert_eq!(h.state(&id), InstanceState::Ready);
    assert_eq!(h.calls(), 1);
    // インストーラは取得済みのものを使う
    let server = &h.fake.as_ref().unwrap().server;
    assert_eq!(hits_on(server, INSTALLER_ROUTE).await, 1);
}

#[tokio::test]
async fn reinstall_after_failed_processor_runs_again() {
    let h = harness(None).await;
    h.set_control("fail");
    let (id, first) = h.create("forge").await;
    assert_eq!(first.state, TaskState::Failed);
    assert_eq!(first.error.unwrap().code, "install.processor_failed");
    assert_eq!(h.state(&id), InstanceState::Broken);
    assert!(!h.record().exists());
    assert_eq!(h.calls(), 1);
    assert!(
        h.staging_leftovers().is_empty(),
        "{:?}",
        h.staging_leftovers()
    );

    // 直した後にプレイを押すと、もう一度導入して processor を動かす
    h.set_control("ok");
    let task = h.launcher.launch_instance(&id, true).unwrap();
    let second = wait(&h.tasks, &task).await;
    assert_eq!(second.kind, "install");
    assert_eq!(second.state, TaskState::Succeeded, "{:?}", second.error);
    assert_eq!(h.state(&id), InstanceState::Ready);
    assert_eq!(h.calls(), 2);
    assert!(h.record().is_file());
}

#[tokio::test]
async fn cancel_leaves_no_completion_record() {
    let h = harness(None).await;
    h.set_control("spin");
    let created = h
        .launcher
        .create_instance(&forge_instance("forge"))
        .unwrap();
    for _ in 0..1500 {
        if h.calls() > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(h.calls(), 1, "the processor never started");
    h.tasks.cancel(&created.task_id).unwrap();
    let snap = wait(&h.tasks, &created.task_id).await;
    assert_eq!(snap.state, TaskState::Cancelled, "{:?}", snap.error);
    assert_eq!(h.state(&created.instance.id), InstanceState::Installing);
    assert!(!h.record().exists());
    assert!(
        h.staging_leftovers().is_empty(),
        "{:?}",
        h.staging_leftovers()
    );
}

#[tokio::test]
async fn mismatched_minecraft_version_is_invalid_metadata() {
    let base = "http://unused.invalid";
    let profile = install_profile(base, "1.16.4", &processor_jar());
    let h = harness(Some(&profile)).await;
    let (id, snap) = h.create("forge").await;
    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "meta.invalid");
    assert_eq!(h.state(&id), InstanceState::Broken);
    assert!(!h.profile_dir().join(format!("forge-{FORGE}.json")).exists());
    assert_eq!(h.calls(), 0);
}

#[tokio::test]
async fn legacy_nested_installer_installs_the_universal_jar() {
    let h = harness_legacy(LEGACY_ENTRY).await;
    let (id, snap) = h.create("forge").await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(h.state(&id), InstanceState::Ready);
    let libs = h.shared().join("libraries");
    // Forge 本体は、インストーラ直下の universal jar を座標のパスに置いたもの
    assert_eq!(
        std::fs::read(libs.join(forge_lib(None))).unwrap(),
        LEGACY_UNIVERSAL
    );
    assert!(
        libs.join("com/example/forge-dep/1.0/forge-dep-1.0.jar")
            .is_file()
    );
    // 固定したプロファイルは、起動引数と Mojang 側の取得元を持ち、サーバー専用のライブラリを持たない
    let pinned =
        std::fs::read_to_string(h.profile_dir().join(format!("forge-{FORGE}.json"))).unwrap();
    assert!(pinned.contains("--tweakClass"), "{pinned}");
    assert!(!pinned.contains("server-only"), "{pinned}");
    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(h.record()).unwrap()).unwrap();
    let files: Vec<&str> = record["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let rel = format!("libraries/{}", forge_lib(None));
    assert!(files.contains(&rel.as_str()), "{rel} not in {files:?}");
    // processor は無く、作業場所も残らない
    assert_eq!(h.calls(), 0);
    assert!(h.staging_leftovers().is_empty());
}

#[tokio::test]
async fn spec_zero_installer_without_processors_installs_the_maven_files() {
    let h = harness_spec0_without_processors().await;
    let (id, snap) = h.create("forge").await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(h.state(&id), InstanceState::Ready);
    let libs = h.shared().join("libraries");
    assert_eq!(
        std::fs::read(libs.join(forge_lib(None))).unwrap(),
        MAVEN_FORGE
    );
    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(h.record()).unwrap()).unwrap();
    let files: Vec<&str> = record["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let rel = format!("libraries/{}", forge_lib(None));
    assert!(files.contains(&rel.as_str()), "{rel} not in {files:?}");
    assert_eq!(h.calls(), 0);
}

#[tokio::test]
async fn legacy_nested_installer_without_the_universal_entry_fails_without_a_record() {
    let h = harness_legacy("missing-universal.jar").await;
    let (id, snap) = h.create("forge").await;
    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "meta.invalid");
    assert_eq!(h.state(&id), InstanceState::Broken);
    assert!(!h.record().exists());
    assert!(!h.shared().join("libraries").join(forge_lib(None)).exists());
}

#[tokio::test]
async fn launch_fails_with_loader_files_missing() {
    let h = harness(None).await;
    let (id, snap) = h.create("forge").await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    h.seed();
    let patched = h.shared().join("libraries").join(forge_lib(Some("client")));
    std::fs::remove_file(&patched).unwrap();
    let launch = h.launch_offline(&id).await;
    assert_eq!(launch.state, TaskState::Failed);
    let error = launch.error.unwrap();
    assert_eq!(error.code, "install.loader_files_missing");
    let path = format!("{:?}", error.params.get("path"));
    assert!(path.contains("forge-1.16.5-36.2.39-client.jar"), "{path}");
    // 壊れた状態にし、次にプレイを押すと導入し直す
    assert_eq!(h.state(&id), InstanceState::Broken);
    let task = h.launcher.launch_instance(&id, true).unwrap();
    let again = wait(&h.tasks, &task).await;
    assert_eq!(again.kind, "install");
    assert_eq!(again.state, TaskState::Succeeded, "{:?}", again.error);
    assert_eq!(h.state(&id), InstanceState::Ready);
    assert_eq!(std::fs::read(&patched).unwrap(), b"patched");
    assert_eq!(h.calls(), 2);
    let launch = h.launch_offline(&id).await;
    assert_eq!(launch.state, TaskState::Succeeded, "{:?}", launch.error);
}

#[tokio::test]
async fn missing_installer_provided_file_fails_launch_and_reinstalls() {
    let h = harness(None).await;
    let (id, snap) = h.create("forge").await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    h.seed();
    // `maven/` から置いたもののうち、ゲームのライブラリに無い（`generated` に入らない）もの
    let universal = h
        .shared()
        .join("libraries")
        .join(forge_lib(Some("universal")));
    std::fs::remove_file(&universal).unwrap();

    let launch = h.launch_offline(&id).await;
    assert_eq!(launch.state, TaskState::Failed);
    let error = launch.error.unwrap();
    assert_eq!(error.code, "install.loader_files_missing");
    let path = format!("{:?}", error.params.get("path"));
    assert!(
        path.contains("forge-1.16.5-36.2.39-universal.jar"),
        "{path}"
    );
    assert_eq!(h.state(&id), InstanceState::Broken);

    // 完了記録は入力と一致するが、ファイルが欠けているので processor をもう一度動かす
    let task = h.launcher.launch_instance(&id, true).unwrap();
    let again = wait(&h.tasks, &task).await;
    assert_eq!(again.kind, "install");
    assert_eq!(again.state, TaskState::Succeeded, "{:?}", again.error);
    assert_eq!(h.calls(), 2);
    assert!(universal.is_file());
    let launch = h.launch_offline(&id).await;
    assert_eq!(launch.state, TaskState::Succeeded, "{:?}", launch.error);
}

#[tokio::test]
async fn record_without_the_file_list_is_reinstalled() {
    let h = harness(None).await;
    let (_, first) = h.create("one").await;
    assert_eq!(first.state, TaskState::Succeeded, "{:?}", first.error);
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(h.record()).unwrap()).unwrap();
    record.as_object_mut().unwrap().remove("files");
    std::fs::write(h.record(), record.to_string()).unwrap();
    let (_, second) = h.create("two").await;
    assert_eq!(second.state, TaskState::Succeeded, "{:?}", second.error);
    assert_eq!(h.calls(), 2);
    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(h.record()).unwrap()).unwrap();
    assert!(record["files"].is_array());
}

#[tokio::test]
async fn offline_launch_does_not_fetch_installer() {
    let mut h = harness(None).await;
    let (id, snap) = h.create("forge").await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    h.seed();
    // サーバーを止め、取得済みのインストーラも消す。起動にはどちらも要らない
    drop(h.fake.take());
    std::fs::remove_dir_all(h.paths.installers_dir()).unwrap();
    let launch = h.launch_offline(&id).await;
    assert_eq!(launch.state, TaskState::Succeeded, "{:?}", launch.error);
}

#[tokio::test]
async fn concurrent_installs_of_same_version_run_processors_once() {
    let h = harness(None).await;
    // 遅い processor にして、2 つ目の導入が 1 つ目の実行中に追いつくようにする
    h.set_control("slow");
    let a = h.launcher.create_instance(&forge_instance("a")).unwrap();
    let b = h.launcher.create_instance(&forge_instance("b")).unwrap();
    let first = wait(&h.tasks, &a.task_id).await;
    let second = wait(&h.tasks, &b.task_id).await;
    assert_eq!(first.state, TaskState::Succeeded, "{:?}", first.error);
    assert_eq!(second.state, TaskState::Succeeded, "{:?}", second.error);
    assert_eq!(h.calls(), 1);
    assert!(h.record().is_file());
    assert_eq!(h.state(&a.instance.id), InstanceState::Ready);
    assert_eq!(h.state(&b.instance.id), InstanceState::Ready);
}

/// Forge の Maven（`/forge/` の下）への要求の数。
async fn forge_maven_hits(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path().starts_with("/forge/"))
        .count()
}

#[tokio::test]
async fn create_refuses_versions_outside_the_supported_range() {
    let h = harness(None).await;
    let cases = [
        // 対応範囲（Forge は 1.16.5 以降）の外
        (LoaderKind::Forge, "1.16.4", "1.16.4-35.1.37"),
        // ローダーの版が別の Minecraft のバージョン向け
        (LoaderKind::Forge, MC, "1.20.1-47.3.0"),
        // NeoForge は 1.20.1 以降
        (LoaderKind::Neoforge, "1.19.4", "1.19.4-45.1.0"),
        (LoaderKind::Neoforge, "1.20.1", "21.1.172"),
    ];
    for (loader, mc, version) in cases {
        let new = NewInstance {
            loader,
            loader_version: Some(version.into()),
            ..NewInstance::vanilla("x", mc)
        };
        let err = h.launcher.create_instance(&new).unwrap_err();
        assert_eq!(err.code(), "install.unsupported_profile", "{mc} {version}");
    }
    assert!(h.launcher.list_instances().unwrap().is_empty());
    assert_eq!(forge_maven_hits(&h.fake.as_ref().unwrap().server).await, 0);
}

#[tokio::test]
async fn install_refuses_a_loader_for_another_minecraft_version_without_fetching() {
    let h = harness(None).await;
    // 作った後に行が書き換えられた（古い版で作った行など）ものを、導入の時点でも断る
    let inst = instance::create(
        h.launcher.db(),
        &h.paths,
        &forge_instance("forge"),
        now_ms(),
    )
    .unwrap();
    h.launcher
        .db()
        .with_conn(|conn| {
            conn.execute(
                "UPDATE instances SET loader_version = '1.20.1-47.3.0' WHERE id = ?1",
                rusqlite::params![inst.id],
            )?;
            Ok(())
        })
        .unwrap();
    let task = h.launcher.launch_instance(&inst.id, true).unwrap();
    let snap = wait(&h.tasks, &task).await;
    assert_eq!(snap.kind, "install");
    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "install.unsupported_profile");
    assert_eq!(h.state(&inst.id), InstanceState::Broken);
    assert_eq!(forge_maven_hits(&h.fake.as_ref().unwrap().server).await, 0);
}

impl Harness {
    fn pinned_json(&self) -> PathBuf {
        self.profile_dir().join(format!("forge-{FORGE}.json"))
    }

    /// 起動が `install.loader_files_missing`（固定済みの version.json のパス）で止まり、
    /// インストーラを取りに行かないこと。取得済みのインストーラは消しておき、取りに行けば必ず数に出るようにする。
    async fn assert_launch_needs_reinstall(&self, id: &str) {
        std::fs::remove_dir_all(self.paths.installers_dir()).unwrap();
        let server = &self.fake.as_ref().unwrap().server;
        let before = hits_on(server, INSTALLER_ROUTE).await;
        let launch = self.launch_offline(id).await;
        assert_eq!(launch.state, TaskState::Failed);
        let error = launch.error.unwrap();
        assert_eq!(error.code, "install.loader_files_missing", "{error:?}");
        let path = format!("{:?}", error.params.get("path"));
        assert!(path.contains(&format!("forge-{FORGE}.json")), "{path}");
        assert_eq!(self.state(id), InstanceState::Broken);
        assert_eq!(hits_on(server, INSTALLER_ROUTE).await, before);
        assert_eq!(hits_on(server, &format!("{INSTALLER_ROUTE}.sha1")).await, 1);
    }
}

#[tokio::test]
async fn launch_without_pinned_profile_is_loader_files_missing() {
    let h = harness(None).await;
    let (id, snap) = h.create("forge").await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    h.seed();
    std::fs::remove_file(h.pinned_json()).unwrap();
    h.assert_launch_needs_reinstall(&id).await;
    assert!(!h.pinned_json().exists());
}

#[tokio::test]
async fn launch_with_pinned_profile_for_another_version_is_loader_files_missing() {
    let h = harness(None).await;
    let (id, snap) = h.create("forge").await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    h.seed();
    let mut pinned: serde_json::Value =
        serde_json::from_slice(&std::fs::read(h.pinned_json()).unwrap()).unwrap();
    pinned["inheritsFrom"] = "1.16.4".into();
    std::fs::write(h.pinned_json(), pinned.to_string()).unwrap();
    h.assert_launch_needs_reinstall(&id).await;
    // 起動は固定済みのファイルを書き換えない
    let after: serde_json::Value =
        serde_json::from_slice(&std::fs::read(h.pinned_json()).unwrap()).unwrap();
    assert_eq!(after["inheritsFrom"], "1.16.4");
}
