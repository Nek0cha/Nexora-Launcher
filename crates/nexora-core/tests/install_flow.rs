mod common;

use std::sync::Arc;
use std::time::Duration;

use nexora_core::db::Db;
use nexora_core::install::{InstallEnv, Urls, install};
use nexora_core::instance::{self, InstanceState, NewInstance};
use nexora_core::java::find_registered;
use nexora_core::paths::AppPaths;
use nexora_core::task::{TaskChange, TaskManager, TaskNotifier, TaskSnapshot, TaskState};

use common::{CuttingServer, FakeMojang, FixedProbe, LIBRARY, http, part_files};

struct Silent;
impl TaskNotifier for Silent {
    fn notify(&self, _: &str, _: u64, _: TaskChange) {}
}

struct Harness {
    _temp: tempfile::TempDir,
    env: InstallEnv,
    tasks: TaskManager,
}

fn harness(fake: &FakeMojang) -> Harness {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    paths.ensure_dirs().unwrap();
    let urls = Urls {
        manifest: fake.manifest_url(),
        runtime_index: fake.runtime_index_url(),
        resources: fake.resources_url(),
        // この試験はバニラだけを扱う。万一ローダーのメタデータを取りに行っても、
        // 本物のサーバーには出ず、手元のつながらないアドレスで失敗させる
        fabric_meta: "http://127.0.0.1:1".into(),
        quilt_meta: "http://127.0.0.1:1".into(),
    };
    let env = InstallEnv::new(
        paths,
        Db::open_in_memory().unwrap(),
        http(),
        urls,
        Arc::new(FixedProbe(17)),
    );
    Harness {
        _temp: temp,
        env,
        tasks: TaskManager::new(Arc::new(Silent)),
    }
}

async fn run_install(h: &Harness, id: &str) -> TaskSnapshot {
    let env = h.env.clone();
    let iid = id.to_string();
    let task = h
        .tasks
        .spawn("install", Some(id.into()), move |handle| async move {
            install(&env, &iid, &handle).await
        });
    wait(&h.tasks, &task).await
}

pub async fn wait(tasks: &TaskManager, id: &str) -> TaskSnapshot {
    for _ in 0..1500 {
        let snap = tasks.get(id).unwrap();
        if snap.state.is_terminal() {
            return snap;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("task {id} did not finish");
}

#[tokio::test]
async fn installs_everything_and_marks_ready() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake);
    let inst = instance::create(
        &h.env.db,
        &h.env.paths,
        &NewInstance::vanilla("test", "1.99-test"),
        1,
    )
    .unwrap();
    let snap = run_install(&h, &inst.id).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert!(snap.progress.total > 0);
    assert_eq!(snap.progress.current, snap.progress.total);
    assert_eq!(
        instance::get(&h.env.db, &inst.id).unwrap().state,
        InstanceState::Ready
    );
    let shared = h.env.paths.shared_dir();
    assert!(shared.join("versions/1.99-test/1.99-test.jar").is_file());
    assert!(shared.join("versions/1.99-test/1.99-test.json").is_file());
    assert!(
        shared
            .join("libraries/com/example/lib/1.0/lib-1.0.jar")
            .is_file()
    );
    assert!(find_registered(&h.env.db, "jre-test").unwrap().is_some());
}

#[tokio::test]
async fn unknown_version_marks_broken() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake);
    let inst = instance::create(
        &h.env.db,
        &h.env.paths,
        &NewInstance::vanilla("x", "9.9.9"),
        1,
    )
    .unwrap();
    let snap = run_install(&h, &inst.id).await;
    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "meta.version_not_found");
    assert_eq!(
        instance::get(&h.env.db, &inst.id).unwrap().state,
        InstanceState::Broken
    );
}

#[tokio::test]
async fn failed_install_resumes_without_refetching_finished_files() {
    let fake = FakeMojang::start_with(false).await;
    let h = harness(&fake);
    let inst = instance::create(
        &h.env.db,
        &h.env.paths,
        &NewInstance::vanilla("x", "1.99-test"),
        1,
    )
    .unwrap();
    let first = run_install(&h, &inst.id).await;
    assert_eq!(first.state, TaskState::Failed);
    assert_eq!(first.error.unwrap().code, "net.http_status");
    assert_eq!(
        instance::get(&h.env.db, &inst.id).unwrap().state,
        InstanceState::Broken
    );

    fake.serve_library().await;
    let second = run_install(&h, &inst.id).await;
    assert_eq!(second.state, TaskState::Succeeded, "{:?}", second.error);
    assert_eq!(fake.hits("/client.jar").await, 1);
    assert_eq!(
        instance::get(&h.env.db, &inst.id).unwrap().state,
        InstanceState::Ready
    );
}

#[tokio::test]
async fn cancelled_install_stays_installing() {
    let fake = FakeMojang::start().await;
    let h = harness(&fake);
    let inst = instance::create(
        &h.env.db,
        &h.env.paths,
        &NewInstance::vanilla("x", "1.99-test"),
        1,
    )
    .unwrap();
    let env = h.env.clone();
    let iid = inst.id.clone();
    let task = h
        .tasks
        .spawn("install", Some(inst.id.clone()), move |handle| async move {
            install(&env, &iid, &handle).await
        });
    h.tasks.cancel(&task).unwrap();
    let snap = wait(&h.tasks, &task).await;
    assert_eq!(snap.state, TaskState::Cancelled);
    assert_eq!(
        instance::get(&h.env.db, &inst.id).unwrap().state,
        InstanceState::Installing
    );
}

#[tokio::test]
async fn connection_lost_mid_download_marks_broken_and_resumes() {
    // 最初の試行と 3 回の再試行のすべてで、ライブラリの本文の途中で接続が切れる
    let attempts = 1 + nexora_core::net::MAX_RETRIES as usize;
    let cutting = CuttingServer::start(LIBRARY, attempts).await;
    let fake = FakeMojang::with_library_at(&cutting.url).await;
    let h = harness(&fake);
    let inst = instance::create(
        &h.env.db,
        &h.env.paths,
        &NewInstance::vanilla("x", "1.99-test"),
        1,
    )
    .unwrap();
    let first = run_install(&h, &inst.id).await;
    assert_eq!(first.state, TaskState::Failed);
    assert_eq!(first.error.unwrap().code, "net.offline");
    assert_eq!(
        instance::get(&h.env.db, &inst.id).unwrap().state,
        InstanceState::Broken
    );
    assert_eq!(cutting.hits(), attempts);
    let lib = h
        .env
        .paths
        .shared_dir()
        .join("libraries/com/example/lib/1.0/lib-1.0.jar");
    assert!(!lib.exists());
    assert!(part_files(h.env.paths.shared_dir().as_path()).is_empty());

    // 接続が戻ったら、もう一度プレイを押すと続きから取得して完了する
    let second = run_install(&h, &inst.id).await;
    assert_eq!(second.state, TaskState::Succeeded, "{:?}", second.error);
    assert_eq!(fake.hits("/client.jar").await, 1);
    assert_eq!(cutting.hits(), attempts + 1);
    assert!(lib.is_file());
    assert!(part_files(h.env.paths.shared_dir().as_path()).is_empty());
    assert_eq!(
        instance::get(&h.env.db, &inst.id).unwrap().state,
        InstanceState::Ready
    );
}
