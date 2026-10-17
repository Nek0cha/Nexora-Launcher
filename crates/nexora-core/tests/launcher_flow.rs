mod common;

use std::sync::Arc;
use std::time::Duration;

use nexora_core::auth::accounts::{self, EntitlementState, ProfileRecord};
use nexora_core::auth::http::AuthHttp;
use nexora_core::auth::login::{
    LOGIN_KIND, LoginMethod, LoginNotifier, LoginProgressEvent, UrlOpener,
};
use nexora_core::auth::service::AuthService;
use nexora_core::auth::store::{MemoryStore, SecretStore};
use nexora_core::db::Db;
use nexora_core::error::ParamValue;
use nexora_core::install::{Urls, now_ms};
use nexora_core::instance::{self, InstanceState};
use nexora_core::launcher::{INSTALL_KIND, LAUNCH_KIND, Launcher};
use nexora_core::paths::AppPaths;
use nexora_core::task::{TaskChange, TaskManager, TaskNotifier, TaskSnapshot, TaskState};
use secrecy::SecretString;
use uuid::Uuid;
use wiremock::MockServer;

use common::{
    FakeMojang, FixedProbe, Ownership, PROFILE_ID, PROFILE_NAME, REFRESH_1, REFRESH_2,
    RecordingGame, XUID, auth_urls, hits_on, http, mount_chain, mount_refresh,
};

#[cfg(unix)]
use common::MC_TOKEN;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

struct Silent;
impl TaskNotifier for Silent {
    fn notify(&self, _: &str, _: u64, _: TaskChange) {}
}

struct Harness {
    _temp: tempfile::TempDir,
    launcher: Launcher,
    tasks: TaskManager,
    game: Arc<RecordingGame>,
    auth_server: MockServer,
    store: Arc<MemoryStore>,
}

/// フェーズ 2 と同じく、開発用のオフラインアカウントで起動する環境。
async fn harness(fake: &FakeMojang) -> Harness {
    let h = harness_with(fake).await;
    #[cfg(debug_assertions)]
    nexora_core::launch::session::set_dev_offline_active(h.launcher.db(), true, 0).unwrap();
    h
}

/// アカウントのない環境。認証の通信は auth_server に向く。
async fn harness_with(fake: &FakeMojang) -> Harness {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    paths.ensure_dirs().unwrap();
    let tasks = TaskManager::new(Arc::new(Silent));
    let game = Arc::new(RecordingGame::default());
    let urls = Urls {
        manifest: fake.manifest_url(),
        runtime_index: fake.runtime_index_url(),
        resources: fake.resources_url(),
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
        paths,
        db,
        tasks.clone(),
        game.clone(),
        http(),
        urls,
        Arc::new(FixedProbe(17)),
        auth,
    );
    Harness {
        _temp: temp,
        launcher,
        tasks,
        game,
        auth_server,
        store,
    }
}

fn profile_id() -> Uuid {
    Uuid::parse_str(PROFILE_ID).unwrap()
}

impl Harness {
    /// Microsoft のアカウントを 1 件入れて使用中にし、store に REFRESH_1 を置く。
    fn seed(&self, verified_at: i64) -> Uuid {
        let id = profile_id();
        accounts::save_profile(
            self.launcher.db(),
            &ProfileRecord {
                id,
                username: PROFILE_NAME.into(),
                xuid: Some(XUID.into()),
                skin_url: None,
            },
            EntitlementState::Verified,
            verified_at,
        )
        .unwrap();
        accounts::set_active(self.launcher.db(), id).unwrap();
        self.store
            .set(
                &id.simple().to_string(),
                &SecretString::from(REFRESH_1.to_string()),
            )
            .unwrap();
        id
    }

    /// インスタンスを作り、インストールが終わるまで待つ。
    async fn installed(&self) -> String {
        let created = self.launcher.create_instance("x", "1.99-test").unwrap();
        let snap = wait(&self.tasks, &created.task_id).await;
        assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
        created.instance.id
    }
}

