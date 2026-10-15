//! ダミーの秘密の値が、ログ、ErrorPayload、TaskSnapshot に出ないことを確かめる。

use std::io::Write;
use std::sync::{Arc, Mutex};

use nexora_core::error::CoreError;
use nexora_core::task::{TaskChange, TaskManager, TaskNotifier};
use secrecy::SecretString;

const SENTINEL: &str = "sentinel-7f3a9c";

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Silent;
impl TaskNotifier for Silent {
    fn notify(&self, _: &str, _: u64, _: TaskChange) {}
}

#[test]
fn secret_does_not_reach_logs() {
    let capture = Capture::default();
    let writer = capture.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .finish();
    let secret = SecretString::from(SENTINEL.to_string());
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(token = ?secret, "refreshing token");
    });
    let logged = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(logged.contains("refreshing token"));
    assert!(
        !logged.contains(SENTINEL),
        "secret leaked into log: {logged}"
    );
}

#[tokio::test]
async fn secret_does_not_reach_error_payload_or_snapshot() {
    let secret = SecretString::from(SENTINEL.to_string());
    let tasks = TaskManager::new(Arc::new(Silent));
    let id = tasks.spawn("test", move |_| async move {
        // 秘密の値を含む文脈でエラーを作っても、Debug は中身を出さない
        Err(CoreError::Unexpected {
            message: format!("refresh failed with {secret:?}"),
        })
    });
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    let snapshot = tasks.get(&id).unwrap();
    let json = serde_json::to_string(&snapshot).unwrap();
    assert!(json.contains("app.unexpected"));
    assert!(
        !json.contains(SENTINEL),
        "secret leaked into snapshot: {json}"
    );
}
