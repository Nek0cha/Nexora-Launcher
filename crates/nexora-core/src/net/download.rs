//! 足りないファイルだけを並列に取得する。`.part` に書き込み、検証してからリネームする。
//!
//! `shared/` は複数のインスタンスで共有するため、同じファイルを別々のタスクが同時に
//! 取得することがある。そのため、ファイルごとのロック（`lock_path`）を取ってから
//! 取得の要否を確かめ直し、`.part` の名前も試行ごとに一意にして、自分の `.part` だけを消す。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use futures::{StreamExt, TryStreamExt};
use serde::Serialize;
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::hash::{hex, sha1_file};
use super::{Http, MAX_RETRIES, io_err, is_transient, lock_path, offline};
use crate::error::CoreError;

/// 既存のファイルをどこまで確かめるか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// 存在とサイズだけ（起動時）。
    Quick,
    /// sha1 まで（インストール時）。
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DownloadItem {
    pub url: String,
    pub path: PathBuf,
    pub size: Option<u64>,
    pub sha1: Option<String>,
    pub executable: bool,
}

/// path の書きかけのファイルの名前（`<name>.<uuid>.part`）。呼ぶたびに別の名前になる。
pub fn part_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(format!(".{}.part", uuid::Uuid::now_v7().simple()));
    PathBuf::from(s)
}

/// この fetch_all が作った `.part` の一覧。失敗したときに、自分の分だけを消すために使う。
#[derive(Default)]
struct Parts(Mutex<HashSet<PathBuf>>);

impl Parts {
    fn add(&self, part: &Path) {
        self.set().insert(part.to_path_buf());
    }

    fn forget(&self, part: &Path) {
        self.set().remove(part);
    }

    fn take(&self) -> Vec<PathBuf> {
        self.set().drain().collect()
    }

    // std の Mutex。呼び出し側は .await をまたいで持たない
    fn set(&self) -> std::sync::MutexGuard<'_, HashSet<PathBuf>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// 取得し直す必要があるか。
pub fn needs_download(item: &DownloadItem, depth: Depth) -> std::io::Result<bool> {
    let meta = match std::fs::metadata(&item.path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(err) => return Err(err),
    };
    if !meta.is_file() {
        return Ok(true);
    }
    if item.size.is_some_and(|size| meta.len() != size) {
        return Ok(true);
    }
    if depth == Depth::Full
        && let Some(expected) = &item.sha1
    {
        return Ok(!sha1_file(&item.path)?.eq_ignore_ascii_case(expected));
    }
    Ok(false)
}

fn join_err(err: tokio::task::JoinError) -> CoreError {
    CoreError::Unexpected {
        message: err.to_string(),
    }
}

/// items のうち足りないものを取得する。最初の失敗で止め、この呼び出しが作った `.part` を消す。
pub async fn fetch_all<F>(
    http: &Http,
    items: Vec<DownloadItem>,
    depth: Depth,
    concurrency: usize,
    token: &CancellationToken,
    on_progress: F,
) -> Result<(), CoreError>
where
    F: Fn(u64, u64) + Send + Sync,
{
    if token.is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    let mut seen = HashSet::new();
    let items: Vec<DownloadItem> = items
        .into_iter()
        .filter(|i| seen.insert(i.path.clone()))
        .collect();
    let pending = tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        for item in items {
            if needs_download(&item, depth).map_err(|source| io_err(&item.path, source))? {
                out.push(item);
            }
        }
        Ok::<_, CoreError>(out)
    })
    .await
    .map_err(join_err)??;

    let total: u64 = pending.iter().filter_map(|i| i.size).sum();
    let done = AtomicU64::new(0);
    let parts = Parts::default();
    on_progress(0, total);
    let result = futures::stream::iter(pending.iter().map(Ok::<_, CoreError>))
        .try_for_each_concurrent(concurrency.clamp(1, 16), |item| {
            let ctx = Ctx {
                http,
                token,
                depth,
                done: &done,
                total,
                parts: &parts,
            };
            download_one(ctx, item, &on_progress)
        })
        .await;
    if result.is_err() {
        // 途中で止められた取得の `.part` も、ここで消す（ほかのタスクの `.part` には触れない）
        for part in parts.take() {
            let _ = tokio::fs::remove_file(part).await;
        }
    }
    result
}