/// 起動のタスクがゲームの起動まで進んだこと。Windows では偽の Java を実行できないため、
/// 起動の失敗（launch.spawn_failed）までを確かめる。
fn assert_reached_spawn(snap: &TaskSnapshot) {
    if cfg!(unix) {
        assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    } else if snap.state != TaskState::Succeeded {
        assert_eq!(snap.state, TaskState::Failed);
        assert_eq!(
            snap.error.as_ref().unwrap().code,
            "launch.spawn_failed",
            "{:?}",
            snap.error
        );
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

/// instance_id のタスクの kind を、作成の古い順に返す。
fn kinds_for(tasks: &TaskManager, instance_id: &str) -> Vec<String> {
    let mut list: Vec<TaskSnapshot> = tasks
        .list()
        .into_iter()
        .filter(|t| t.instance_id.as_deref() == Some(instance_id))
        .collect();
    list.reverse();
    list.into_iter().map(|t| t.kind).collect()
}

fn insert_running(launcher: &Launcher, instance_id: &str, pid: u32) {
    launcher
        .db()
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO running_sessions VALUES (?1, ?2, 1, 1, 'acc', 'x.log')",
                rusqlite::params![instance_id, pid],
            )?;
            Ok(())
        })
        .unwrap();
}

#[tokio::test]
async fn lists_supported_versions() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let ids: Vec<_> = h
        .launcher
        .list_versions()
        .await
        .unwrap()
        .into_iter()
        .map(|v| v.id)
        .collect();
    assert_eq!(ids, vec!["1.99-test", "1.8.9"]);
}

#[tokio::test]
async fn create_installs_and_play_during_install_reuses_the_task() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h.launcher.create_instance("テスト", "1.99-test").unwrap();
    assert_eq!(created.instance.state, InstanceState::Installing);
    let again = h
        .launcher
        .launch_instance(&created.instance.id, false)
        .unwrap();
    assert_eq!(again, created.task_id);
    let snap = wait(&h.tasks, &created.task_id).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(snap.kind, "install");
    assert_eq!(
        snap.instance_id.as_deref(),
        Some(created.instance.id.as_str())
    );
    let listed = h.launcher.list_instances().unwrap();
    assert_eq!(listed[0].state, InstanceState::Ready);
}

#[tokio::test]
async fn repeated_play_during_install_creates_no_other_task() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h.launcher.create_instance("x", "1.99-test").unwrap();
    let id = &created.instance.id;
    // インストールの途中で何度押しても、同じインストールのタスクを返す
    for _ in 0..5 {
        assert_eq!(
            h.launcher.launch_instance(id, false).unwrap(),
            created.task_id
        );
    }
    assert_eq!(kinds_for(&h.tasks, id), vec![INSTALL_KIND]);
    wait(&h.tasks, &created.task_id).await;
    assert!(h.game.started.lock().unwrap().is_empty());
}

#[tokio::test]
async fn repeated_play_reuses_the_launch_task() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h.launcher.create_instance("x", "1.99-test").unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = &created.instance.id;
    let first = h.launcher.launch_instance(id, false).unwrap();
    for _ in 0..5 {
        match h.launcher.launch_instance(id, false) {
            Ok(t) => assert_eq!(t, first),
            Err(e) => assert_eq!(e.code(), "launch.already_running"),
        }
    }
    assert_eq!(kinds_for(&h.tasks, id), vec![INSTALL_KIND, LAUNCH_KIND]);
    wait(&h.tasks, &first).await;
    // Windows では偽の Java を実行できないため、起動の成否は問わない。起動は多くても 1 回
    assert!(h.game.started.lock().unwrap().len() <= 1);
}

#[tokio::test]
async fn launch_is_refused_while_running() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h.launcher.create_instance("x", "1.99-test").unwrap();
    wait(&h.tasks, &created.task_id).await;
    h.launcher
        .db()
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO running_sessions VALUES (?1, 1, 1, 1, 'acc', 'x.log')",
                [&created.instance.id],
            )?;
            Ok(())
        })
        .unwrap();
    let err = h
        .launcher
        .launch_instance(&created.instance.id, false)
        .unwrap_err();
    assert_eq!(err.code(), "launch.already_running");
}

#[tokio::test]
async fn install_is_refused_while_running() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h.launcher.create_instance("x", "1.99-test").unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = &created.instance.id;
    instance::set_state(h.launcher.db(), id, InstanceState::Broken).unwrap();
    insert_running(&h.launcher, id, 1);
    let err = h.launcher.launch_instance(id, false).unwrap_err();
    assert_eq!(err.code(), "launch.already_running");
    assert_eq!(kinds_for(&h.tasks, id), vec![INSTALL_KIND]);
}

