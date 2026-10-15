//! 長い処理をタスクとして動かし、状態の変化を通知する。

pub mod types;

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::error::CoreError;
pub use types::{TaskChange, TaskChangedEvent, TaskProgress, TaskSnapshot, TaskState};

pub const MAX_FINISHED_TASKS: usize = 50;
pub const PROGRESS_NOTIFY_INTERVAL: Duration = Duration::from_millis(100);

/// タスクの状態の変化を外へ伝える口。Tauri 側が実装する。
pub trait TaskNotifier: Send + Sync {
    fn notify(&self, task_id: &str, revision: u64, change: TaskChange);
}

struct Entry {
    snapshot: TaskSnapshot,
    token: CancellationToken,
    last_progress_notify: Option<Instant>,
}

#[derive(Clone)]
pub struct TaskManager {
    tasks: Arc<Mutex<HashMap<String, Entry>>>,
    notifier: Arc<dyn TaskNotifier>,
}

/// タスクの処理に渡すハンドル。
pub struct TaskHandle {
    id: String,
    token: CancellationToken,
    manager: TaskManager,
}

impl TaskHandle {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    pub fn set_progress(&self, current: u32, total: u32) {
        self.manager.set_progress(&self.id, current, total);
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl TaskManager {
    pub fn new(notifier: Arc<dyn TaskNotifier>) -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            notifier,
        }
    }

