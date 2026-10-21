//! 足りないファイルだけを並列に取得する。`.part` に書き込み、検証してからリネームする。
//!
//! `shared/` は複数のインスタンスで共有するため、同じファイルを別々のタスクが同時に
//! 取得することがある。そのため、ファイルごとのロック（`lock_path`）を取ってから
//! 取得の要否を確かめ直し、`.part` の名前も試行ごとに一意にして、自分の `.part` だけを消す。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures::StreamExt;
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

/// fetch_all_with が知らせる段階。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchPhase {
    /// 手元のファイルを確かめている（`checked` 件済み、全体で `total` 件）。
    Verifying { checked: u64, total: u64 },
    /// 確認を終え、足りないものの取得に入った。この後の on_progress はバイト数。
    Downloading,
}

/// 確認の進み具合を知らせる間隔（タスクの進捗の通知の間隔と同じ）。
const VERIFY_REPORT_INTERVAL: Duration = Duration::from_millis(100);

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
    fetch_all_with(http, items, depth, concurrency, token, |_| {}, on_progress).await
}

/// fetch_all に段階の通知を足したもの。まず手元のファイルを確かめ（`Verifying` で件数を知らせる）、
/// 次に `Downloading` を知らせてから、足りないものを取得する（on_progress はバイト数）。
/// 確認は別のスレッドで動くが、ファイルごとに取り消しを確かめ、取り消されたらすぐに `Cancelled` を返す。
pub async fn fetch_all_with<P, F>(
    http: &Http,
    items: Vec<DownloadItem>,
    depth: Depth,
    concurrency: usize,
    token: &CancellationToken,
    on_phase: P,
    on_progress: F,
) -> Result<(), CoreError>
where
    P: Fn(FetchPhase) + Send + Sync,
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
    let pending = verify(items, depth, token, &on_phase).await?;
    on_phase(FetchPhase::Downloading);

    let started = std::time::Instant::now();
    let files = pending.len();
    let total: u64 = pending.iter().filter_map(|i| i.size).sum();
    let done = AtomicU64::new(0);
    let parts = Parts::default();
    on_progress(0, total);
    // 最初の失敗で残りを止めるための子の token。途中の取得の future は落とさずに、取り消しを
    // 見て終わるまで待つ（落とすと、別のスレッドで進むファイルの作成や書き込みが、返った後も
    // 残りうる。呼び出し側がその直後に置き場所を消すと、消せなかったり作り直されたりする）
    let stop = token.child_token();
    let first_err: Mutex<Option<CoreError>> = Mutex::new(None);
    futures::stream::iter(pending.iter())
        .for_each_concurrent(concurrency.clamp(1, 16), |item| {
            let ctx = Ctx {
                http,
                token: &stop,
                depth,
                done: &done,
                total,
                parts: &parts,
            };
            let (stop, first_err) = (&stop, &first_err);
            let on_progress = &on_progress;
            async move {
                if let Err(err) = download_one(ctx, item, on_progress).await {
                    let mut slot = first_err.lock().unwrap_or_else(PoisonError::into_inner);
                    // 止めたことによる取り消しより、最初の失敗を返す
                    if slot.is_none() {
                        *slot = Some(err);
                    }
                    drop(slot);
                    stop.cancel();
                }
            }
        })
        .await;
    let result = match first_err
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
    {
        Some(err) => Err(err),
        None => Ok(()),
    };
    if result.is_err() {
        // 途中で止められた取得の `.part` も、ここで消す（ほかのタスクの `.part` には触れない）
        for part in parts.take() {
            let _ = tokio::fs::remove_file(part).await;
        }
    } else if files > 0 {
        tracing::info!(
            files,
            kib = total.div_ceil(1024),
            elapsed_ms = elapsed_ms(started),
            "downloads finished"
        );
    }
    result
}

