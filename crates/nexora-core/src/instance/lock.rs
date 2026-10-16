//! インスタンス単位の排他。同じインスタンスへの変更は 1 つずつ行う。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::OwnedMutexGuard;

#[derive(Clone, Default)]
pub struct InstanceLocks {
    inner: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

impl InstanceLocks {
    pub async fn lock(&self, instance_id: &str) -> OwnedMutexGuard<()> {
        let mutex = {
            let mut map = self.inner.lock().expect("instance lock map poisoned");
            map.entry(instance_id.to_string()).or_default().clone()
        };
        mutex.lock_owned().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test(start_paused = true)]
    async fn same_instance_is_serialized_and_others_are_not() {
        let locks = InstanceLocks::default();
        let held = locks.lock("a").await;
        let other = tokio::time::timeout(Duration::from_millis(10), locks.lock("b")).await;
        assert!(other.is_ok());
        let waiting = tokio::time::timeout(Duration::from_millis(10), locks.lock("a")).await;
        assert!(waiting.is_err());
        drop(held);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), locks.lock("a"))
                .await
                .is_ok()
        );
    }
}