    /// タスクを作って、すぐに ID を返す。処理は tokio のタスクとして動く。
    pub fn spawn<F, Fut>(&self, kind: &str, work: F) -> String
    where
        F: FnOnce(TaskHandle) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), CoreError>> + Send + 'static,
    {
        let (id, token) = {
            let mut tasks = self.tasks.lock().unwrap();
            self.insert(&mut tasks, kind)
        };
        self.notifier.notify(&id, 1, TaskChange::State);
        self.run(id.clone(), token, work);
        id
    }

    /// 同じ種類の終了していないタスクがあれば、その ID を返す。なければ新しく作る。
    pub fn spawn_unique<F, Fut>(&self, kind: &str, work: F) -> String
    where
        F: FnOnce(TaskHandle) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), CoreError>> + Send + 'static,
    {
        let (id, token) = {
            let mut tasks = self.tasks.lock().unwrap();
            let existing = tasks
                .values()
                .find(|e| e.snapshot.kind == kind && !e.snapshot.state.is_terminal())
                .map(|e| e.snapshot.id.clone());
            if let Some(existing) = existing {
                return existing;
            }
            self.insert(&mut tasks, kind)
        };
        self.notifier.notify(&id, 1, TaskChange::State);
        self.run(id.clone(), token, work);
        id
    }

    fn insert(
        &self,
        tasks: &mut HashMap<String, Entry>,
        kind: &str,
    ) -> (String, CancellationToken) {
        let id = uuid::Uuid::now_v7().to_string();
        let token = CancellationToken::new();
        let now = now_ms();
        tasks.insert(
            id.clone(),
            Entry {
                snapshot: TaskSnapshot {
                    id: id.clone(),
                    kind: kind.to_string(),
                    state: TaskState::Queued,
                    progress: TaskProgress::default(),
                    revision: 1,
                    error: None,
                    created_at: now,
                    updated_at: now,
                },
                token: token.clone(),
                last_progress_notify: None,
            },
        );
        (id, token)
    }

    fn run<F, Fut>(&self, id: String, token: CancellationToken, work: F)
    where
        F: FnOnce(TaskHandle) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), CoreError>> + Send + 'static,
    {
        let manager = self.clone();
        tokio::spawn(async move {
            manager.transition(&id, TaskState::Running, None);
            let handle = TaskHandle {
                id: id.clone(),
                token,
                manager: manager.clone(),
            };
            let (state, error) = match work(handle).await {
                Ok(()) => (TaskState::Succeeded, None),
                Err(CoreError::Cancelled) => (TaskState::Cancelled, None),
                Err(err) => {
                    tracing::warn!(task_id = %id, code = err.code(), "task failed");
                    (TaskState::Failed, Some(err.to_payload()))
                }
            };
            manager.transition(&id, state, error);
            manager.prune();
        });
    }

    /// 状態を変えて通知する。終了済みのタスクは変更しない。
    fn transition(&self, id: &str, state: TaskState, error: Option<crate::error::ErrorPayload>) {
        let revision = {
            let mut tasks = self.tasks.lock().unwrap();
            let Some(entry) = tasks.get_mut(id) else {
                return;
            };
            if entry.snapshot.state.is_terminal() {
                return;
            }
            // キャンセル中のタスクが走り出しても、キャンセル中のまま表示する
            if state == TaskState::Running && entry.snapshot.state == TaskState::Cancelling {
                return;
            }
            entry.snapshot.state = state;
            entry.snapshot.error = error;
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now_ms();
            entry.snapshot.revision
        };
        self.notifier.notify(id, revision, TaskChange::State);
    }

    fn set_progress(&self, id: &str, current: u32, total: u32) {
        let notify_revision = {
            let mut tasks = self.tasks.lock().unwrap();
            let Some(entry) = tasks.get_mut(id) else {
                return;
            };
            if entry.snapshot.state.is_terminal() {
                return;
            }
            entry.snapshot.progress = TaskProgress { current, total };
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now_ms();
            let now = Instant::now();
            let due = entry
                .last_progress_notify
                .is_none_or(|last| now.duration_since(last) >= PROGRESS_NOTIFY_INTERVAL);
            if due {
                entry.last_progress_notify = Some(now);
                Some(entry.snapshot.revision)
            } else {
                None
            }
        };
        if let Some(revision) = notify_revision {
            self.notifier.notify(id, revision, TaskChange::Progress);
        }
    }

    /// 終了したタスクを新しい順に MAX_FINISHED_TASKS 件だけ残す。
    fn prune(&self) {
        let removed: Vec<(String, u64)> = {
            let mut tasks = self.tasks.lock().unwrap();
            let mut finished: Vec<(String, i64, u64)> = tasks
                .values()
                .filter(|e| e.snapshot.state.is_terminal())
                .map(|e| {
                    (
                        e.snapshot.id.clone(),
                        e.snapshot.updated_at,
                        e.snapshot.revision,
                    )
                })
                .collect();
            if finished.len() <= MAX_FINISHED_TASKS {
                return;
            }
            // 新しい順。updated_at が同じ場合は UUID v7（作成順）で決める
            finished.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
            finished
                .split_off(MAX_FINISHED_TASKS)
                .into_iter()
                .map(|(id, _, revision)| {
                    tasks.remove(&id);
                    (id, revision + 1)
                })
                .collect()
        };
        for (id, revision) in removed {
            self.notifier.notify(&id, revision, TaskChange::Removed);
        }
    }

    /// すべてのタスクを、作成の新しい順に返す。
    pub fn list(&self) -> Vec<TaskSnapshot> {
        let tasks = self.tasks.lock().unwrap();
        let mut list: Vec<TaskSnapshot> = tasks.values().map(|e| e.snapshot.clone()).collect();
        list.sort_by(|a, b| b.id.cmp(&a.id));
        list
    }

    pub fn get(&self, id: &str) -> Result<TaskSnapshot, CoreError> {
        let tasks = self.tasks.lock().unwrap();
        tasks
            .get(id)
            .map(|e| e.snapshot.clone())
            .ok_or_else(|| CoreError::TaskNotFound {
                task_id: id.to_string(),
            })
    }

    /// キャンセルを要求する。終了済みのタスクには何もしない。
    pub fn cancel(&self, id: &str) -> Result<(), CoreError> {
        let revision = {
            let mut tasks = self.tasks.lock().unwrap();
            let entry = tasks.get_mut(id).ok_or_else(|| CoreError::TaskNotFound {
                task_id: id.to_string(),
            })?;
            if entry.snapshot.state.is_terminal() || entry.snapshot.state == TaskState::Cancelling {
                return Ok(());
            }
            entry.token.cancel();
            entry.snapshot.state = TaskState::Cancelling;
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now_ms();
            entry.snapshot.revision
        };
        self.notifier.notify(id, revision, TaskChange::State);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct Recorder(StdMutex<Vec<(String, u64, TaskChange)>>);

    impl TaskNotifier for Recorder {
        fn notify(&self, task_id: &str, revision: u64, change: TaskChange) {
            self.0
                .lock()
                .unwrap()
                .push((task_id.to_string(), revision, change));
        }
    }

    impl Recorder {
        fn changes_for(&self, id: &str) -> Vec<(u64, TaskChange)> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .filter(|(t, _, _)| t == id)
                .map(|(_, r, c)| (*r, *c))
                .collect()
        }
    }

    fn manager() -> (TaskManager, Arc<Recorder>) {
        let recorder = Arc::new(Recorder::default());
        (TaskManager::new(recorder.clone()), recorder)
    }

    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn successful_task_reaches_succeeded() {
        let (tasks, recorder) = manager();
        let id = tasks.spawn("test", |handle| async move {
            handle.set_progress(1, 2);
            Ok(())
        });
        settle().await;
        let snap = tasks.get(&id).unwrap();
        assert_eq!(snap.state, TaskState::Succeeded);
        assert_eq!(
            snap.progress,
            TaskProgress {
                current: 1,
                total: 2
            }
        );
        let revisions: Vec<u64> = recorder.changes_for(&id).iter().map(|(r, _)| *r).collect();
        assert!(
            revisions.windows(2).all(|w| w[0] < w[1]),
            "revisions must increase: {revisions:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn failing_task_keeps_error_payload() {
        let (tasks, _) = manager();
        let id = tasks.spawn("test", |_| async {
            Err(CoreError::DemoFailed { percent: 3 })
        });
        settle().await;
        let snap = tasks.get(&id).unwrap();
        assert_eq!(snap.state, TaskState::Failed);
        assert_eq!(snap.error.unwrap().code, "demo.failed");
    }

    #[tokio::test(start_paused = true)]
    async fn cancel_goes_through_cancelling() {
        let (tasks, _) = manager();
        let id = tasks.spawn("test", |handle| async move {
            handle.token().cancelled().await;
            Err(CoreError::Cancelled)
        });
        settle().await;
        assert_eq!(tasks.get(&id).unwrap().state, TaskState::Running);
        tasks.cancel(&id).unwrap();
        assert_eq!(tasks.get(&id).unwrap().state, TaskState::Cancelling);
        settle().await;
        assert_eq!(tasks.get(&id).unwrap().state, TaskState::Cancelled);
        // 終了したタスクへのキャンセルは何もしない
        tasks.cancel(&id).unwrap();
        assert_eq!(
            tasks.cancel("missing").unwrap_err().code(),
            "task.not_found"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn progress_notifications_are_throttled() {
        let (tasks, recorder) = manager();
        let id = tasks.spawn("test", |handle| async move {
            for i in 0..100 {
                handle.set_progress(i, 100);
            }
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            handle.set_progress(100, 100);
            Ok(())
        });
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        settle().await;
        let changes = recorder.changes_for(&id);
        let progress = changes
            .iter()
            .filter(|(_, c)| *c == TaskChange::Progress)
            .count();
        assert_eq!(
            progress, 2,
            "one burst notification and one after the interval: {changes:?}"
        );
        assert_eq!(changes.last().unwrap().1, TaskChange::State);
        assert_eq!(tasks.get(&id).unwrap().progress.current, 100);
    }

    #[tokio::test(start_paused = true)]
    async fn spawn_unique_reuses_the_active_task() {
        let (tasks, _) = manager();
        let work = |handle: TaskHandle| async move {
            handle.token().cancelled().await;
            Err(CoreError::Cancelled)
        };
        let first = tasks.spawn_unique("demo", work);
        let second = tasks.spawn_unique("demo", work);
        assert_eq!(first, second);
        tasks.cancel(&first).unwrap();
        settle().await;
        let third = tasks.spawn_unique("demo", work);
        assert_ne!(first, third);
    }

    #[tokio::test(start_paused = true)]
    async fn finished_tasks_are_pruned() {
        let (tasks, recorder) = manager();
        let mut ids = Vec::new();
        for _ in 0..(MAX_FINISHED_TASKS + 5) {
            ids.push(tasks.spawn("test", |_| async { Ok(()) }));
            settle().await;
            tokio::time::advance(std::time::Duration::from_millis(1)).await;
        }
        assert_eq!(tasks.list().len(), MAX_FINISHED_TASKS);
        for old in &ids[..5] {
            assert!(tasks.get(old).is_err());
            assert!(
                recorder
                    .changes_for(old)
                    .iter()
                    .any(|(_, c)| *c == TaskChange::Removed)
            );
        }
    }
}