fn elapsed_ms(since: std::time::Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// 取得が要るものだけを返す。確認は別のスレッドで 1 件ずつ行い、件数を一定の間隔で知らせる。
async fn verify<P>(
    items: Vec<DownloadItem>,
    depth: Depth,
    token: &CancellationToken,
    on_phase: &P,
) -> Result<Vec<DownloadItem>, CoreError>
where
    P: Fn(FetchPhase) + Send + Sync,
{
    let started = std::time::Instant::now();
    let total = items.len() as u64;
    let checked = Arc::new(AtomicU64::new(0));
    let mut last_reported = None;
    let mut report = |n: u64| {
        if last_reported != Some(n) {
            last_reported = Some(n);
            on_phase(FetchPhase::Verifying { checked: n, total });
        }
    };
    report(0);
    let job = {
        let checked = checked.clone();
        let token = token.clone();
        tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            for item in items {
                // 共有の assets などで数千件を sha1 まで読むため、1 件ごとに取り消しを確かめる
                if token.is_cancelled() {
                    return Err(CoreError::Cancelled);
                }
                if needs_download(&item, depth).map_err(|source| io_err(&item.path, source))? {
                    out.push(item);
                }
                checked.fetch_add(1, Ordering::Relaxed);
            }
            Ok::<_, CoreError>(out)
        })
    };
    tokio::pin!(job);
    let mut tick = tokio::time::interval_at(
        tokio::time::Instant::now() + VERIFY_REPORT_INTERVAL,
        VERIFY_REPORT_INTERVAL,
    );
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let pending = loop {
        tokio::select! {
            // 別のスレッドの確認は、次のファイルの前に取り消しに気づいて止まる。終わるのは待たない
            _ = token.cancelled() => return Err(CoreError::Cancelled),
            result = &mut job => break result.map_err(join_err)??,
            _ = tick.tick() => report(checked.load(Ordering::Relaxed)),
        }
    };
    report(total);
    tracing::info!(
        files = total,
        missing = pending.len(),
        full = depth == Depth::Full,
        elapsed_ms = elapsed_ms(started),
        "verification finished"
    );
    Ok(pending)
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
    if token.is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    // 同じファイルを書くほかのタスクが終わるまで待つ
    let _guard = tokio::select! {
        biased;
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
        // 取り消しは attempt_download の中で、ファイルの操作の合間にだけ見る（操作の途中で落とさない）
        let result = attempt_download(ctx, item, &part, &mut written, on_progress).await;
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

/// 1 回の取得。取り消しは通信を待つ間にだけ見て、ファイルの操作は落とさずに終わらせる。
async fn attempt_download<F>(
    ctx: Ctx<'_>,
    item: &DownloadItem,
    part: &Path,
    written: &mut u64,
    on_progress: &F,
) -> Result<(), CoreError>
where
    F: Fn(u64, u64) + Send + Sync,
{
    let Ctx {
        http,
        token,
        done,
        total,
        ..
    } = ctx;
    if token.is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    if let Some(parent) = item.path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|s| io_err(parent, s))?;
    }
    let response = tokio::select! {
        biased;
        _ = token.cancelled() => return Err(CoreError::Cancelled),
        r = http.client().get(&item.url).send() => r.map_err(offline)?,
    };
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
    let streamed: Result<(), CoreError> = async {
        loop {
            let next = tokio::select! {
                biased;
                _ = token.cancelled() => return Err(CoreError::Cancelled),
                next = stream.next() => next,
            };
            let Some(chunk) = next else {
                return Ok(());
            };
            let chunk = chunk.map_err(offline)?;
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(|s| io_err(part, s))?;
            let n = chunk.len() as u64;
            size += n;
            *written += n;
            let now = done.fetch_add(n, Ordering::Relaxed) + n;
            on_progress(now, total);
        }
    }
    .await;
    // tokio の書き込みは別のスレッドで進む。失敗や取り消しでも、書き終わるのを待ってから閉じる
    let flushed = file.flush().await.map_err(|s| io_err(part, s));
    drop(file);
    streamed?;
    flushed?;
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
