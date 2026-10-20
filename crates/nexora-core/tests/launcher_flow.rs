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
use nexora_core::instance::folder::FolderKind;
use nexora_core::instance::{
    self, InstanceSettings, InstanceState, JavaMode, MemoryMode, NewInstance,
};
use nexora_core::launcher::{DUPLICATE_KIND, INSTALL_KIND, LAUNCH_KIND, Launcher};
use nexora_core::paths::AppPaths;
use nexora_core::task::{TaskChange, TaskManager, TaskNotifier, TaskSnapshot, TaskState};
use secrecy::SecretString;
use uuid::Uuid;
use wiremock::MockServer;

use common::{
    FAKE_JAVA, FakeMojang, FixedProbe, Ownership, PROFILE_ID, PROFILE_NAME, REFRESH_1, REFRESH_2,
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
        let created = self
            .launcher
            .create_instance(&NewInstance::vanilla("x", "1.99-test"))
            .unwrap();
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
async fn stop_instance_of_missing_instance_is_not_found() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let err = h.launcher.stop_instance("missing", false).unwrap_err();
    assert_eq!(err.code(), "instance.not_found");
}

#[tokio::test]
async fn instance_folder_creates_missing_dirs_under_the_game_dir() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = &created.instance.id;
    let game = h.launcher.instance_folder(id, FolderKind::Game).unwrap();
    assert!(game.is_dir());
    for (kind, name) in [
        (FolderKind::Mods, "mods"),
        (FolderKind::Screenshots, "screenshots"),
        (FolderKind::Saves, "saves"),
    ] {
        let dir = h.launcher.instance_folder(id, kind).unwrap();
        assert!(dir.is_dir());
        assert_eq!(dir, game.join(name));
    }
}

#[tokio::test]
async fn instance_folder_logs_is_the_game_log_dir_and_is_created() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = &created.instance.id;
    let expected = AppPaths::from_root(h._temp.path()).game_logs_dir().join(id);
    assert!(!expected.exists());
    let dir = h.launcher.instance_folder(id, FolderKind::Logs).unwrap();
    assert_eq!(dir, expected);
    assert!(dir.is_dir());
}

#[tokio::test]
async fn instance_folder_of_missing_instance_is_not_found() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let err = h
        .launcher
        .instance_folder("missing", FolderKind::Game)
        .unwrap_err();
    assert_eq!(err.code(), "instance.not_found");
}

#[tokio::test]
async fn create_installs_and_play_during_install_reuses_the_task() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("テスト", "1.99-test"))
        .unwrap();
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
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
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
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
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
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
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
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = &created.instance.id;
    instance::set_state(h.launcher.db(), id, InstanceState::Broken).unwrap();
    insert_running(&h.launcher, id, 1);
    let err = h.launcher.launch_instance(id, false).unwrap_err();
    assert_eq!(err.code(), "launch.already_running");
    assert_eq!(kinds_for(&h.tasks, id), vec![INSTALL_KIND]);
}

#[tokio::test]
async fn delete_is_refused_while_running() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = &created.instance.id;
    insert_running(&h.launcher, id, 1);
    let err = h.launcher.delete_instance(id).await.unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    assert!(instance::get(h.launcher.db(), id).is_ok());
}

#[tokio::test]
async fn delete_is_refused_while_task_is_live_but_stale_installing_row_is_deletable() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
    let id = &created.instance.id;
    // インストールのタスクが動いている間
    let err = h.launcher.delete_instance(id).await.unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    wait(&h.tasks, &created.task_id).await;
    assert!(instance::get(h.launcher.db(), id).is_ok());
    // 途中で落ちて state が installing のまま、タスクもない行は削除できる
    instance::set_state(h.launcher.db(), id, InstanceState::Installing).unwrap();
    h.launcher.delete_instance(id).await.unwrap();
    assert!(h.launcher.list_instances().unwrap().is_empty());
}

#[tokio::test]
async fn update_instance_works_while_running() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = &created.instance.id;
    insert_running(&h.launcher, id, 1);
    let mut s = settings(MemoryMode::Manual, Some(2048), None, "-Dfoo=1");
    s.name = "renamed".into();
    let updated = h.launcher.update_instance(id, &s).await.unwrap();
    assert_eq!(updated.name, "renamed");
    assert_eq!(updated.memory_mb, Some(2048));
    assert_eq!(
        instance::get(h.launcher.db(), id).unwrap().jvm_args,
        "-Dfoo=1"
    );
    // 存在しない ID は見つからない
    let err = h.launcher.update_instance("nope", &s).await.unwrap_err();
    assert_eq!(err.code(), "instance.not_found");
}

