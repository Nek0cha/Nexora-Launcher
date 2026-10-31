//! パックのファイルの取得。許可したホストからだけ取り、ハッシュとサイズをすべて確かめる。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use futures::StreamExt;
use sha2::{Digest, Sha512};
use tokio_util::sync::CancellationToken;

use super::manifest::PackFile;
use crate::error::CoreError;
use crate::net::hash::hex;
use crate::net::{Depth, DownloadItem, HostPolicy, Http, fetch_all, io_err};

/// ダウンロードを許す配信元（完全一致）。`github.com` のリリースのファイルは、実際に
/// `release-assets.githubusercontent.com` へ 1 回リダイレクトされる（2026-10-31 に確認）。
/// `objects.githubusercontent.com` は従来の配信先で、古いリリースが使う可能性があるため残す。
/// GitLab の配信先は未確認で、`gitlab.com` 自身が返す場合だけを許す。
pub const ALLOWED_HOSTS: [&str; 6] = [
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "objects.githubusercontent.com",
    "gitlab.com",
];

/// 追うリダイレクトの最大回数。
pub const MAX_REDIRECTS: usize = 3;

/// 同時に取るファイルの数。
const CONCURRENCY: usize = 4;

pub fn production_policy() -> HostPolicy {
    HostPolicy::new(&ALLOWED_HOSTS)
}

/// `files` の `index` 番目を置くステージングの中のパス。
pub fn staged_path(staging: &Path, index: usize) -> PathBuf {
    staging.join(index.to_string())
}

fn sha512_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha512::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buf[..n]);
    }
}

/// 1 ファイルを、`downloads` の先頭から順に試して取る。最初に `sha1`、`sha512`、サイズが
/// すべて合ったものを採る。
async fn fetch_one(
    http: &Http,
    policy: &HostPolicy,
    file: &PackFile,
    dest: &Path,
    token: &CancellationToken,
) -> Result<(), CoreError> {
    let mut last = None;
    for url in &file.downloads {
        if let Err(err) = policy.check(url) {
            last = Some(err);
            continue;
        }
        let item = DownloadItem {
            url: url.clone(),
            path: dest.to_path_buf(),
            size: Some(file.file_size),
            sha1: Some(file.sha1.clone()),
            executable: false,
        };
        match fetch_all(http, vec![item], Depth::Full, 1, token, |_, _| {}).await {
            Ok(()) => {}
            Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
            Err(err) => {
                last = Some(err);
                continue;
            }
        }
        let owned = dest.to_path_buf();
        let actual = tokio::task::spawn_blocking(move || sha512_file(&owned))
            .await
            .map_err(|e| CoreError::Unexpected {
                message: e.to_string(),
            })?
            .map_err(|e| io_err(dest, e))?;
        if actual.eq_ignore_ascii_case(&file.sha512) {
            return Ok(());
        }
        let _ = tokio::fs::remove_file(dest).await;
        last = Some(CoreError::HashMismatch {
            path: dest.display().to_string(),
        });
    }
    Err(last.unwrap_or_else(|| CoreError::MrpackInvalid {
        reason: format!("{} has no download URL", file.path),
    }))
}

/// `files` を `staging` に取る。`i` 番目は `staged_path(staging, i)` に置く。
/// 1 つでも失敗すれば残りを止めて、最初の失敗を返す（置いたものを消すのは呼び出し側）。
/// 進捗は、取り終えたファイルの大きさの合計と全体の大きさ。
pub async fn download_pack_files(
    http: &Http,
    policy: &HostPolicy,
    files: &[&PackFile],
    staging: &Path,
    file_limit: u64,
    token: &CancellationToken,
    on_progress: impl Fn(u64, u64) + Send + Sync,
) -> Result<(), CoreError> {
    if let Some(big) = files.iter().find(|f| f.file_size > file_limit) {
        return Err(CoreError::MrpackLimit {
            reason: format!("{} is larger than the per-file limit", big.path),
        });
    }
    if token.is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    tokio::fs::create_dir_all(staging)
        .await
        .map_err(|e| io_err(staging, e))?;
    let total: u64 = files.iter().map(|f| f.file_size).sum();
    let done = AtomicU64::new(0);
    on_progress(0, total);
    let stop = token.child_token();
    let first_err: Mutex<Option<CoreError>> = Mutex::new(None);
    futures::stream::iter(files.iter().enumerate())
        .for_each_concurrent(CONCURRENCY, |(index, file)| {
            let (stop, first_err, done, on_progress) = (&stop, &first_err, &done, &on_progress);
            async move {
                if stop.is_cancelled() {
                    return;
                }
                let dest = staged_path(staging, index);
                match fetch_one(http, policy, file, &dest, stop).await {
                    Ok(()) => {
                        let now =
                            done.fetch_add(file.file_size, Ordering::Relaxed) + file.file_size;
                        on_progress(now, total);
                    }
                    Err(err) => {
                        let mut slot = first_err.lock().unwrap_or_else(PoisonError::into_inner);
                        if slot.is_none() {
                            *slot = Some(err);
                        }
                        drop(slot);
                        stop.cancel();
                    }
                }
            }
        })
        .await;
    match first_err
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
    {
        Some(err) => Err(err),
        None if token.is_cancelled() => Err(CoreError::Cancelled),
        None => Ok(()),
    }
}
