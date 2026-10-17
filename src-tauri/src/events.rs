//! core のタスクの通知を Tauri のイベントとして送る。

use nexora_core::auth::login::{LoginNotifier, LoginProgressEvent, UrlOpener};
use nexora_core::launch::log4j::LogRecord;
use nexora_core::launch::process::{GameExitedEvent, GameLogEvent, GameNotifier, RunningSession};
use nexora_core::task::{TaskChange, TaskChangedEvent, TaskNotifier};
use tauri::{AppHandle, Emitter};
use tauri_plugin_opener::OpenerExt;

pub const TASK_CHANGED: &str = "task://changed";
pub const GAME_STARTED: &str = "game://started";
pub const GAME_EXITED: &str = "game://exited";
pub const GAME_LOG: &str = "game://log";
pub const ACCOUNTS_CHANGED: &str = "accounts://changed";
pub const LOGIN_PROGRESS: &str = "login://progress";

/// アカウントの一覧が変わったことを UI に知らせる（ペイロードなし）。
pub fn emit_accounts_changed(app: &AppHandle) {
    if let Err(err) = app.emit(ACCOUNTS_CHANGED, ()) {
        tracing::warn!(%err, "failed to emit accounts event");
    }
}

/// ログインの URL をシステムのブラウザで開く。
pub struct TauriUrlOpener {
    app: AppHandle,
}

impl TauriUrlOpener {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl UrlOpener for TauriUrlOpener {
    fn open(&self, url: &str) -> Result<(), String> {
        // URL はエラーの文に入ることがあるため、種類だけを返す
        self.app
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|_| "failed to open the browser".to_string())
    }
}

/// ログインの進捗を `login://progress` で送る。
pub struct TauriLoginNotifier {
    app: AppHandle,
}

impl TauriLoginNotifier {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl LoginNotifier for TauriLoginNotifier {
    fn progress(&self, event: &LoginProgressEvent) {
        if let Err(err) = self.app.emit(LOGIN_PROGRESS, event.clone()) {
            tracing::warn!(%err, "failed to emit login event");
        }
    }
}

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
