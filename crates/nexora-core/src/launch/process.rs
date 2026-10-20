//! ゲームのプロセスの起動、監視、ランチャーの再起動後の引き継ぎ。

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{OptionalExtension, params};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use ts_rs::TS;

use super::log4j::{Log4jParser, LogRecord, redact};
use crate::db::Db;
use crate::error::CoreError;
use crate::install::now_ms;
use crate::instance;
use crate::net::io_err;
use crate::paths::AppPaths;

/// Windows でコンソールの窓を出さないためのフラグ。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// ゲームの状態の変化を受け取る側（Tauri のイベントなど）。
pub trait GameNotifier: Send + Sync {
    fn started(&self, session: &RunningSession);
    fn log(&self, instance_id: &str, record: &LogRecord);
    fn exited(&self, instance_id: &str, exit_code: Option<i32>);
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunningSession {
    pub instance_id: String,
    pub pid: u32,
    pub launched_at: i64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameExitedEvent {
    pub instance_id: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameLogEvent {
    pub instance_id: String,
    pub record: LogRecord,
}

pub struct SpawnSpec {
    pub instance_id: String,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub log_path: PathBuf,
    pub account_id: String,
    pub launched_at: i64,
    /// 出力から伏せる値（access token）。
    pub secret: Option<SecretString>,
}

/// プロセスの開始時刻（UNIX ミリ秒、秒単位の精度）。プロセスがなければ None。
pub fn process_start_ms(pid: u32) -> Option<i64> {
    let mut sys = System::new();
    let pid = Pid::from_u32(pid);
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    sys.process(pid).map(|p| p.start_time() as i64 * 1000)
}

/// PID が同じでも開始時刻が違えば、別のプロセスに再利用されたとみなす。
fn is_alive(pid: u32, started_at_ms: i64) -> bool {
    process_start_ms(pid) == Some(started_at_ms)
}

/// ランチャーが閉じている間に終了したゲームのプレイ時間。latest.log の更新時刻を終了時刻とみなす。
pub fn closed_play_time(launched_at: i64, latest_log_mtime: Option<i64>) -> i64 {
    match latest_log_mtime {
        Some(end) if end > launched_at => end - launched_at,
        _ => 0,
    }
}

#[derive(Clone)]
pub struct GameProcesses {
    db: Db,
    paths: AppPaths,
    notifier: Arc<dyn GameNotifier>,
    poll: Duration,
}

struct Row {
    instance_id: String,
    pid: u32,
    started_at: i64,
    launched_at: i64,
}

impl GameProcesses {
    pub fn new(db: Db, paths: AppPaths, notifier: Arc<dyn GameNotifier>) -> Self {
        Self {
            db,
            paths,
            notifier,
            poll: Duration::from_secs(5),
        }
    }

    /// 引き継いだプロセスの生存を確かめる間隔。
    pub fn with_poll_interval(mut self, poll: Duration) -> Self {
        self.poll = poll;
        self
    }

    fn rows(&self) -> Result<Vec<Row>, CoreError> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT instance_id, pid, process_started_at, launched_at
                 FROM running_sessions ORDER BY launched_at",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(Row {
                        instance_id: r.get(0)?,
                        pid: r.get(1)?,
                        started_at: r.get(2)?,
                        launched_at: r.get(3)?,
                    })
                })?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        })
    }

    pub fn list(&self) -> Result<Vec<RunningSession>, CoreError> {
        Ok(self
            .rows()?
            .into_iter()
            .map(|r| RunningSession {
                instance_id: r.instance_id,
                pid: r.pid,
                launched_at: r.launched_at,
            })
            .collect())
    }

    pub fn is_running(&self, instance_id: &str) -> Result<bool, CoreError> {
        self.db.with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT count(*) FROM running_sessions WHERE instance_id = ?1",
                [instance_id],
                |r| r.get::<_, i64>(0),
            )? > 0)
        })
    }

    /// account_id（ハイフンなしの UUID）で実行中のゲームがあるか。
    pub fn is_account_running(&self, account_id: &str) -> Result<bool, CoreError> {
        self.db.with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT count(*) FROM running_sessions WHERE account_id = ?1",
                [account_id],
                |r| r.get::<_, i64>(0),
            )? > 0)
        })
    }

    /// 記録されたゲームを止める。`force` なら強制終了し、そうでなければ閉じるよう頼む。終了は待たない。
    /// 記録が無い、または pid が別のプロセスに再利用されているときは、何も送らずに成功とする。
    /// 終了後の記録（プレイ時間など）は、既存の終了の検知に任せる。
    pub fn stop(&self, instance_id: &str, force: bool) -> Result<(), CoreError> {
        let row = self.db.with_conn(|conn| {
            Ok(conn
                .query_row(
                    "SELECT pid, process_started_at FROM running_sessions WHERE instance_id = ?1",
                    [instance_id],
                    |r| Ok((r.get::<_, u32>(0)?, r.get::<_, i64>(1)?)),
                )
                .optional()?)
        })?;
        let Some((pid, started_at)) = row else {
            tracing::info!(
                instance_id,
                force,
                found = false,
                "stop: no running session"
            );
            return Ok(());
        };
        if force {
            // 開始時刻の確認と強制終了に、同じプロセスの情報を使う
            let mut sys = System::new();
            let sys_pid = Pid::from_u32(pid);
            sys.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[sys_pid]),
                true,
                ProcessRefreshKind::nothing(),
            );
            let matched = sys
                .process(sys_pid)
                .is_some_and(|p| p.start_time() as i64 * 1000 == started_at);
            tracing::info!(
                instance_id,
                pid,
                force,
                found = true,
                matched,
                "stop: force"
            );
            if matched
                && let Some(process) = sys.process(sys_pid)
                && !process.kill()
            {
                tracing::warn!(instance_id, pid, "failed to kill the game");
            }
        } else {
            let matched = is_alive(pid, started_at);
            tracing::info!(
                instance_id,
                pid,
                force,
                found = true,
                matched,
                "stop: close request"
            );
            if matched {
                let windows = super::stop::request_close(pid);
                tracing::info!(instance_id, pid, windows, "stop: close request sent");
            } else {
                tracing::warn!(
                    instance_id,
                    pid,
                    "stop: the process is gone or its pid was reused; nothing sent"
                );
            }
        }
        Ok(())
    }

    /// 起動して記録する。出力の読み取りと終了の処理は、裏で続ける。
    pub async fn spawn(&self, spec: SpawnSpec) -> Result<RunningSession, CoreError> {
        if let Some(parent) = spec.log_path.parent() {
            std::fs::create_dir_all(parent).map_err(|s| io_err(parent, s))?;
        }
        let mut cmd = tokio::process::Command::new(&spec.program);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let mut child = cmd.spawn().map_err(|e| CoreError::SpawnFailed {
            message: e.to_string(),
        })?;
        let Some(pid) = child.id() else {
            return Err(CoreError::SpawnFailed {
                message: "the process exited before its id was read".into(),
            });
        };
        // すぐに終了した場合は開始時刻を取れないため、今の時刻で代える（再起動後には一致せず、終了済みとして扱われる）
        let started_at = process_start_ms(pid).unwrap_or_else(now_ms);
        let session = RunningSession {
            instance_id: spec.instance_id.clone(),
            pid,
            launched_at: spec.launched_at,
        };
        let recorded = self.db.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO running_sessions
                 (instance_id, pid, process_started_at, launched_at, account_id, log_path)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    spec.instance_id,
                    pid,
                    started_at,
                    spec.launched_at,
                    spec.account_id,
                    spec.log_path.display().to_string()
                ],
            )?;
            Ok(())
        });
        if let Err(err) = recorded {
            // 記録できないゲームは監視も引き継ぎもできないため、止める
            let _ = child.start_kill();
            return Err(err);
        }
        self.notifier.started(&session);

        let secret = spec.secret.map(Arc::new);
        let (tx, rx) = mpsc::unbounded_channel();
        if let Some(out) = child.stdout.take() {
            tokio::spawn(read_stream(out, tx.clone(), secret.clone()));
        }
        if let Some(err) = child.stderr.take() {
            tokio::spawn(read_stream(err, tx.clone(), secret));
        }
        drop(tx);
        let this = self.clone();
        let instance_id = spec.instance_id;
        let launched_at = spec.launched_at;
        let log_path = spec.log_path;
        tokio::spawn(async move {
            let writer = this.write_logs(&instance_id, &log_path, rx);
            let (status, ()) = tokio::join!(child.wait(), writer);
            let code = status.ok().and_then(|s| s.code());
            this.finish(&instance_id, pid, now_ms() - launched_at, code);
        });
        Ok(session)
    }

    async fn write_logs(
        &self,
        instance_id: &str,
        log_path: &Path,
        mut rx: mpsc::UnboundedReceiver<LogRecord>,
    ) {
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .await
            .map_err(|err| tracing::warn!(%err, "failed to open game log"))
            .ok();
        while let Some(record) = rx.recv().await {
            if let Some(f) = file.as_mut() {
                let line = format!("{}\n", record.to_file_line());
                if f.write_all(line.as_bytes()).await.is_err() {
                    file = None;
                }
            }
            self.notifier.log(instance_id, &record);
        }
        if let Some(mut f) = file {
            let _ = f.flush().await;
        }
    }

    /// プレイ時間を足し、記録と argfile を消して終了を知らせる。pid が違う行（新しい起動）は消さない。
    fn finish(&self, instance_id: &str, pid: u32, play_ms: i64, exit_code: Option<i32>) {
        let argfile = self.paths.argfile(instance_id);
        match std::fs::remove_file(&argfile) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => tracing::warn!(instance_id, %err, "failed to remove the argfile"),
        }
        if let Err(err) = instance::add_play_time(&self.db, instance_id, play_ms) {
            tracing::warn!(instance_id, code = err.code(), "failed to add play time");
        }
        if let Err(err) = self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM running_sessions WHERE instance_id = ?1 AND pid = ?2",
                params![instance_id, pid],
            )?;
            Ok(())
        }) {
            tracing::warn!(
                instance_id,
                code = err.code(),
                "failed to clear running session"
            );
        }
        self.notifier.exited(instance_id, exit_code);
    }

    fn latest_log_mtime(&self, instance_id: &str) -> Option<i64> {
        let inst = instance::get(&self.db, instance_id).ok()?;
        let path = instance::game_dir(&self.paths, &inst)
            .join("logs")
            .join("latest.log");
        let modified = std::fs::metadata(path).ok()?.modified().ok()?;
        let ms = modified
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis();
        i64::try_from(ms).ok()
    }

    /// ランチャーの起動時に呼ぶ（tokio の実行環境の中で）。
    /// 動いているゲームは監視を再開し、終わっていたものは記録を片付ける。
    pub fn reattach(&self) -> Result<(), CoreError> {
        for row in self.rows()? {
            if is_alive(row.pid, row.started_at) {
                let this = self.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(this.poll).await;
                        if !is_alive(row.pid, row.started_at) {
                            this.finish(
                                &row.instance_id,
                                row.pid,
                                now_ms() - row.launched_at,
                                None,
                            );
                            break;
                        }
                    }
                });
            } else {
                let play =
                    closed_play_time(row.launched_at, self.latest_log_mtime(&row.instance_id));
                self.finish(&row.instance_id, row.pid, play, None);
            }
        }
        Ok(())
    }
}

async fn read_stream<R: AsyncRead + Unpin>(
    stream: R,
    tx: mpsc::UnboundedSender<LogRecord>,
    secret: Option<Arc<SecretString>>,
) {
    let mut reader = BufReader::new(stream);
    let mut parser = Log4jParser::new();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                // UTF-8 でないバイトは置換文字にする
                let text = String::from_utf8_lossy(&buf);
                let line = redact(
                    text.trim_end_matches(['\r', '\n']),
                    secret.as_deref().map(|s| s.expose_secret()),
                );
                for record in parser.push_line(&line) {
                    let _ = tx.send(record);
                }
            }
        }
    }
    for record in parser.finish() {
        let _ = tx.send(record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_play_time_uses_latest_log_only_when_newer() {
        assert_eq!(closed_play_time(1_000, Some(61_000)), 60_000);
        assert_eq!(closed_play_time(1_000, Some(500)), 0);
        assert_eq!(closed_play_time(1_000, None), 0);
    }

    #[test]
    fn own_process_has_a_start_time() {
        assert!(process_start_ms(std::process::id()).is_some());
        assert!(process_start_ms(u32::MAX - 1).is_none());
    }
}
