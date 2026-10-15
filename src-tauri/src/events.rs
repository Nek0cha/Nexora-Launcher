//! core のタスクの通知を Tauri のイベントとして送る。

use nexora_core::task::{TaskChange, TaskChangedEvent, TaskNotifier};
use tauri::{AppHandle, Emitter};

pub const TASK_CHANGED: &str = "task://changed";

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
