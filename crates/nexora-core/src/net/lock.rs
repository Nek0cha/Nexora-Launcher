//! 同じファイルへの書き込みを、プロセス全体で 1 つずつにするためのロック。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

type PathLock = tokio::sync::Mutex<()>;

fn table() -> &'static Mutex<HashMap<PathBuf, Weak<PathLock>>> {
    static TABLE: OnceLock<Mutex<HashMap<PathBuf, Weak<PathLock>>>> = OnceLock::new();
    TABLE.get_or_init(Default::default)
}

/// path の書き込みの権利を得る。返り値を持っている間、同じ path への書き込みは待たされる。
pub(crate) async fn lock_path(path: &Path) -> tokio::sync::OwnedMutexGuard<()> {
    let lock = {
        // std の Mutex は、この同期のブロックの中だけで持つ（.await をまたがない）
        let mut map = table().lock().unwrap_or_else(PoisonError::into_inner);
        map.retain(|_, weak| weak.strong_count() > 0);
        match map.get(path).and_then(Weak::upgrade) {
            Some(lock) => lock,
            None => {
                let lock = Arc::new(PathLock::new(()));
                map.insert(path.to_path_buf(), Arc::downgrade(&lock));
                lock
            }
        }
    };
    lock.lock_owned().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn same_path_is_exclusive_and_other_paths_are_not() {
        let path = Path::new("lock-test/a");
        let guard = lock_path(path).await;
        let waited =
            tokio::time::timeout(std::time::Duration::from_millis(50), lock_path(path)).await;
        assert!(waited.is_err(), "second lock on the same path must wait");
        // 別の path は待たない
        let _other = lock_path(Path::new("lock-test/b")).await;
        drop(guard);
        let _again = lock_path(path).await;
    }
}
