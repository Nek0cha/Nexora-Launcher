//! core のタスクの通知を Tauri のイベントとして送る。

use nexora_core::launch::log4j::LogRecord;
use nexora_core::launch::process::{GameExitedEvent, GameLogEvent, GameNotifier, RunningSession};
use nexora_core::task::{TaskChange, TaskChangedEvent, TaskNotifier};
use tauri::{AppHandle, Emitter};

pub const TASK_CHANGED: &str = "task://changed";
pub const GAME_STARTED: &str = "game://started";
pub const GAME_EXITED: &str = "game://exited";
pub const GAME_LOG: &str = "game://log";

pub struct TauriNotifier {
    app: AppHandle,
}

impl TauriNotifier {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl TaskNotifier for TauriNotifier {
    fn notify(&self, task_id: &str, revision: u64, change: TaskChange) {
        let event = TaskChangedEvent {
            task_id: task_id.to_string(),
            revision,
            change,
        };
        if let Err(err) = self.app.emit(TASK_CHANGED, event) {
            tracing::warn!(%err, "failed to emit task event");
        }
    }
}

pub struct TauriGameNotifier {
    app: AppHandle,
}

impl TauriGameNotifier {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn emit<T: serde::Serialize + Clone>(&self, event: &str, payload: T) {
        if let Err(err) = self.app.emit(event, payload) {
            tracing::warn!(%err, event, "failed to emit game event");
        }
    }
}

impl GameNotifier for TauriGameNotifier {
    fn started(&self, session: &RunningSession) {
        self.emit(GAME_STARTED, session.clone());
    }

    fn log(&self, instance_id: &str, record: &LogRecord) {
        self.emit(
            GAME_LOG,
            GameLogEvent {
                instance_id: instance_id.to_string(),
                record: record.clone(),
            },
        );
    }

    fn exited(&self, instance_id: &str, exit_code: Option<i32>) {
        tracing::info!(instance_id, ?exit_code, "game exited");
        self.emit(
            GAME_EXITED,
            GameExitedEvent {
                instance_id: instance_id.to_string(),
                exit_code,
            },
        );
    }
}