#[tokio::test]
async fn reattach_runs_only_once() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h.launcher.create_instance("x", "1.99-test").unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = &created.instance.id;
    // 存在しない PID の記録は、引き継ぎのときに終了済みとして片付けられる
    insert_running(&h.launcher, id, u32::MAX - 1);
    h.launcher.reattach().unwrap();
    assert_eq!(h.game.exits.lock().unwrap().len(), 1);
    assert!(h.launcher.list_running().unwrap().is_empty());
    // 2 回目は何もしない（監視や終了の通知を重ねない）
    insert_running(&h.launcher, id, u32::MAX - 1);
    h.launcher.reattach().unwrap();
    assert_eq!(h.game.exits.lock().unwrap().len(), 1);
    assert_eq!(h.launcher.list_running().unwrap().len(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn play_launches_the_game_and_records_the_session() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h.launcher.create_instance("x", "1.99-test").unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = &created.instance.id;
    let task = h.launcher.launch_instance(id, false).unwrap();
    let again = h.launcher.launch_instance(id, false);
    // 連打しても 2 件目の起動のタスクは作らない（同じ ID か、実行中のエラー）
    match again {
        Ok(t) => assert_eq!(t, task),
        Err(e) => assert_eq!(e.code(), "launch.already_running"),
    }
    let snap = wait(&h.tasks, &task).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(snap.kind, "launch");
    assert_eq!(h.game.wait_exit(id).await, Some(0));
    let messages = h.game.messages().join("\n");
    assert!(messages.contains("fake game"), "{messages}");
    assert!(messages.contains("--username Player"), "{messages}");
    assert_eq!(h.game.started.lock().unwrap().len(), 1);
    let inst = instance::get(h.launcher.db(), id).unwrap();
    assert!(inst.last_played_at.is_some());
    assert!(h.launcher.list_running().unwrap().is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn installed_instance_launches_offline() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h.launcher.create_instance("x", "1.99-test").unwrap();
    wait(&h.tasks, &created.task_id).await;
    drop(fake);
    let task = h
        .launcher
        .launch_instance(&created.instance.id, false)
        .unwrap();
    let snap = wait(&h.tasks, &task).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
}

#[tokio::test]
async fn failed_spawn_does_not_update_last_played() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h.launcher.create_instance("x", "1.99-test").unwrap();
    let installed = wait(&h.tasks, &created.task_id).await;
    assert_eq!(
        installed.state,
        TaskState::Succeeded,
        "{:?}",
        installed.error
    );
    let id = &created.instance.id;
    // 登録済みの Java を、どの OS でも実行できないファイルに差し替える
    let java = nexora_core::java::find_registered(h.launcher.db(), "jre-test")
        .unwrap()
        .unwrap();
    std::fs::write(&java, b"not an executable").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&java, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    let task = h.launcher.launch_instance(id, false).unwrap();
    let snap = wait(&h.tasks, &task).await;
    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "launch.spawn_failed");
    let inst = instance::get(h.launcher.db(), id).unwrap();
    assert_eq!(inst.last_played_at, None);
    assert!(h.launcher.list_running().unwrap().is_empty());
}

// ---- Microsoft のアカウントでの起動（フェーズ 3） ----

#[tokio::test]
async fn launch_gets_the_session_before_spawning() {
    let fake = FakeMojang::start().await;
    let h = harness_with(&fake).await;
    let id = h.installed().await;
    h.seed(now_ms());
    mount_refresh(&h.auth_server, Some(REFRESH_1), Some(REFRESH_2)).await;
    mount_chain(&h.auth_server, 86_400, Ownership::Owned).await;
    let task = h.launcher.launch_instance(&id, false).unwrap();
    let snap = wait(&h.tasks, &task).await;
    assert_reached_spawn(&snap);
    assert_eq!(hits_on(&h.auth_server, "/mc/login").await, 1);
    #[cfg(unix)]
    {
        assert_eq!(h.game.wait_exit(&id).await, Some(0));
        let messages = h.game.messages().join("\n");
        // 起動引数に Minecraft のトークンが入る（出力では伏せられる）
        assert!(messages.contains("--accessToken [REDACTED]"), "{messages}");
        assert!(!messages.contains(MC_TOKEN), "{messages}");
        assert!(messages.contains("--username Steve"), "{messages}");
        assert!(
            messages.contains(&format!("--uuid {PROFILE_ID}")),
            "{messages}"
        );
        assert!(messages.contains("--userType msa"), "{messages}");
    }
}

#[tokio::test]
async fn launch_without_an_account_fails_with_auth_no_account() {
    let fake = FakeMojang::start().await;
    let h = harness_with(&fake).await;
    let id = h.installed().await;
    let task = h.launcher.launch_instance(&id, false).unwrap();
    let snap = wait(&h.tasks, &task).await;
    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "auth.no_account");
    assert!(h.game.started.lock().unwrap().is_empty());
}

