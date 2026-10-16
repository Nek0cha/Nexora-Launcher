mod common;

use std::sync::Arc;
use std::time::Duration;

use nexora_core::db::Db;
use nexora_core::instance;
use nexora_core::launch::process::{GameProcesses, SpawnSpec, process_start_ms};
use nexora_core::paths::AppPaths;
use secrecy::SecretString;

use common::{RecordingGame, shell};

struct Fixture {
    _temp: tempfile::TempDir,
    paths: AppPaths,
    db: Db,
    game: Arc<RecordingGame>,
    processes: GameProcesses,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    paths.ensure_dirs().unwrap();
    let db = Db::open_in_memory().unwrap();
    let game = Arc::new(RecordingGame::default());
    let processes = GameProcesses::new(db.clone(), paths.clone(), game.clone())
        .with_poll_interval(Duration::from_millis(50));
    Fixture {
        _temp: temp,
        paths,
        db,
        game,
        processes,
    }
}

fn spec(f: &Fixture, id: &str, script: &str, secret: Option<&str>) -> SpawnSpec {
    let (program, args) = shell(script);
    let inst = instance::get(&f.db, id).unwrap();
    SpawnSpec {
        instance_id: id.into(),
        program,
        args,
        cwd: instance::game_dir(&f.paths, &inst),
        log_path: f.paths.game_logs_dir().join(id).join("1.log"),
        account_id: "acc".into(),
        launched_at: nexora_core::install::now_ms(),
        secret: secret.map(|s| SecretString::from(s.to_string())),
    }
}

#[tokio::test]
async fn records_output_exit_and_play_time() {
    let f = fixture();
    let inst = instance::create(&f.db, &f.paths, "x", "1.0", 1).unwrap();
    let session = f
        .processes
        .spawn(spec(&f, &inst.id, "echo hello", None))
        .await
        .unwrap();
    assert_eq!(session.instance_id, inst.id);
    assert!(f.processes.is_running(&inst.id).unwrap() || !f.game.exits.lock().unwrap().is_empty());
    assert_eq!(f.game.wait_exit(&inst.id).await, Some(0));
    assert!(f.game.messages().iter().any(|m| m.trim() == "hello"));
    assert!(!f.processes.is_running(&inst.id).unwrap());
    assert!(f.processes.list().unwrap().is_empty());
    let log =
        std::fs::read_to_string(f.paths.game_logs_dir().join(&inst.id).join("1.log")).unwrap();
    assert!(log.contains("hello"));
    assert_eq!(f.game.started.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn argfile_is_removed_when_the_game_exits() {
    let f = fixture();
    let inst = instance::create(&f.db, &f.paths, "x", "1.0", 1).unwrap();
    let argfile = f.paths.argfile(&inst.id);
    std::fs::create_dir_all(argfile.parent().unwrap()).unwrap();
    std::fs::write(&argfile, b"\"-cp\"\n").unwrap();
    f.processes
        .spawn(spec(&f, &inst.id, "exit 0", None))
        .await
        .unwrap();
    f.game.wait_exit(&inst.id).await;
    assert!(!argfile.exists());
}

#[tokio::test]
async fn reports_nonzero_exit_codes() {
    let f = fixture();
    let inst = instance::create(&f.db, &f.paths, "x", "1.0", 1).unwrap();
    f.processes
        .spawn(spec(&f, &inst.id, "exit 3", None))
        .await
        .unwrap();
    assert_eq!(f.game.wait_exit(&inst.id).await, Some(3));
}

#[tokio::test]
async fn secrets_are_redacted_from_output() {
    let f = fixture();
    let inst = instance::create(&f.db, &f.paths, "x", "1.0", 1).unwrap();
    f.processes
        .spawn(spec(
            &f,
            &inst.id,
            "echo token=supersecret123",
            Some("supersecret123"),
        ))
        .await
        .unwrap();
    f.game.wait_exit(&inst.id).await;
    let all = f.game.messages().join("\n");
    assert!(all.contains("[REDACTED]"), "{all}");
    assert!(!all.contains("supersecret123"));
    let log =
        std::fs::read_to_string(f.paths.game_logs_dir().join(&inst.id).join("1.log")).unwrap();
    assert!(!log.contains("supersecret123"));
}

#[cfg(unix)]
#[tokio::test]
async fn log4j_events_and_invalid_utf8_are_handled() {
    let f = fixture();
    let inst = instance::create(&f.db, &f.paths, "x", "1.0", 1).unwrap();
    let script = r#"printf '<log4j:Event logger="a" timestamp="0" level="INFO" thread="main">\n<log4j:Message><![CDATA[Hi]]></log4j:Message>\n</log4j:Event>\n'; printf '\377abc\n'"#;
    f.processes
        .spawn(spec(&f, &inst.id, script, None))
        .await
        .unwrap();
    f.game.wait_exit(&inst.id).await;
    let logs = f.game.logs.lock().unwrap().clone();
    assert!(
        logs.iter()
            .any(|(_, r)| r.message == "Hi" && r.level.as_deref() == Some("INFO"))
    );
    assert!(logs.iter().any(|(_, r)| r.message.ends_with("abc")));
}

#[tokio::test]
async fn reattach_keeps_live_sessions() {
    let f = fixture();
    let inst = instance::create(&f.db, &f.paths, "x", "1.0", 1).unwrap();
    let pid = std::process::id();
    let started = process_start_ms(pid).unwrap();
    insert_session(&f.db, &inst.id, pid, started, 1);
    f.processes.reattach().unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(f.processes.is_running(&inst.id).unwrap());
    assert!(f.game.exits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn reattach_finishes_dead_sessions_using_latest_log_time() {
    let f = fixture();
    let inst = instance::create(&f.db, &f.paths, "x", "1.0", 1).unwrap();
    let logs = instance::game_dir(&f.paths, &inst).join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(logs.join("latest.log"), b"done").unwrap();
    insert_session(&f.db, &inst.id, u32::MAX - 1, 12_345, 1_000);
    let argfile = f.paths.argfile(&inst.id);
    std::fs::create_dir_all(argfile.parent().unwrap()).unwrap();
    std::fs::write(&argfile, b"\"-cp\"\n").unwrap();
    f.processes.reattach().unwrap();
    assert!(!f.processes.is_running(&inst.id).unwrap());
    assert!(!argfile.exists());
    assert!(instance::get(&f.db, &inst.id).unwrap().play_time_ms > 0);
    assert_eq!(
        f.game.exits.lock().unwrap().as_slice(),
        &[(inst.id.clone(), None)]
    );
}

#[tokio::test]
async fn reused_pid_is_not_mistaken_for_the_game() {
    let f = fixture();
    let inst = instance::create(&f.db, &f.paths, "x", "1.0", 1).unwrap();
    let pid = std::process::id();
    let wrong_start = process_start_ms(pid).unwrap() - 60_000;
    insert_session(&f.db, &inst.id, pid, wrong_start, 1_000);
    f.processes.reattach().unwrap();
    assert!(!f.processes.is_running(&inst.id).unwrap());
    // latest.log がないため、プレイ時間は加算しない
    assert_eq!(instance::get(&f.db, &inst.id).unwrap().play_time_ms, 0);
}

fn insert_session(db: &Db, id: &str, pid: u32, started: i64, launched: i64) {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO running_sessions (instance_id, pid, process_started_at, launched_at, account_id, log_path)
             VALUES (?1, ?2, ?3, ?4, 'acc', 'x.log')",
            rusqlite::params![id, pid, started, launched],
        )?;
        Ok(())
    })
    .unwrap();
}
