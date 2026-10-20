//! 長い処理をタスクとして動かし、状態の変化を通知する。

pub mod types;

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::error::CoreError;
pub use types::{TaskChange, TaskChangedEvent, TaskPhase, TaskProgress, TaskSnapshot, TaskState};

pub const MAX_FINISHED_TASKS: usize = 50;
pub const PROGRESS_NOTIFY_INTERVAL: Duration = Duration::from_millis(100);

/// タスクの状態の変化を外へ伝える口。Tauri 側が実装する。
///
/// Notifications may arrive out of revision order across threads. Consumers must
/// ignore any revision <= the last revision seen for that task and re-fetch the
/// snapshot when accepting a newer notification.
pub trait TaskNotifier: Send + Sync {
    fn notify(&self, task_id: &str, revision: u64, change: TaskChange);
}

struct Entry {
    snapshot: TaskSnapshot,
    token: CancellationToken,
    last_progress_notify: Option<Instant>,
    last_progress_revision: u64,
    progress_flush_pending: bool,
    /// いまの段階に入った時刻（段階ごとの所要時間をログに残すため）。
    phase_started: Option<std::time::Instant>,
    /// キャンセルを受け付けた時刻（取り消しが済むまでの時間をログに残すため）。
    cancel_requested: Option<std::time::Instant>,
}

#[derive(Clone)]
pub struct TaskManager {
    tasks: Arc<Mutex<HashMap<String, Entry>>>,
    notifier: Arc<dyn TaskNotifier>,
}