/// 1 回の fetch_all の中で、各項目の取得が共有するもの。
#[derive(Clone, Copy)]
struct Ctx<'a> {
    http: &'a Http,
    token: &'a CancellationToken,
    depth: Depth,
    done: &'a AtomicU64,
    total: u64,
    parts: &'a Parts,
}

async fn download_one<F>(
    ctx: Ctx<'_>,
    item: &DownloadItem,
    on_progress: &F,
) -> Result<(), CoreError>
where
    F: Fn(u64, u64) + Send + Sync,
{
    let Ctx {
        http,
        token,
        depth,
        done,
        total,
        parts,
    } = ctx;
    // 同じファイルを書くほかのタスクが終わるまで待つ
    let _guard = tokio::select! {
        _ = token.cancelled() => return Err(CoreError::Cancelled),
        guard = lock_path(&item.path) => guard,
    };
    // 待っている間に、ほかのタスクが取得を済ませたかもしれない
    let still_needed = {
        let item = item.clone();
        tokio::task::spawn_blocking(move || {
            needs_download(&item, depth).map_err(|source| io_err(&item.path, source))
        })
        .await
        .map_err(join_err)??
    };
    if !still_needed {
        let n = item.size.unwrap_or(0);
        let now = done.fetch_add(n, Ordering::Relaxed) + n;
        on_progress(now, total);
        return Ok(());
    }
    let mut attempt = 0;
    loop {
        let mut written = 0u64;
        let part = part_path(&item.path);
        parts.add(&part);
        let result = tokio::select! {
            _ = token.cancelled() => Err(CoreError::Cancelled),
            r = attempt_download(http, item, &part, &mut written, done, total, on_progress) => r,
        };
        match result {
            Ok(()) => {
                parts.forget(&part);
                return Ok(());
            }
            Err(err) => {
                // この試行で数えた分は取り消す
                done.fetch_sub(written, Ordering::Relaxed);
                let _ = tokio::fs::remove_file(&part).await;
                parts.forget(&part);
                if is_transient(&err) && attempt < MAX_RETRIES {
                    tokio::select! {
                        _ = token.cancelled() => return Err(CoreError::Cancelled),
                        _ = tokio::time::sleep(http.backoff(attempt)) => {}
                    }
                    attempt += 1;
                    continue;
                }
                return Err(err);
            }
        }
    }
}

async fn attempt_download<F>(
    http: &Http,
    item: &DownloadItem,
    part: &Path,
    written: &mut u64,
    done: &AtomicU64,
    total: u64,
    on_progress: &F,
) -> Result<(), CoreError>
where
    F: Fn(u64, u64) + Send + Sync,
{
    if let Some(parent) = item.path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|s| io_err(parent, s))?;
    }
    let response = http.client().get(&item.url).send().await.map_err(offline)?;
    let status = response.status();
    if !status.is_success() {
        return Err(CoreError::HttpStatus {
            status: status.as_u16(),
            url: item.url.clone(),
        });
    }
    let mut file = tokio::fs::File::create_new(part)
        .await
        .map_err(|s| io_err(part, s))?;
    let mut hasher = Sha1::new();
    let mut stream = response.bytes_stream();
    let mut size = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(offline)?;
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(|s| io_err(part, s))?;
        let n = chunk.len() as u64;
        size += n;
        *written += n;
        let now = done.fetch_add(n, Ordering::Relaxed) + n;
        on_progress(now, total);
    }
    file.flush().await.map_err(|s| io_err(part, s))?;
    drop(file);
    let actual = hex(&hasher.finalize());
    let size_ok = item.size.is_none_or(|s| s == size);
    let hash_ok = item
        .sha1
        .as_ref()
        .is_none_or(|e| e.eq_ignore_ascii_case(&actual));
    if !size_ok || !hash_ok {
        return Err(CoreError::HashMismatch {
            path: item.path.display().to_string(),
        });
    }
    if item.executable {
        set_executable(part)?;
    }
    tokio::fs::rename(part, &item.path)
        .await
        .map_err(|s| io_err(&item.path, s))?;
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), CoreError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .map_err(|s| io_err(path, s))?
        .permissions();
    perms.set_mode(perms.mode() | 0o755);
    std::fs::set_permissions(path, perms).map_err(|s| io_err(path, s))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), CoreError> {
    Ok(())
}