#[tokio::test]
async fn delete_rechecks_after_taking_the_instance_lock() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = created.instance.id.clone();
    // 起動の処理がロックを持っている間に削除が待ち、その間にゲームが起動した状況
    let guard = h.launcher.instance_locks().lock(&id).await;
    let launcher = h.launcher.clone();
    let target = id.clone();
    let deleting = tokio::spawn(async move { launcher.delete_instance(&target).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!deleting.is_finished());
    insert_running(&h.launcher, &id, 1);
    drop(guard);
    let err = deleting.await.unwrap().unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    assert!(instance::get(h.launcher.db(), &id).is_ok());
}

#[tokio::test]
async fn delete_removes_a_ready_instance() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = &created.instance.id;
    h.launcher.delete_instance(id).await.unwrap();
    assert!(h.launcher.list_instances().unwrap().is_empty());
    // 共有のライブラリとアセットは消さない
    let paths = AppPaths::from_root(h._temp.path());
    assert!(paths.libraries_dir().is_dir());
}

/// instances/ の下にあるディレクトリの名前。
fn instance_dirs(h: &Harness) -> Vec<String> {
    let paths = AppPaths::from_root(h._temp.path());
    let mut names: Vec<String> = std::fs::read_dir(paths.instances_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn source_dir(h: &Harness, id: &str) -> std::path::PathBuf {
    let paths = AppPaths::from_root(h._temp.path());
    instance::game_dir(&paths, &instance::get(h.launcher.db(), id).unwrap())
}

/// 複製が失敗か取り消しで終わり、元の行とディレクトリだけが残っていること。
fn assert_nothing_left(h: &Harness, source: &instance::Instance, dirs_before: &[String]) {
    let listed = h.launcher.list_instances().unwrap();
    assert_eq!(listed, vec![source.clone()]);
    assert_eq!(instance_dirs(h), dirs_before);
}

#[tokio::test]
async fn duplicate_creates_independent_copy() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let id = h.installed().await;
    let src_dir = source_dir(&h, &id);
    std::fs::create_dir_all(src_dir.join("saves/world")).unwrap();
    std::fs::write(src_dir.join("saves/world/level.dat"), "world").unwrap();
    instance::record_launch(h.launcher.db(), &id, 50).unwrap();
    let before = instance::get(h.launcher.db(), &id).unwrap();

    let task_id = h.launcher.duplicate_instance(&id).unwrap();
    let snap = wait(&h.tasks, &task_id).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(snap.kind, DUPLICATE_KIND);
    assert_eq!(snap.instance_id.as_deref(), Some(id.as_str()));
    assert_eq!(snap.progress.current, snap.progress.total);

    let listed = h.launcher.list_instances().unwrap();
    assert_eq!(listed.len(), 2);
    let copy = listed.iter().find(|i| i.id != id).unwrap();
    assert_eq!(copy.name, before.name);
    assert_eq!(copy.state, InstanceState::Ready);
    assert_eq!(copy.last_played_at, None);
    let copy_dir = source_dir(&h, &copy.id);
    assert_ne!(copy_dir, src_dir);
    assert_eq!(
        std::fs::read_to_string(copy_dir.join("saves/world/level.dat")).unwrap(),
        "world"
    );
    // 複製の側を変えても元は変わらない
    std::fs::write(copy_dir.join("saves/world/level.dat"), "changed").unwrap();
    assert_eq!(
        std::fs::read_to_string(src_dir.join("saves/world/level.dat")).unwrap(),
        "world"
    );
    assert_eq!(instance::get(h.launcher.db(), &id).unwrap(), before);
}

#[tokio::test]
async fn duplicate_of_a_broken_instance_is_broken_after_the_copy() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let id = h.installed().await;
    instance::set_state(h.launcher.db(), &id, InstanceState::Broken).unwrap();

    let task_id = h.launcher.duplicate_instance(&id).unwrap();
    let snap = wait(&h.tasks, &task_id).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    let listed = h.launcher.list_instances().unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|i| i.state == InstanceState::Broken));
}

#[tokio::test]
async fn duplicate_failure_leaves_no_row_or_dir() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let id = h.installed().await;
    // 元のディレクトリがないとコピーは失敗する（コピー先を作った後で）
    std::fs::remove_dir_all(source_dir(&h, &id)).unwrap();
    let source = instance::get(h.launcher.db(), &id).unwrap();
    let dirs_before = instance_dirs(&h);

    let task_id = h.launcher.duplicate_instance(&id).unwrap();
    let snap = wait(&h.tasks, &task_id).await;
    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "io.failed");
    assert_nothing_left(&h, &source, &dirs_before);
}