/// タスクの処理に渡すハンドル。`spawn_blocking` の中へ渡せるよう複製できる。
#[derive(Clone)]
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

    /// 段階を変える。変わったときだけ進捗を 0/0 に戻してすぐに通知する（同じ段階なら何もしない）。
    pub fn set_phase(&self, phase: TaskPhase) {
        self.manager.set_phase(&self.id, phase);
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn elapsed_ms(since: std::time::Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// いまの段階の所要時間をログに残す。段階が無ければ何もしない。
fn log_phase_end(entry: &mut Entry) {
    if let (Some(phase), Some(started)) = (entry.snapshot.phase, entry.phase_started.take()) {
        tracing::info!(
            task_id = %entry.snapshot.id,
            kind = %entry.snapshot.kind,
            phase = phase.as_str(),
            elapsed_ms = elapsed_ms(started),
            progress_total = entry.snapshot.progress.total,
            "task phase finished"
        );
    }
}

impl TaskManager {
    pub fn new(notifier: Arc<dyn TaskNotifier>) -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            notifier,
        }
    }

    /// タスクを作って、すぐに ID を返す。処理は tokio のタスクとして動く。
    pub fn spawn<F, Fut>(&self, kind: &str, instance_id: Option<String>, work: F) -> String
    where
        F: FnOnce(TaskHandle) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), CoreError>> + Send + 'static,
    {
        let (id, token) = {
            let mut tasks = self.tasks.lock().unwrap();
            self.insert(&mut tasks, kind, instance_id)
        };
        self.notifier.notify(&id, 1, TaskChange::State);
        self.run(id.clone(), token, work);
        id
    }

    /// 種類とインスタンスが同じで終了していないタスクがあれば、その ID を返す。なければ新しく作る。
    pub fn spawn_unique<F, Fut>(&self, kind: &str, instance_id: Option<String>, work: F) -> String
    where
        F: FnOnce(TaskHandle) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), CoreError>> + Send + 'static,
    {
        let (id, token) = {
            let mut tasks = self.tasks.lock().unwrap();
            let existing = tasks
                .values()
                .find(|e| {
                    e.snapshot.kind == kind
                        && e.snapshot.instance_id == instance_id
                        && !e.snapshot.state.is_terminal()
                })
                .map(|e| e.snapshot.id.clone());
            if let Some(existing) = existing {
                return existing;
            }
            self.insert(&mut tasks, kind, instance_id)
        };
        self.notifier.notify(&id, 1, TaskChange::State);
        self.run(id.clone(), token, work);
        id
    }

    fn insert(
        &self,
        tasks: &mut HashMap<String, Entry>,
        kind: &str,
        instance_id: Option<String>,
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
                    instance_id,
                    state: TaskState::Queued,
                    phase: None,
                    progress: TaskProgress::default(),
                    revision: 1,
                    error: None,
                    created_at: now,
                    updated_at: now,
                },
                token: token.clone(),
                last_progress_notify: None,
                last_progress_revision: 0,
                progress_flush_pending: false,
                phase_started: None,
                cancel_requested: None,
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
            let result = tokio::spawn(async move { work(handle).await })
                .await
                .unwrap_or_else(|_| {
                    Err(CoreError::Unexpected {
                        message: "task panicked".into(),
                    })
                });
            let (state, error) = match result {
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
            if state.is_terminal() {
                // 終わったタスクは段階を持たない。最後の段階の所要時間と、取り消しにかかった時間を残す
                log_phase_end(entry);
                entry.snapshot.phase = None;
                if state == TaskState::Cancelled
                    && let Some(at) = entry.cancel_requested
                {
                    tracing::info!(
                        task_id = %id,
                        kind = %entry.snapshot.kind,
                        elapsed_ms = elapsed_ms(at),
                        "task cancelled"
                    );
                }
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
        let (notify_revision, flush_at) = {
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
            // A pending flush owns the next notification, even if it is delayed
            // past its deadline while another thread updates progress.
            if due && !entry.progress_flush_pending {
                entry.last_progress_notify = Some(now);
                entry.last_progress_revision = entry.snapshot.revision;
                (Some(entry.snapshot.revision), None)
            } else if !entry.progress_flush_pending {
                entry.progress_flush_pending = true;
                (
                    None,
                    entry
                        .last_progress_notify
                        .map(|last| last + PROGRESS_NOTIFY_INTERVAL),
                )
            } else {
                (None, None)
            }
        };
        if let Some(revision) = notify_revision {
            self.notifier.notify(id, revision, TaskChange::Progress);
        }
        if let Some(deadline) = flush_at {
            let manager = self.clone();
            let id = id.to_string();
            tokio::spawn(async move {
                tokio::time::sleep_until(deadline).await;
                let revision = {
                    let mut tasks = manager.tasks.lock().unwrap();
                    let Some(entry) = tasks.get_mut(&id) else {
                        return;
                    };
                    entry.progress_flush_pending = false;
                    if entry.snapshot.state.is_terminal()
                        || entry.snapshot.revision <= entry.last_progress_revision
                    {
                        return;
                    }
                    entry.last_progress_notify = Some(Instant::now());
                    entry.last_progress_revision = entry.snapshot.revision;
                    entry.snapshot.revision
                };
                manager.notifier.notify(&id, revision, TaskChange::Progress);
            });
        }
    }

    fn set_phase(&self, id: &str, phase: TaskPhase) {
        let revision = {
            let mut tasks = self.tasks.lock().unwrap();
            let Some(entry) = tasks.get_mut(id) else {
                return;
            };
            if entry.snapshot.state.is_terminal() || entry.snapshot.phase == Some(phase) {
                return;
            }
            log_phase_end(entry);
            tracing::info!(
                task_id = %id,
                kind = %entry.snapshot.kind,
                phase = phase.as_str(),
                "task phase started"
            );
            entry.phase_started = Some(std::time::Instant::now());
            entry.snapshot.phase = Some(phase);
            // 段階ごとに単位が違うため、前の段階の数字を持ち越さない
            entry.snapshot.progress = TaskProgress::default();
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now_ms();
            // この通知が最新を運ぶため、待っている進捗の通知は同じ revision なら出さない
            entry.last_progress_revision = entry.snapshot.revision;
            // 新しい段階の最初の進捗（処理の数が決まった直後など）は間引かずに知らせる
            // 前の段階の通知待ちが残っていても、それに先を越されない
            entry.last_progress_notify = None;
            entry.progress_flush_pending = false;
            entry.snapshot.revision
        };
        self.notifier.notify(id, revision, TaskChange::Phase);
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
            entry.cancel_requested = Some(std::time::Instant::now());
            tracing::info!(
                task_id = %id,
                kind = %entry.snapshot.kind,
                phase = entry.snapshot.phase.map(TaskPhase::as_str),
                "cancel requested"
            );
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
    async fn spawn_unique_is_scoped_to_the_instance() {
        let (tasks, _) = manager();
        let slow = || {
            |_h: TaskHandle| async move {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok(())
            }
        };
        let a1 = tasks.spawn_unique("install", Some("a".into()), slow());
        let a2 = tasks.spawn_unique("install", Some("a".into()), slow());
        let b1 = tasks.spawn_unique("install", Some("b".into()), slow());
        let a_launch = tasks.spawn_unique("launch", Some("a".into()), slow());
        assert_eq!(a1, a2);
        assert_ne!(a1, b1);
        assert_ne!(a1, a_launch);
        assert_eq!(tasks.get(&a1).unwrap().instance_id.as_deref(), Some("a"));
        assert_eq!(tasks.get(&b1).unwrap().instance_id.as_deref(), Some("b"));
    }

    #[tokio::test(start_paused = true)]
    async fn successful_task_reaches_succeeded() {
        let (tasks, recorder) = manager();
        let id = tasks.spawn("test", None, |handle| async move {
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
        let id = tasks.spawn("test", None, |_| async {
            Err(CoreError::Offline {
                message: "x".into(),
            })
        });
        settle().await;
        let snap = tasks.get(&id).unwrap();
        assert_eq!(snap.state, TaskState::Failed);
        assert_eq!(snap.error.unwrap().code, "net.offline");
    }

    #[tokio::test(start_paused = true)]
    async fn panicking_task_fails_and_releases_its_unique_kind() {
        let (tasks, recorder) = manager();
        let id = tasks.spawn_unique("panic", None, |_| async {
            panic!("private panic payload");
        });
        settle().await;
        let snap = tasks.get(&id).unwrap();
        assert_eq!(snap.state, TaskState::Failed);
        let error = snap.error.unwrap();
        assert_eq!(error.code, "app.unexpected");
        assert_eq!(
            error,
            CoreError::Unexpected {
                message: "task panicked".into()
            }
            .to_payload()
        );
        assert_eq!(
            recorder.changes_for(&id).last(),
            Some(&(snap.revision, TaskChange::State))
        );
        let next = tasks.spawn_unique("panic", None, |_| async { Ok(()) });
        assert_ne!(id, next);
        settle().await;
        assert_eq!(tasks.get(&next).unwrap().state, TaskState::Succeeded);
    }

    #[tokio::test(start_paused = true)]
    async fn suppressed_progress_is_flushed_while_worker_stalls() {
        let (tasks, recorder) = manager();
        let id = tasks.spawn("test", None, |handle| async move {
            for i in 1..=100 {
                handle.set_progress(i, 100);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(())
        });
        settle().await;
        let latest = tasks.get(&id).unwrap().revision;
        let progress = || {
            recorder
                .changes_for(&id)
                .into_iter()
                .filter(|(_, change)| *change == TaskChange::Progress)
                .collect::<Vec<_>>()
        };
        assert_eq!(progress().len(), 1);
        assert!(progress()[0].0 < latest);
        tokio::time::advance(Duration::from_millis(99)).await;
        settle().await;
        assert_eq!(progress().len(), 1);
        tokio::time::advance(Duration::from_millis(1)).await;
        settle().await;
        assert_eq!(progress().len(), 2);
        assert_eq!(progress()[1], (latest, TaskChange::Progress));
        assert_eq!(tasks.get(&id).unwrap().state, TaskState::Running);
        tokio::time::advance(Duration::from_millis(900)).await;
        settle().await;
        assert_eq!(progress().len(), 2);
        assert_eq!(tasks.get(&id).unwrap().state, TaskState::Succeeded);
    }

    #[tokio::test(start_paused = true)]
    async fn phase_change_resets_progress_and_notifies_at_once() {
        let (tasks, recorder) = manager();
        let (go, wait) = tokio::sync::oneshot::channel::<()>();
        let id = tasks.spawn("test", None, |handle| async move {
            handle.set_phase(TaskPhase::Verifying);
            handle.set_progress(3, 10);
            // 通知の間隔の内でも、段階の変化はすぐに知らせる
            handle.set_phase(TaskPhase::Downloading);
            let _ = wait.await;
            Ok(())
        });
        settle().await;
        let snap = tasks.get(&id).unwrap();
        assert_eq!(snap.phase, Some(TaskPhase::Downloading));
        assert_eq!(snap.progress, TaskProgress::default());
        let phases: Vec<(u64, TaskChange)> = recorder
            .changes_for(&id)
            .into_iter()
            .filter(|(_, c)| *c == TaskChange::Phase)
            .collect();
        assert_eq!(phases.len(), 2, "{phases:?}");
        assert_eq!(phases[1].0, snap.revision);
        go.send(()).unwrap();
        settle().await;
        let snap = tasks.get(&id).unwrap();
        assert_eq!(snap.state, TaskState::Succeeded);
        assert_eq!(snap.phase, None, "a finished task has no phase");
    }

    #[tokio::test(start_paused = true)]
    async fn same_phase_keeps_progress() {
        let (tasks, recorder) = manager();
        let id = tasks.spawn("test", None, |handle| async move {
            handle.set_phase(TaskPhase::Verifying);
            handle.set_progress(3, 10);
            handle.set_phase(TaskPhase::Verifying);
            std::future::pending::<()>().await;
            Ok(())
        });
        settle().await;
        let snap = tasks.get(&id).unwrap();
        assert_eq!(snap.phase, Some(TaskPhase::Verifying));
        assert_eq!(
            snap.progress,
            TaskProgress {
                current: 3,
                total: 10
            }
        );
        let phases = recorder
            .changes_for(&id)
            .into_iter()
            .filter(|(_, c)| *c == TaskChange::Phase)
            .count();
        assert_eq!(phases, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_task_clears_its_phase() {
        let (tasks, _) = manager();
        let id = tasks.spawn("test", None, |handle| async move {
            handle.set_phase(TaskPhase::BuildingLoader);
            handle.token().cancelled().await;
            Err(CoreError::Cancelled)
        });
        settle().await;
        tasks.cancel(&id).unwrap();
        // キャンセル中も段階は残す（何を止めているかが分かるように）
        assert_eq!(
            tasks.get(&id).unwrap().phase,
            Some(TaskPhase::BuildingLoader)
        );
        settle().await;
        let snap = tasks.get(&id).unwrap();
        assert_eq!(snap.state, TaskState::Cancelled);
        assert_eq!(snap.phase, None);
    }

    #[tokio::test(start_paused = true)]
    async fn cancel_goes_through_cancelling() {
        let (tasks, _) = manager();
        let id = tasks.spawn("test", None, |handle| async move {
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
        let id = tasks.spawn("test", None, |handle| async move {
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
        let first = tasks.spawn_unique("demo", None, work);
        let second = tasks.spawn_unique("demo", None, work);
        assert_eq!(first, second);
        tasks.cancel(&first).unwrap();
        settle().await;
        let third = tasks.spawn_unique("demo", None, work);
        assert_ne!(first, third);
    }

    #[tokio::test(start_paused = true)]
    async fn finished_tasks_are_pruned() {
        let (tasks, recorder) = manager();
        let mut ids = Vec::new();
        for _ in 0..(MAX_FINISHED_TASKS + 5) {
            ids.push(tasks.spawn("test", None, |_| async { Ok(()) }));
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
