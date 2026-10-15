//! 足りないファイルだけを並列に取得する。`.part` に書き込み、検証してからリネームする。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use futures::{StreamExt, TryStreamExt};
use serde::Serialize;
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::hash::{hex, sha1_file};
use super::{Http, MAX_RETRIES, io_err, is_transient, offline};
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

pub fn part_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
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

/// items のうち足りないものを取得する。最初の失敗で止め、書きかけの `.part` を消す。
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
    on_progress(0, total);
    let result = futures::stream::iter(pending.iter().map(Ok::<_, CoreError>))
        .try_for_each_concurrent(concurrency.clamp(1, 16), |item| {
            download_one(http, item, token, &done, total, &on_progress)
        })
        .await;
    if result.is_err() {
        for item in &pending {
            let _ = tokio::fs::remove_file(part_path(&item.path)).await;
        }
    }
    result
}

async fn download_one<F>(
    http: &Http,
    item: &DownloadItem,
    token: &CancellationToken,
    done: &AtomicU64,
    total: u64,
    on_progress: &F,
) -> Result<(), CoreError>
where
    F: Fn(u64, u64) + Send + Sync,
{
    let mut attempt = 0;
    loop {
        let mut written = 0u64;
        let result = tokio::select! {
            _ = token.cancelled() => Err(CoreError::Cancelled),
            r = attempt_download(http, item, &mut written, done, total, on_progress) => r,
        };
        match result {
            Ok(()) => return Ok(()),
            Err(err) => {
                // この試行で数えた分は取り消す
                done.fetch_sub(written, Ordering::Relaxed);
                let _ = tokio::fs::remove_file(part_path(&item.path)).await;
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
    let part = part_path(&item.path);
    let response = http.client().get(&item.url).send().await.map_err(offline)?;
    let status = response.status();
    if !status.is_success() {
        return Err(CoreError::HttpStatus {
            status: status.as_u16(),
            url: item.url.clone(),
        });
    }
    let mut file = tokio::fs::File::create(&part)
        .await
        .map_err(|s| io_err(&part, s))?;
    let mut hasher = Sha1::new();
    let mut stream = response.bytes_stream();
    let mut size = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(offline)?;
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(|s| io_err(&part, s))?;
        let n = chunk.len() as u64;
        size += n;
        *written += n;
        let now = done.fetch_add(n, Ordering::Relaxed) + n;
        on_progress(now, total);
    }
    file.flush().await.map_err(|s| io_err(&part, s))?;
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
        set_executable(&part)?;
    }
    tokio::fs::rename(&part, &item.path)
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