/// コピーの途中で読めないファイルに当たる。Windows では共有なしで開いておく。
#[cfg(windows)]
#[tokio::test]
async fn duplicate_failure_in_the_middle_leaves_no_row_or_dir() {
    use std::os::windows::fs::OpenOptionsExt;
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let id = h.installed().await;
    let src_dir = source_dir(&h, &id);
    std::fs::write(src_dir.join("a.txt"), "a").unwrap();
    std::fs::create_dir_all(src_dir.join("saves")).unwrap();
    std::fs::write(src_dir.join("saves/locked.dat"), "x").unwrap();
    let _locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(src_dir.join("saves/locked.dat"))
        .unwrap();
    let source = instance::get(h.launcher.db(), &id).unwrap();
    let dirs_before = instance_dirs(&h);

    let task_id = h.launcher.duplicate_instance(&id).unwrap();
    let snap = wait(&h.tasks, &task_id).await;
    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "io.failed");
    assert_nothing_left(&h, &source, &dirs_before);
    assert_eq!(std::fs::read_to_string(src_dir.join("a.txt")).unwrap(), "a");
}

/// コピーの途中で読めないファイルに当たる。Unix では権限を外す（root では確かめられない）。
#[cfg(unix)]
#[tokio::test]
async fn duplicate_failure_in_the_middle_leaves_no_row_or_dir() {
    use std::os::unix::fs::PermissionsExt;
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let id = h.installed().await;
    let src_dir = source_dir(&h, &id);
    std::fs::write(src_dir.join("a.txt"), "a").unwrap();
    std::fs::create_dir_all(src_dir.join("saves")).unwrap();
    let locked = src_dir.join("saves/locked.dat");
    std::fs::write(&locked, "x").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&locked).is_ok() {
        return;
    }
    let source = instance::get(h.launcher.db(), &id).unwrap();
    let dirs_before = instance_dirs(&h);

    let task_id = h.launcher.duplicate_instance(&id).unwrap();
    let snap = wait(&h.tasks, &task_id).await;
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "io.failed");
    assert_nothing_left(&h, &source, &dirs_before);
}

#[tokio::test]
async fn duplicate_cancel_leaves_no_row_or_dir() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let id = h.installed().await;
    std::fs::write(source_dir(&h, &id).join("options.txt"), "a").unwrap();
    let source = instance::get(h.launcher.db(), &id).unwrap();
    let dirs_before = instance_dirs(&h);
    // ロックを待っている間に取り消す。コピーは始まった直後に取り消しに気づく
    let guard = h.launcher.instance_locks().lock(&id).await;
    let task_id = h.launcher.duplicate_instance(&id).unwrap();
    h.tasks.cancel(&task_id).unwrap();
    drop(guard);
    let snap = wait(&h.tasks, &task_id).await;
    assert_eq!(snap.state, TaskState::Cancelled);
    assert_nothing_left(&h, &source, &dirs_before);
}

#[tokio::test]
async fn duplicate_is_refused_while_running() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let id = h.installed().await;
    insert_running(&h.launcher, &id, 1);
    let err = h.launcher.duplicate_instance(&id).unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    assert_eq!(kinds_for(&h.tasks, &id), vec![INSTALL_KIND]);
    assert_eq!(h.launcher.list_instances().unwrap().len(), 1);
}

#[tokio::test]
async fn duplicate_rechecks_after_taking_the_instance_lock() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let id = h.installed().await;
    let source = instance::get(h.launcher.db(), &id).unwrap();
    let dirs_before = instance_dirs(&h);
    // 起動の処理がロックを持っている間に複製が待ち、その間にゲームが起動した状況
    let guard = h.launcher.instance_locks().lock(&id).await;
    let task_id = h.launcher.duplicate_instance(&id).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!h.tasks.get(&task_id).unwrap().state.is_terminal());
    insert_running(&h.launcher, &id, 1);
    drop(guard);
    let snap = wait(&h.tasks, &task_id).await;
    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "instance.busy");
    assert_nothing_left(&h, &source, &dirs_before);
}

#[tokio::test]
async fn duplicate_is_refused_while_installing_but_not_for_a_stale_row() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
    let id = &created.instance.id;
    // インストールのタスクが動いている間
    let err = h.launcher.duplicate_instance(id).unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    wait(&h.tasks, &created.task_id).await;
    // 途中で落ちて state が installing のまま、タスクもない行は複製できる
    instance::set_state(h.launcher.db(), id, InstanceState::Installing).unwrap();
    let task_id = h.launcher.duplicate_instance(id).unwrap();
    let snap = wait(&h.tasks, &task_id).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    let listed = h.launcher.list_instances().unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|i| i.state == InstanceState::Installing));
}

#[tokio::test]
async fn duplicate_of_missing_instance_is_not_found() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let err = h.launcher.duplicate_instance("nope").unwrap_err();
    assert_eq!(err.code(), "instance.not_found");
}

#[tokio::test]
async fn reattach_runs_only_once() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
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
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
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
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
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
    let created = h
        .launcher
        .create_instance(&NewInstance::vanilla("x", "1.99-test"))
        .unwrap();
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

