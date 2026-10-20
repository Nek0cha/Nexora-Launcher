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

/// タスクがいま何をしているか。段階が変わると進捗は 0/0 から数え直す。
/// 進捗の単位は段階ごとに違う（Verifying は確かめたファイルの数、Downloading は KiB、
/// BuildingLoader は済んだ processor の数）。ほかの段階は進み具合を持たない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TaskPhase {
    /// メタデータやインストーラを用意している。
    Resolving,
    /// 手元のファイルを確かめている。
    Verifying,
    /// 足りないファイルを取得している。
    Downloading,
    /// Forge や NeoForge の processor を実行している。
    BuildingLoader,
    /// 完了の記録や Java の登録をしている。
    Finishing,
    /// ゲームを起動している。
    Launching,
}

impl TaskPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskPhase::Resolving => "resolving",
            TaskPhase::Verifying => "verifying",
            TaskPhase::Downloading => "downloading",
            TaskPhase::BuildingLoader => "building_loader",
            TaskPhase::Finishing => "finishing",
            TaskPhase::Launching => "launching",
        }
    }
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
    /// いまの段階。段階を持たないタスクと、終了したタスクは None。
    pub phase: Option<TaskPhase>,
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
    /// 段階が変わった（進捗も 0/0 に戻っている）。
    Phase,
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
