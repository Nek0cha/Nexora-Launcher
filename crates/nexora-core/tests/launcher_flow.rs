mod common;

use std::sync::Arc;
use std::time::Duration;

use nexora_core::db::Db;
use nexora_core::install::Urls;
use nexora_core::instance::{self, InstanceState};
use nexora_core::launcher::{INSTALL_KIND, LAUNCH_KIND, Launcher};
use nexora_core::paths::AppPaths;
use nexora_core::task::{TaskChange, TaskManager, TaskNotifier, TaskSnapshot, TaskState};

use common::{FakeMojang, FixedProbe, RecordingGame, http};

struct Silent;
impl TaskNotifier for Silent {
    fn notify(&self, _: &str, _: u64, _: TaskChange) {}
}

struct Harness {
    _temp: tempfile::TempDir,
    launcher: Launcher,
    tasks: TaskManager,
    game: Arc<RecordingGame>,
}

fn harness(fake: &FakeMojang) -> Harness {
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
    let launcher = Launcher::with(
        paths,
        Db::open_in_memory().unwrap(),
        tasks.clone(),
        game.clone(),
        http(),
        urls,
        Arc::new(FixedProbe(17)),
    );
    Harness {
        _temp: temp,
        launcher,
        tasks,
        game,
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
    let h = harness(&fake);
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
    let h = harness(&fake);
    let created = h.launcher.create_instance("テスト", "1.99-test").unwrap();
    assert_eq!(created.instance.state, InstanceState::Installing);
    let again = h.launcher.launch_instance(&created.instance.id).unwrap();
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
    let h = harness(&fake);
    let created = h.launcher.create_instance("x", "1.99-test").unwrap();
    let id = &created.instance.id;
    // インストールの途中で何度押しても、同じインストールのタスクを返す
    for _ in 0..5 {
        assert_eq!(h.launcher.launch_instance(id).unwrap(), created.task_id);
    }
    assert_eq!(kinds_for(&h.tasks, id), vec![INSTALL_KIND]);
    wait(&h.tasks, &created.task_id).await;
    assert!(h.game.started.lock().unwrap().is_empty());
}

#[tokio::test]
async fn repeated_play_reuses_the_launch_task() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake);
    let created = h.launcher.create_instance("x", "1.99-test").unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = &created.instance.id;
    let first = h.launcher.launch_instance(id).unwrap();
    for _ in 0..5 {
        match h.launcher.launch_instance(id) {
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
    let h = harness(&fake);
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
        .launch_instance(&created.instance.id)
        .unwrap_err();
    assert_eq!(err.code(), "launch.already_running");
}

#[tokio::test]
async fn install_is_refused_while_running() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake);
    let created = h.launcher.create_instance("x", "1.99-test").unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = &created.instance.id;
    instance::set_state(h.launcher.db(), id, InstanceState::Broken).unwrap();
    insert_running(&h.launcher, id, 1);
    let err = h.launcher.launch_instance(id).unwrap_err();
    assert_eq!(err.code(), "launch.already_running");
    assert_eq!(kinds_for(&h.tasks, id), vec![INSTALL_KIND]);
}

#[tokio::test]
async fn reattach_runs_only_once() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake);
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
    let h = harness(&fake);
    let created = h.launcher.create_instance("x", "1.99-test").unwrap();
    wait(&h.tasks, &created.task_id).await;
    let id = &created.instance.id;
    let task = h.launcher.launch_instance(id).unwrap();
    let again = h.launcher.launch_instance(id);
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
    let h = harness(&fake);
    let created = h.launcher.create_instance("x", "1.99-test").unwrap();
    wait(&h.tasks, &created.task_id).await;
    drop(fake);
    let task = h.launcher.launch_instance(&created.instance.id).unwrap();
    let snap = wait(&h.tasks, &task).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
}

#[tokio::test]
async fn failed_spawn_does_not_update_last_played() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake);
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
    let task = h.launcher.launch_instance(id).unwrap();
    let snap = wait(&h.tasks, &task).await;
    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "launch.spawn_failed");
    let inst = instance::get(h.launcher.db(), id).unwrap();
    assert_eq!(inst.last_played_at, None);
    assert!(h.launcher.list_running().unwrap().is_empty());
}