// ---- インスタンスの設定の起動への反映（フェーズ 4） ----

/// 引数をそのまま出力する偽の Java を作る。Windows ではバッチファイルにする。
fn echo_java(dir: &std::path::Path) -> std::path::PathBuf {
    if cfg!(windows) {
        let path = dir.join("fake-java.bat");
        std::fs::write(
            &path,
            b"@echo fake game %*
",
        )
        .unwrap();
        path
    } else {
        let path = dir.join("fake-java");
        std::fs::write(&path, FAKE_JAVA).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }
}

fn settings(
    memory_mode: MemoryMode,
    memory_mb: Option<u32>,
    java_path: Option<String>,
    jvm_args: &str,
) -> InstanceSettings {
    InstanceSettings {
        name: "x".into(),
        memory_mode,
        memory_mb,
        java_mode: if java_path.is_some() {
            JavaMode::Manual
        } else {
            JavaMode::Auto
        },
        java_path,
        jvm_args: jvm_args.into(),
    }
}

/// 手動の Java（引数を出力するもの）で起動し、ゲームの出力を返す。
async fn launch_with(memory_mode: MemoryMode, memory_mb: Option<u32>, jvm_args: &str) -> String {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let id = h.installed().await;
    let java = echo_java(h._temp.path());
    instance::update_settings(
        h.launcher.db(),
        &id,
        &settings(
            memory_mode,
            memory_mb,
            Some(java.display().to_string()),
            jvm_args,
        ),
    )
    .unwrap();
    let task = h.launcher.launch_instance(&id, false).unwrap();
    let snap = wait(&h.tasks, &task).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    h.game.wait_exit(&id).await;
    h.game.messages().join(
        "
",
    )
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[tokio::test]
async fn manual_memory_is_used_for_xmx() {
    let messages = launch_with(MemoryMode::Manual, Some(4096), "").await;
    assert!(messages.contains("fake game"), "{messages}");
    assert!(messages.contains("-Xmx4096M"), "{messages}");
    assert_eq!(count(&messages, "-Xmx"), 1, "{messages}");
}

#[tokio::test]
async fn auto_memory_keeps_the_default() {
    let messages = launch_with(MemoryMode::Auto, None, "").await;
    assert!(messages.contains("-Xmx2048M"), "{messages}");
    assert_eq!(count(&messages, "-Xmx"), 1, "{messages}");
}

#[tokio::test]
async fn jvm_args_are_passed_and_their_xmx_is_ignored() {
    let messages = launch_with(
        MemoryMode::Manual,
        Some(3072),
        "-Xmx8G -XX:+UseG1GC -Dnexora.test=ok",
    )
    .await;
    assert!(messages.contains("-XX:+UseG1GC"), "{messages}");
    assert!(messages.contains("-Dnexora.test=ok"), "{messages}");
    assert!(messages.contains("-Xmx3072M"), "{messages}");
    assert!(!messages.contains("-Xmx8G"), "{messages}");
    assert_eq!(count(&messages, "-Xmx"), 1, "{messages}");
}

#[tokio::test]
async fn unclosed_quote_or_empty_quotes_in_jvm_args_still_launch() {
    for jvm_args in ["-Dnexora.test=\"unclosed", "\"\"", "\""] {
        let messages = launch_with(MemoryMode::Auto, None, jvm_args).await;
        assert!(messages.contains("fake game"), "{jvm_args:?}: {messages}");
        assert!(
            messages.contains("net.minecraft.client.main.Main"),
            "{jvm_args:?}: {messages}"
        );
        assert_eq!(count(&messages, "-Xmx"), 1, "{jvm_args:?}: {messages}");
    }
}

#[tokio::test]
async fn missing_manual_java_fails_before_downloading() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake).await;
    let id = h.installed().await;
    let missing = h
        ._temp
        .path()
        .join("no-such-java.exe")
        .display()
        .to_string();
    instance::update_settings(
        h.launcher.db(),
        &id,
        &settings(MemoryMode::Auto, None, Some(missing.clone()), ""),
    )
    .unwrap();
    // 起動で取り直すファイルを消しておく（失敗が取得より前なら、取得の要求は増えない）
    std::fs::remove_dir_all(AppPaths::from_root(h._temp.path()).libraries_dir()).unwrap();
    let before = fake.server.received_requests().await.unwrap().len();
    let task = h.launcher.launch_instance(&id, false).unwrap();
    let snap = wait(&h.tasks, &task).await;
    assert_eq!(snap.state, TaskState::Failed);
    let error = snap.error.unwrap();
    assert_eq!(error.code, "java.path_missing");
    assert_eq!(error.params.get("path"), Some(&ParamValue::Str(missing)));
    assert_eq!(fake.server.received_requests().await.unwrap().len(), before);
    assert!(h.game.started.lock().unwrap().is_empty());
}
