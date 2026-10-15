//! プレイボタンの試作用のデモタスク。フェーズ 2 で本物の起動処理に置き換えたら削除する。

use std::time::Duration;

use crate::error::CoreError;
use crate::task::TaskManager;

pub const DEMO_KIND: &str = "demo";

/// 0.1 秒ごとに 1% ずつ進むタスクを始める。fail_at を指定すると、その % でエラーになる。
/// 実行中のデモタスクがあれば、その ID を返す。
pub fn start_demo(tasks: &TaskManager, fail_at: Option<u8>) -> String {
    tasks.spawn_unique(DEMO_KIND, None, move |handle| async move {
        for percent in 1..=100u32 {
            tokio::select! {
                _ = handle.token().cancelled() => return Err(CoreError::Cancelled),
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
            if fail_at.is_some_and(|f| u32::from(f) == percent) {
                return Err(CoreError::DemoFailed {
                    percent: percent as u8,
                });
            }
            handle.set_progress(percent, 100);
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{TaskChange, TaskNotifier, TaskState};
    use std::sync::Arc;

    struct Silent;
    impl TaskNotifier for Silent {
        fn notify(&self, _: &str, _: u64, _: TaskChange) {}
    }

    fn tasks() -> TaskManager {
        TaskManager::new(Arc::new(Silent))
    }

    #[tokio::test(start_paused = true)]
    async fn runs_to_completion_in_ten_seconds() {
        let tasks = tasks();
        let id = start_demo(&tasks, None);
        tokio::time::sleep(std::time::Duration::from_millis(10_100)).await;
        let snap = tasks.get(&id).unwrap();
        assert_eq!(snap.state, TaskState::Succeeded);
        assert_eq!(snap.progress.current, 100);
    }

    #[tokio::test(start_paused = true)]
    async fn fails_at_requested_percent() {
        let tasks = tasks();
        let id = start_demo(&tasks, Some(30));
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let snap = tasks.get(&id).unwrap();
        assert_eq!(snap.state, TaskState::Failed);
        assert_eq!(snap.error.unwrap().code, "demo.failed");
        assert_eq!(snap.progress.current, 29);
    }

    #[tokio::test(start_paused = true)]
    async fn second_start_returns_the_running_task() {
        let tasks = tasks();
        let first = start_demo(&tasks, None);
        let second = start_demo(&tasks, None);
        assert_eq!(first, second);
        assert_eq!(tasks.list().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn can_be_cancelled() {
        let tasks = tasks();
        let id = start_demo(&tasks, None);
        tokio::time::sleep(std::time::Duration::from_millis(550)).await;
        tasks.cancel(&id).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(tasks.get(&id).unwrap().state, TaskState::Cancelled);
    }
}