#[tokio::test]
async fn launch_offline_uses_the_cached_identity() {
    let fake = FakeMojang::start().await;
    let h = harness_with(&fake).await;
    let id = h.installed().await;
    h.seed(now_ms() - DAY_MS);
    let task = h.launcher.launch_instance(&id, true).unwrap();
    let snap = wait(&h.tasks, &task).await;
    assert_reached_spawn(&snap);
    // 認証の通信はしない
    assert!(h.auth_server.received_requests().await.unwrap().is_empty());
    #[cfg(unix)]
    {
        assert_eq!(h.game.wait_exit(&id).await, Some(0));
        let messages = h.game.messages().join("\n");
        assert!(messages.contains("--username Steve"), "{messages}");
        assert!(
            messages.contains(&format!("--uuid {PROFILE_ID}")),
            "{messages}"
        );
        assert!(messages.contains("--accessToken 0"), "{messages}");
        assert!(messages.contains(&format!("--xuid {XUID}")), "{messages}");
        assert!(messages.contains("--userType msa"), "{messages}");
    }
}

#[tokio::test]
async fn launch_offline_outside_the_grace_period_fails() {
    let fake = FakeMojang::start().await;
    let h = harness_with(&fake).await;
    let id = h.installed().await;
    h.seed(now_ms() - 31 * DAY_MS);
    let task = h.launcher.launch_instance(&id, true).unwrap();
    let snap = wait(&h.tasks, &task).await;
    assert_eq!(snap.state, TaskState::Failed);
    let error = snap.error.unwrap();
    assert_eq!(error.code, "auth.offline");
    assert_eq!(
        error.params.get("offlineAvailable"),
        Some(&ParamValue::Bool(false))
    );
    assert!(h.game.started.lock().unwrap().is_empty());
    assert!(h.auth_server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn removing_an_account_with_a_running_game_fails() {
    let fake = FakeMojang::start().await;
    let h = harness_with(&fake).await;
    let id = h.installed().await;
    let account = h.seed(now_ms());
    h.launcher
        .db()
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO running_sessions VALUES (?1, 1, 1, 1, ?2, 'x.log')",
                [id, account.simple().to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    let err = h.launcher.remove_account(account).await.unwrap_err();
    assert_eq!(err.code(), "auth.account_in_use");
    assert_eq!(h.launcher.auth().list().unwrap().accounts.len(), 1);
    assert!(h.store.value(PROFILE_ID).is_some());

    // ゲームが終われば削除できる
    h.launcher
        .db()
        .with_conn(|conn| {
            conn.execute("DELETE FROM running_sessions", [])?;
            Ok(())
        })
        .unwrap();
    h.launcher.remove_account(account).await.unwrap();
    assert!(h.launcher.auth().list().unwrap().accounts.is_empty());
    assert!(h.store.value(PROFILE_ID).is_none());
}

struct NoBrowser;
impl UrlOpener for NoBrowser {
    fn open(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

struct NoProgress;
impl LoginNotifier for NoProgress {
    fn progress(&self, _: &LoginProgressEvent) {}
}

#[tokio::test]
async fn login_task_is_unique() {
    let fake = FakeMojang::start().await;
    let h = harness_with(&fake).await;
    let first = h.launcher.begin_login(
        LoginMethod::Browser,
        Arc::new(NoBrowser),
        Arc::new(NoProgress),
    );
    let second = h.launcher.begin_login(
        LoginMethod::Browser,
        Arc::new(NoBrowser),
        Arc::new(NoProgress),
    );
    assert_eq!(first, second);
    let snap = h.tasks.get(&first).unwrap();
    assert_eq!(snap.kind, LOGIN_KIND);
    assert_eq!(snap.instance_id, None);
    h.tasks.cancel(&first).unwrap();
    assert_eq!(wait(&h.tasks, &first).await.state, TaskState::Cancelled);
}

#[tokio::test]
async fn login_after_a_cancelled_login_starts_a_new_task() {
    let fake = FakeMojang::start().await;
    let h = harness_with(&fake).await;
    let begin = |method| {
        h.launcher
            .begin_login(method, Arc::new(NoBrowser), Arc::new(NoProgress))
    };
    let first = begin(LoginMethod::Browser);
    h.tasks.cancel(&first).unwrap();
    assert_eq!(wait(&h.tasks, &first).await.state, TaskState::Cancelled);
    let second = begin(LoginMethod::DeviceCode);
    assert_ne!(first, second);
    h.tasks.cancel(&second).unwrap();
    wait(&h.tasks, &second).await;
}
