//! UI に渡すタスクの型。

use serde::Serialize;
use ts_rs::TS;

use crate::error::ErrorPayload;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TaskState {
    Queued,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskState::Succeeded | TaskState::Failed | TaskState::Cancelled
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, TS)]
#[ts(export)]
pub struct TaskProgress {
    pub current: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskSnapshot {
    pub id: String,
    pub kind: String,
    /// このタスクが対象とするインスタンス。インスタンスに属さないタスクは None。
    pub instance_id: Option<String>,
    pub state: TaskState,
    pub progress: TaskProgress,
    pub revision: u64,
    pub error: Option<ErrorPayload>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TaskChange {
    Progress,
    State,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskChangedEvent {
    pub task_id: String,
    pub revision: u64,
    pub change: TaskChange,
}
