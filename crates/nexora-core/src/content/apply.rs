//! Modrinth の版の導入と更新の実行（spec 5-4 の 6.2、6.4、6.5 と spec 0 の 3.7 節）。
//!
//! 全体で成功か失敗のどちらかにする。手順：
//! 1. 計画を確かめる（URL のオリジン、ファイル名、置き場所の重なり）。ここまではファイルに触らない
//! 2. `journal::begin` で prepared を記録する（変更の対象と退避の名前を含む）
//! 3. `staging/<id>/new/` にダウンロードしてハッシュを確かめる
//! 4. 置き換える jar と、有効にする jar を `staging/<id>/backup/` に退避する
//! 5. 新しいファイルを `mods/` に置き、有効にする jar を `.disabled` を外した名前で置く
//! 6. 行の更新と `mark_committed` を 1 つのトランザクションで行う
//! 7. 置き換えた jar を `backups/<instance_id>/` に移して記録し、`staging/<id>/` と記録を消す
//!
//! 6 より前の失敗と取り消しでは、行ったことを逆の順に戻し、`staging/<id>/` と記録を消す。
//! 戻しきれなければ記録と staging を残し、次の起動の復旧（`journal::recover`）に任せる。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use super::journal::{self, Operation, Target};
use super::ops::content_name;
use super::resolve::{InstallPlan, PlannedFile};
use super::scan::{
    DISABLED_SUFFIX, DiskFile, classify, hash_file, io_error, local_row, mtime_ms, toggled,
};
use super::store::{self, Row};
use super::{ContentKind, InstalledAs, Provider};
use crate::db::Db;
use crate::error::{CoreError, ErrorPayload};
use crate::net::{Depth, DownloadItem, Http, fetch_all, part_path};
use crate::paths::AppPaths;

/// 版のファイルを取ってよいオリジン（本番）。これ以外の `url` の版は導入しない。
pub const MODRINTH_CDN: &str = "https://cdn.modrinth.com";

/// 更新の前の退避を `backups` に記録するときの理由。
const BACKUP_REASON: &str = "pre-update";
/// インスタンスごとに残す、更新の前の退避の数。
const BACKUP_KEEP: usize = 5;
/// 同時にダウンロードする数。
const CONCURRENCY: usize = 4;

/// 導入と更新に要るもの。
pub struct ApplyContext<'a> {
    pub db: &'a Db,
    pub http: &'a Http,
    pub paths: &'a AppPaths,
    pub game_dir: &'a Path,
    pub instance_id: &'a str,
    pub token: &'a CancellationToken,
    /// ファイルを取ってよいオリジン。本番は `MODRINTH_CDN`、テストではモックのサーバー。
    pub cdn: &'a str,
}

/// 1 回の `apply_plan` の結果。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ApplyOutcome {
    /// 新しく入れた MOD（依存を除く）。
    pub installed: u32,
    /// 依存として新しく入れた MOD。
    pub dependencies_added: u32,
    /// 更新で置き換えた MOD。
    pub replaced: u32,
}

/// 導入や「すべて更新」のタスクの結果。
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContentTaskReport {
    pub installed: u32,
    pub dependencies_added: u32,
    pub updated: u32,
    pub failures: Vec<ItemFailure>,
}

/// 失敗した 1 件。`name` は MOD の表示名（分からなければファイル名）。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ItemFailure {
    pub name: String,
    pub error: ErrorPayload,
}

/// 置く 1 件。
struct Placement<'p> {
    planned: &'p PlannedFile,
    /// 置き場所（`mods/<name>`。置き換える元が無効なら `.disabled` 付き）。
    dest_rel: String,
    enabled: bool,
    /// ダウンロード先のファイル名（`staging/<id>/new/` の中）。
    staged_name: String,
    /// 置き換える元の行。
    old: Option<Row>,
}

/// 置き換えで退避する 1 件。
struct Replaced {
    rel_path: String,
    backup_name: String,
}

/// 有効にする 1 件。元を退避し、その複製を `.disabled` を外した名前で置く。
struct Enabling {
    from_rel: String,
    to_rel: String,
    backup_name: String,
}

/// 確かめ終えた計画。
struct Prepared<'p> {
    placements: Vec<Placement<'p>>,
    replaced: Vec<Replaced>,
    enabling: Vec<Enabling>,
    targets: Vec<Target>,
}

/// 戻すために覚えておく、行ったこと。
enum Done {
    /// `orig` を `backup` に退避した。
    Backed { orig: PathBuf, backup: PathBuf },
    /// `dest` に新しいファイルを置いた。
    Placed { dest: PathBuf },
}

fn invalid_path() -> CoreError {
    CoreError::InvalidInstanceSetting {
        field: "rel_path".into(),
    }
}

fn invalid_metadata(message: String) -> CoreError {
    CoreError::InvalidMetadata { message }
}

/// `url` が `cdn` と同じオリジン（スキーム、ホスト、ポート）か。
fn check_origin(url: &str, cdn: &str) -> Result<(), CoreError> {
    let refused = || invalid_metadata(format!("download host is not allowed: {url}"));
    let allowed = reqwest::Url::parse(cdn).map_err(|_| refused())?.origin();
    let origin = reqwest::Url::parse(url).map_err(|_| refused())?.origin();
    if origin.is_tuple() && origin == allowed {
        Ok(())
    } else {
        Err(refused())
    }
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// 版のファイルの検証用のハッシュを確かめる。sha1 と sha512 の少なくとも一方が要る。
fn check_hashes(p: &PlannedFile) -> Result<(), CoreError> {
    let f = &p.file;
    let sha1_ok = f.sha1.is_empty() || is_hex(&f.sha1, 40);
    let sha512_ok = f.sha512.as_deref().is_none_or(|h| is_hex(h, 128));
    if !sha1_ok || !sha512_ok || (f.sha1.is_empty() && f.sha512.is_none()) {
        return Err(invalid_metadata(format!(
            "version {} has no usable hash for {}",
            p.version.id, f.filename
        )));
    }
    Ok(())
}

/// 大文字と小文字を区別しないファイルシステム（Windows）でも重ならないように比べるキー。
fn key(rel_path: &str) -> String {
    rel_path.to_lowercase()
}

/// 計画を確かめて、置き場所と変更の対象を決める。ファイルには触らない。
fn prepare<'p>(cx: &ApplyContext<'_>, plan: &'p InstallPlan) -> Result<Prepared<'p>, CoreError> {
    let kind = ContentKind::Mod;
    let mut placements = Vec::new();
    let mut replaced = Vec::new();
    for (i, planned) in plan.files.iter().enumerate() {
        check_origin(&planned.file.url, cx.cdn)?;
        let new_rel = format!("{}/{}", kind.dir(), planned.file.filename);
        let new_name = content_name(kind, &new_rel)?.to_string();
        // 版のファイルは有効な名前（`.jar`）で配られる
        if classify(&new_name, kind) != Some(true) {
            return Err(invalid_path());
        }
        check_hashes(planned)?;
        let (dest_rel, enabled, old) = match &planned.replaces {
            Some(rel) => {
                let name = content_name(kind, rel)?;
                let enabled = classify(name, kind) == Some(true);
                replaced.push(Replaced {
                    rel_path: rel.clone(),
                    backup_name: format!("{i}-{name}"),
                });
                // 元が無効なら、新しいファイルも無効のままにする
                let dest = if enabled {
                    new_rel
                } else {
                    format!("{new_rel}{DISABLED_SUFFIX}")
                };
                (dest, enabled, store::get(cx.db, cx.instance_id, rel)?)
            }
            None => (new_rel, true, None),
        };
        placements.push(Placement {
            planned,
            dest_rel,
            enabled,
            staged_name: format!("{i}-{new_name}"),
            old,
        });
    }
    let mut enabling = Vec::new();
    for (i, rel) in plan.enable.iter().enumerate() {
        let name = content_name(kind, rel)?;
        if classify(name, kind) != Some(false) {
            return Err(invalid_path());
        }
        enabling.push(Enabling {
            from_rel: rel.clone(),
            to_rel: toggled(rel),
            backup_name: format!("e{i}-{name}"),
        });
    }

    // 退避するもの（動かす元）と、新しく置くもの（置き場所）が重ならないことを確かめる
    let mut sources = HashSet::new();
    for rel in replaced
        .iter()
        .map(|r| &r.rel_path)
        .chain(enabling.iter().map(|e| &e.from_rel))
    {
        if !sources.insert(key(rel)) {
            return Err(CoreError::ContentExists { path: rel.clone() });
        }
    }
    let mut dests = HashSet::new();
    for p in &placements {
        // 同じ名前への更新は、元を退避してから置くので、元の場所と同じでよい
        let own = p.planned.replaces.as_deref() == Some(p.dest_rel.as_str());
        if !dests.insert(key(&p.dest_rel)) || (!own && sources.contains(&key(&p.dest_rel))) {
            return Err(CoreError::ContentExists {
                path: p.dest_rel.clone(),
            });
        }
        if !own && cx.game_dir.join(&p.dest_rel).symlink_metadata().is_ok() {
            return Err(CoreError::ContentExists {
                path: p.dest_rel.clone(),
            });
        }
    }
    for e in &enabling {
        if !dests.insert(key(&e.to_rel))
            || sources.contains(&key(&e.to_rel))
            || cx.game_dir.join(&e.to_rel).symlink_metadata().is_ok()
        {
            return Err(CoreError::ContentExists {
                path: e.to_rel.clone(),
            });
        }
    }

    // 記録する変更の対象。実際の名前（`.disabled` 付きを含む）で持つ
    let mut targets = Vec::new();
    for r in &replaced {
        targets.push(Target {
            rel_path: r.rel_path.clone(),
            backup: Some(r.backup_name.clone()),
            created: false,
        });
    }
    for p in &placements {
        // 同じ名前への更新は、上の退避の対象が戻す（`created` にすると元のファイルを消しうる）
        if p.planned.replaces.as_deref() != Some(p.dest_rel.as_str()) {
            targets.push(Target {
                rel_path: p.dest_rel.clone(),
                backup: None,
                created: true,
            });
        }
    }
    for e in &enabling {
        targets.push(Target {
            rel_path: e.from_rel.clone(),
            backup: Some(e.backup_name.clone()),
            created: false,
        });
        targets.push(Target {
            rel_path: e.to_rel.clone(),
            backup: None,
            created: true,
        });
    }
    Ok(Prepared {
        placements,
        replaced,
        enabling,
        targets,
    })
}

/// 計画を実行する。全体で成功か失敗のどちらかで、失敗と取り消しでは何も変えない。
/// `on_progress` はダウンロードのバイト数（済んだ分、全体）。
pub async fn apply_plan(
    cx: &ApplyContext<'_>,
    plan: &InstallPlan,
    now_ms: i64,
    on_progress: impl Fn(u64, u64) + Send + Sync,
) -> Result<ApplyOutcome, CoreError> {
    let prepared = prepare(cx, plan)?;
    check_cancel(cx.token)?;
    let kind = if prepared.replaced.is_empty() {
        "install"
    } else {
        "update"
    };
    let op = journal::begin(cx.db, kind, cx.instance_id, &prepared.targets, now_ms)?;
    let staging = cx.paths.staging_dir().join(&op.id);
    let mut done = Vec::new();
    match run(cx, &prepared, &op, &staging, &mut done, &on_progress).await {
        Ok(()) => {
            finish_committed(cx, &prepared, &op, &staging, now_ms);
            Ok(outcome(&prepared))
        }
        Err(err) => {
            abort(cx, &op, &staging, &done);
            Err(err)
        }
    }
}

fn outcome(prepared: &Prepared<'_>) -> ApplyOutcome {
    let mut out = ApplyOutcome::default();
    for p in &prepared.placements {
        if p.planned.replaces.is_some() {
            out.replaced += 1;
        } else if p.planned.as_dependency {
            out.dependencies_added += 1;
        } else {
            out.installed += 1;
        }
    }
    out
}

fn check_cancel(token: &CancellationToken) -> Result<(), CoreError> {
    if token.is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    Ok(())
}

/// 確定（DB のコミット）までを行う。`Err` のときは、`done` を戻せば元の状態になる。
async fn run<F>(
    cx: &ApplyContext<'_>,
    prepared: &Prepared<'_>,
    op: &Operation,
    staging: &Path,
    done: &mut Vec<Done>,
    on_progress: &F,
) -> Result<(), CoreError>
where
    F: Fn(u64, u64) + Send + Sync,
{
    let new_dir = staging.join("new");
    let backup_dir = staging.join("backup");
    std::fs::create_dir_all(&new_dir).map_err(|e| io_error(&new_dir, e))?;
    std::fs::create_dir_all(&backup_dir).map_err(|e| io_error(&backup_dir, e))?;

    // 3. ダウンロードと検証
    let hashes = download(cx, prepared, &new_dir, on_progress).await?;
    check_cancel(cx.token)?;

    // 4. 退避。置き換える jar と、有効にする jar
    let mods = cx.game_dir.join(ContentKind::Mod.dir());
    std::fs::create_dir_all(&mods).map_err(|e| io_error(&mods, e))?;
    for r in &prepared.replaced {
        let orig = cx.game_dir.join(&r.rel_path);
        // 行はあるがファイルが無いときは、退避するものが無い
        if orig.symlink_metadata().is_err() {
            continue;
        }
        let backup = backup_dir.join(&r.backup_name);
        move_file(&orig, &backup)?;
        done.push(Done::Backed { orig, backup });
    }
    let mut copies = Vec::new();
    for e in &prepared.enabling {
        check_cancel(cx.token)?;
        let orig = cx.game_dir.join(&e.from_rel);
        let backup = backup_dir.join(&e.backup_name);
        move_file(&orig, &backup)?;
        done.push(Done::Backed {
            orig,
            backup: backup.clone(),
        });
        // 退避は復旧のために残し、置くのは複製にする
        let copy = new_dir.join(&e.backup_name);
        std::fs::copy(&backup, &copy).map_err(|err| io_error(&copy, err))?;
        copies.push(copy);
    }

    // 5. 置く
    for p in &prepared.placements {
        check_cancel(cx.token)?;
        place(
            cx.game_dir,
            &new_dir.join(&p.staged_name),
            &p.dest_rel,
            done,
        )?;
    }
    for (e, copy) in prepared.enabling.iter().zip(&copies) {
        check_cancel(cx.token)?;
        place(cx.game_dir, copy, &e.to_rel, done)?;
    }
    check_cancel(cx.token)?;

    // 6. 行の更新と確定を 1 つのトランザクションで
    let mut rows = Vec::new();
    for (p, (sha1, sha512)) in prepared.placements.iter().zip(hashes) {
        rows.push(new_row(cx.game_dir, p, sha1, sha512)?);
    }
    cx.db.with_conn(|conn| {
        let tx = conn.transaction()?;
        for (p, row) in prepared.placements.iter().zip(&rows) {
            if let Some(old) = p.planned.replaces.as_deref()
                && old != p.dest_rel
            {
                store::delete_in(&tx, cx.instance_id, old)?;
            }
            store::upsert_in(&tx, cx.instance_id, row)?;
        }
        for e in &prepared.enabling {
            store::rename_in(&tx, cx.instance_id, &e.from_rel, &e.to_rel, true)?;
        }
        journal::mark_committed(&tx, &op.id)?;
        tx.commit()?;
        Ok(())
    })
}

/// 計画のファイルを `new_dir` に取り、sha1 と（あれば）sha512 を確かめる。並びは `placements` と同じ。
async fn download<F>(
    cx: &ApplyContext<'_>,
    prepared: &Prepared<'_>,
    new_dir: &Path,
    on_progress: &F,
) -> Result<Vec<(String, String)>, CoreError>
where
    F: Fn(u64, u64) + Send + Sync,
{
    if prepared.placements.is_empty() {
        return Ok(Vec::new());
    }
    let items: Vec<DownloadItem> = prepared
        .placements
        .iter()
        .map(|p| {
            let f = &p.planned.file;
            DownloadItem {
                url: f.url.clone(),
                path: new_dir.join(&p.staged_name),
                // Modrinth が大きさを返さなかった（0 の）ときは確かめない
                size: (f.size > 0).then_some(f.size),
                sha1: (!f.sha1.is_empty()).then(|| f.sha1.clone()),
                executable: false,
            }
        })
        .collect();
    // staging は新しいフォルダーなので、手元のファイルを sha1 まで確かめる必要はない
    fetch_all(
        cx.http,
        items,
        Depth::Quick,
        CONCURRENCY,
        cx.token,
        on_progress,
    )
    .await?;

    let files: Vec<(PathBuf, String, Option<String>)> = prepared
        .placements
        .iter()
        .map(|p| {
            (
                new_dir.join(&p.staged_name),
                p.planned.file.sha1.clone(),
                p.planned.file.sha512.clone(),
            )
        })
        .collect();
    let token = cx.token.clone();
    tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        for (path, sha1, sha512) in files {
            if token.is_cancelled() {
                return Err(CoreError::Cancelled);
            }
            let (actual1, actual512) = hash_file(&path).map_err(|e| io_error(&path, e))?;
            let sha1_ok = sha1.is_empty() || sha1.eq_ignore_ascii_case(&actual1);
            let sha512_ok = sha512
                .as_deref()
                .is_none_or(|h| h.eq_ignore_ascii_case(&actual512));
            if !sha1_ok || !sha512_ok {
                return Err(CoreError::HashMismatch {
                    path: path.display().to_string(),
                });
            }
            out.push((actual1, actual512));
        }
        Ok(out)
    })
    .await
    .map_err(|e| CoreError::Unexpected {
        message: e.to_string(),
    })?
}

/// 置いたファイルの行。特定の情報（名前、説明など）は次の特定で埋めるので、`identified_at` は空にする。
fn new_row(
    game_dir: &Path,
    p: &Placement<'_>,
    sha1: String,
    sha512: String,
) -> Result<Row, CoreError> {
    let path = game_dir.join(&p.dest_rel);
    let meta = std::fs::metadata(&path).map_err(|e| io_error(&path, e))?;
    let file = DiskFile {
        rel_path: p.dest_rel.clone(),
        enabled: p.enabled,
        size: meta.len(),
        mtime_ms: mtime_ms(&meta).map_err(|e| io_error(&path, e))?,
    };
    let mut row = local_row(ContentKind::Mod, &file, sha1, sha512);
    let v = &p.planned.version;
    row.item.provider = Provider::Modrinth;
    row.item.project_id = Some(v.project_id.clone());
    row.item.version_id = Some(v.id.clone());
    row.item.version_number = Some(v.version_number.clone());
    match &p.old {
        // 入れた経緯と固定は、利用者の意図として元の行から引き継ぐ
        Some(old) => {
            row.item.installed_as = old.item.installed_as;
            row.item.pinned = old.item.pinned;
        }
        None if p.planned.as_dependency => row.item.installed_as = InstalledAs::Dependency,
        None => {}
    }
    Ok(row)
}

/// `staged` を `game_dir/dest_rel` に置く。置き場所に何かあれば上書きせずに `ContentExists`。
fn place(
    game_dir: &Path,
    staged: &Path,
    dest_rel: &str,
    done: &mut Vec<Done>,
) -> Result<(), CoreError> {
    let dest = game_dir.join(dest_rel);
    // Windows の rename は既存のファイルを置き換えるので、先に確かめる
    if dest.symlink_metadata().is_ok() {
        return Err(CoreError::ContentExists {
            path: dest_rel.to_string(),
        });
    }
    move_file(staged, &dest)?;
    done.push(Done::Placed { dest });
    Ok(())
}

/// ファイルを動かす。rename できなければ（別のドライブなど）コピーしてから元を消す。
/// 元を消せなければコピーを消して `Err` を返し、元の場所だけに残す。
fn move_file(from: &Path, to: &Path) -> Result<(), CoreError> {
    move_file_with(from, to, |a, b| std::fs::rename(a, b))
}

/// `move_file` の本体。テストで rename の失敗を起こせるように、rename を受け取る。
///
/// コピーは `<to>.<uuid>.part` に書いてディスクまで同期し、書き終えてから `to` へ rename する。
/// 途中で落ちても `to` の名前に書きかけが現れることはない（退避の書きかけを復旧が元の jar に
/// 上書きするのを防ぐ）。残った `.part` は記録に無い名前で、種類の拡張子でもないので、
/// 復旧にも走査にも拾われず、staging ごと消える。
fn move_file_with(
    from: &Path,
    to: &Path,
    rename: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), CoreError> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
    }
    let rename_err = match rename(from, to) {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };
    if !from.is_file() {
        return Err(io_error(from, rename_err));
    }
    let part = part_path(to);
    if let Err(err) = copy_synced(from, &part) {
        let _ = std::fs::remove_file(&part);
        return Err(err);
    }
    if let Err(e) = rename(&part, to) {
        let _ = std::fs::remove_file(&part);
        return Err(io_error(to, e));
    }
    if let Err(e) = std::fs::remove_file(from) {
        let _ = std::fs::remove_file(to);
        return Err(io_error(from, e));
    }
    Ok(())
}

/// `from` を新しい `to` にコピーし、中身をディスクまで書き出す。
fn copy_synced(from: &Path, to: &Path) -> Result<(), CoreError> {
    let mut input = std::fs::File::open(from).map_err(|e| io_error(from, e))?;
    let mut output = std::fs::File::create_new(to).map_err(|e| io_error(to, e))?;
    std::io::copy(&mut input, &mut output).map_err(|e| io_error(to, e))?;
    output.sync_all().map_err(|e| io_error(to, e))
}

fn remove_file_if_exists(path: &Path) -> Result<(), CoreError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_error(path, e)),
    }
}

/// 確定の前の失敗で、行ったことを逆の順に戻す。戻しきれたら staging と記録を消す。
/// 戻しきれなければ両方を残し、次の起動の復旧に任せる。
fn abort(cx: &ApplyContext<'_>, op: &Operation, staging: &Path, done: &[Done]) {
    let mut ok = true;
    for d in done.iter().rev() {
        let result = match d {
            Done::Placed { dest } => remove_file_if_exists(dest),
            Done::Backed { orig, backup } => move_file(backup, orig),
        };
        if let Err(err) = result {
            ok = false;
            tracing::warn!(op_id = %op.id, code = err.code(), error = %err, "could not roll back a content change");
        }
    }
    if !ok {
        return;
    }
    if let Err(e) = std::fs::remove_dir_all(staging)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(op_id = %op.id, error = %e, "could not remove the staging folder");
    }
    if let Err(err) = journal::finish(cx.db, &op.id) {
        tracing::warn!(op_id = %op.id, code = err.code(), "could not remove an operation record");
    }
}

/// 確定の後の片付け。ここでの失敗は導入の失敗にしない（警告に残す）。
/// staging を消せなければ記録を残し、次の起動の復旧で staging を消す。
fn finish_committed(
    cx: &ApplyContext<'_>,
    prepared: &Prepared<'_>,
    op: &Operation,
    staging: &Path,
    now_ms: i64,
) {
    let backup_dir = staging.join("backup");
    for r in &prepared.replaced {
        let from = backup_dir.join(&r.backup_name);
        if !from.is_file() {
            continue;
        }
        if let Err(err) = keep_backup(cx, op, &from, &r.backup_name, now_ms) {
            tracing::warn!(op_id = %op.id, code = err.code(), error = %err, "could not keep a pre-update backup");
        }
    }
    match std::fs::remove_dir_all(staging) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            tracing::warn!(op_id = %op.id, error = %e, "could not remove the staging folder");
            return;
        }
    }
    if let Err(err) = journal::finish(cx.db, &op.id) {
        tracing::warn!(op_id = %op.id, code = err.code(), "could not remove an operation record");
    }
    if let Err(err) = prune(cx) {
        tracing::warn!(code = err.code(), error = %err, "could not prune pre-update backups");
    }
}

/// 退避した jar を `backups/<instance_id>/<操作 ID>-<名前>` に移して記録する。
fn keep_backup(
    cx: &ApplyContext<'_>,
    op: &Operation,
    from: &Path,
    backup_name: &str,
    now_ms: i64,
) -> Result<(), CoreError> {
    let rel = format!("{}/{}-{backup_name}", cx.instance_id, op.id);
    if !journal::is_safe_relative(&rel) {
        return Err(invalid_path());
    }
    let to = cx.paths.backups_dir().join(&rel);
    move_file(from, &to)?;
    let size = std::fs::metadata(&to).map_or(0, |m| m.len());
    journal::add_backup(cx.db, cx.instance_id, BACKUP_REASON, &rel, size, now_ms)
}

/// 新しい `BACKUP_KEEP` 件を残して、古い退避の記録とファイルを消す。
fn prune(cx: &ApplyContext<'_>) -> Result<(), CoreError> {
    let removed = journal::prune_backups(cx.db, cx.instance_id, BACKUP_REASON, BACKUP_KEEP)?;
    for rel in removed {
        // 記録は自分で書いたものだが、backups の外を消さないように確かめる
        if !journal::is_safe_relative(&rel) {
            tracing::warn!(path = %rel, "skipping an unsafe backup path");
            continue;
        }
        if let Err(err) = remove_file_if_exists(&cx.paths.backups_dir().join(&rel)) {
            tracing::warn!(code = err.code(), error = %err, "could not remove an old backup");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_check_compares_scheme_host_and_port() {
        let cdn = MODRINTH_CDN;
        assert!(check_origin("https://cdn.modrinth.com/data/a/b.jar", cdn).is_ok());
        assert!(check_origin("https://cdn.modrinth.com:443/data/a/b.jar", cdn).is_ok());
        for url in [
            "http://cdn.modrinth.com/data/a/b.jar",
            "https://cdn.modrinth.com:8443/data/a/b.jar",
            "https://cdn.modrinth.com.evil.invalid/b.jar",
            "https://evil.invalid/cdn.modrinth.com/b.jar",
            "file:///C:/b.jar",
            "",
        ] {
            assert!(check_origin(url, cdn).is_err(), "{url}");
        }
    }

    #[test]
    fn hex_check_wants_exact_length_and_hex_digits() {
        assert!(is_hex(&"a".repeat(40), 40));
        assert!(!is_hex(&"a".repeat(39), 40));
        assert!(!is_hex(&"g".repeat(40), 40));
    }

    use crate::content::modrinth::{FileInfo, FullVersion};
    use crate::instance::{self, NewInstance};

    fn planned(project: &str, filename: &str, replaces: Option<&str>) -> PlannedFile {
        let file = FileInfo {
            url: format!("{MODRINTH_CDN}/data/{project}/{filename}"),
            filename: filename.to_string(),
            sha1: "0".repeat(40),
            sha512: None,
            size: 1,
            primary: true,
        };
        PlannedFile {
            version: FullVersion {
                id: format!("{project}-v"),
                project_id: project.to_string(),
                version_number: "1".into(),
                version_type: "release".into(),
                game_versions: Vec::new(),
                loaders: Vec::new(),
                files: vec![file.clone()],
                dependencies: Vec::new(),
            },
            file,
            as_dependency: replaces.is_none(),
            replaces: replaces.map(str::to_string),
        }
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    /// 確定の前に落ちたときの状態を作り、起動時の復旧（`journal::recover`）で元に戻ることを確かめる。
    /// `placed` が偽なら退避の前（記録だけある）、真なら退避と配置を終えた後で落ちたことにする。
    fn crash_and_recover(placed: bool) {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path());
        paths.ensure_dirs().unwrap();
        let db = Db::open_in_memory().unwrap();
        let inst =
            instance::create(&db, &paths, &NewInstance::vanilla("x", "1.99-test"), 0).unwrap();
        let game = instance::game_dir(&paths, &inst);
        let mods = game.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        let originals: [(&str, &[u8]); 4] = [
            ("a-1.jar", b"a"),
            ("d.jar.disabled", b"d"),
            ("s.jar", b"s"),
            ("c.jar.disabled", b"c"),
        ];
        for (name, body) in originals {
            std::fs::write(mods.join(name), body).unwrap();
        }
        let http = Http::new().unwrap();
        let token = CancellationToken::new();
        let cx = ApplyContext {
            db: &db,
            http: &http,
            paths: &paths,
            game_dir: &game,
            instance_id: &inst.id,
            token: &token,
            cdn: MODRINTH_CDN,
        };
        let plan = InstallPlan {
            files: vec![
                planned("b", "b-1.jar", None),
                planned("a", "a-2.jar", Some("mods/a-1.jar")),
                planned("d", "d-2.jar", Some("mods/d.jar.disabled")),
                planned("s", "s.jar", Some("mods/s.jar")),
            ],
            enable: vec!["mods/c.jar.disabled".into()],
            incompatible_installed: Vec::new(),
        };
        let prepared = prepare(&cx, &plan).unwrap();
        let op = journal::begin(&db, "update", &inst.id, &prepared.targets, 1).unwrap();
        let staging = paths.staging_dir().join(&op.id);
        let backup_dir = staging.join("backup");
        std::fs::create_dir_all(&backup_dir).unwrap();
        if placed {
            for r in &prepared.replaced {
                move_file(&game.join(&r.rel_path), &backup_dir.join(&r.backup_name)).unwrap();
            }
            for e in &prepared.enabling {
                let backup = backup_dir.join(&e.backup_name);
                move_file(&game.join(&e.from_rel), &backup).unwrap();
                std::fs::copy(&backup, game.join(&e.to_rel)).unwrap();
            }
            for p in &prepared.placements {
                std::fs::write(game.join(&p.dest_rel), b"new").unwrap();
            }
        }

        journal::recover(&db, &paths).unwrap();

        let expected: Vec<&str> = {
            let mut names: Vec<&str> = originals.iter().map(|(n, _)| *n).collect();
            names.sort();
            names
        };
        assert_eq!(names_in(&mods), expected, "placed = {placed}");
        for (name, body) in originals {
            assert_eq!(std::fs::read(mods.join(name)).unwrap(), body, "{name}");
        }
        assert!(!staging.exists());
        assert!(journal::list(&db).unwrap().is_empty());
    }

    #[test]
    fn recorded_targets_let_recovery_undo_a_crash_before_the_backup() {
        crash_and_recover(false);
    }

    #[test]
    fn recorded_targets_let_recovery_undo_a_crash_after_placing() {
        crash_and_recover(true);
    }

    fn not_same_device() -> std::io::Error {
        std::io::Error::other("simulated: not on the same device")
    }

    fn parts_in(dir: &Path) -> Vec<String> {
        names_in(dir)
            .into_iter()
            .filter(|n| n.ends_with(".part"))
            .collect()
    }

    #[test]
    fn fallback_copy_appears_under_the_final_name_only_when_complete() {
        let temp = tempfile::tempdir().unwrap();
        let from = temp.path().join("mods").join("a.jar");
        std::fs::create_dir_all(from.parent().unwrap()).unwrap();
        let body: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&from, &body).unwrap();
        let backup_dir = temp.path().join("backup");
        let to = backup_dir.join("0-a.jar");
        let calls = std::cell::Cell::new(0);

        move_file_with(&from, &to, |a, b| {
            calls.set(calls.get() + 1);
            if a == from {
                // 直接の rename は失敗させ、コピーに回す
                return Err(not_same_device());
            }
            // 最終の名前に置く直前：まだ `to` は無く、`.part` は書き終えている
            assert_eq!(b, to);
            assert!(
                !to.exists(),
                "the final name appeared before the copy finished"
            );
            assert!(a.to_string_lossy().ends_with(".part"), "{}", a.display());
            assert_eq!(std::fs::read(a).unwrap(), body);
            std::fs::rename(a, b)
        })
        .unwrap();

        assert_eq!(calls.get(), 2);
        assert_eq!(std::fs::read(&to).unwrap(), body);
        assert!(!from.exists());
        assert!(parts_in(&backup_dir).is_empty());
    }

    #[test]
    fn failed_fallback_leaves_no_partial_file_and_keeps_the_source() {
        let temp = tempfile::tempdir().unwrap();
        let from = temp.path().join("a.jar");
        std::fs::write(&from, b"original").unwrap();
        let backup_dir = temp.path().join("backup");
        let to = backup_dir.join("0-a.jar");

        // コピーの後の rename も失敗する
        let err = move_file_with(&from, &to, |_, _| Err(not_same_device())).unwrap_err();

        assert!(matches!(err, CoreError::Io { .. }), "{err:?}");
        assert_eq!(std::fs::read(&from).unwrap(), b"original");
        assert!(!to.exists());
        assert!(parts_in(&backup_dir).is_empty());
    }

    /// 退避のコピーの途中で落ちた（`staging/<id>/backup/` に `.part` だけが残った）ときと、
    /// 退避を終えて新しいファイルを置いた後に `.part` の残りもあるときに、復旧が元の jar を守る。
    #[test]
    fn recovery_ignores_a_leftover_part_file() {
        for backed_up in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let paths = AppPaths::from_root(temp.path());
            paths.ensure_dirs().unwrap();
            let db = Db::open_in_memory().unwrap();
            let inst =
                instance::create(&db, &paths, &NewInstance::vanilla("x", "1.99-test"), 0).unwrap();
            let game = instance::game_dir(&paths, &inst);
            let mods = game.join("mods");
            std::fs::create_dir_all(&mods).unwrap();
            std::fs::write(mods.join("a.jar"), b"original").unwrap();
            let http = Http::new().unwrap();
            let token = CancellationToken::new();
            let cx = ApplyContext {
                db: &db,
                http: &http,
                paths: &paths,
                game_dir: &game,
                instance_id: &inst.id,
                token: &token,
                cdn: MODRINTH_CDN,
            };
            let plan = InstallPlan {
                files: vec![planned("a", "a.jar", Some("mods/a.jar"))],
                ..InstallPlan::default()
            };
            let prepared = prepare(&cx, &plan).unwrap();
            let op = journal::begin(&db, "update", &inst.id, &prepared.targets, 1).unwrap();
            let backup_dir = paths.staging_dir().join(&op.id).join("backup");
            std::fs::create_dir_all(&backup_dir).unwrap();
            let backup = backup_dir.join(&prepared.replaced[0].backup_name);
            // 書きかけのコピー
            std::fs::write(part_path(&backup), b"orig").unwrap();
            if backed_up {
                std::fs::rename(mods.join("a.jar"), &backup).unwrap();
                std::fs::write(mods.join("a.jar"), b"new").unwrap();
            }

            journal::recover(&db, &paths).unwrap();

            assert_eq!(names_in(&mods), ["a.jar"], "backed_up = {backed_up}");
            assert_eq!(
                std::fs::read(mods.join("a.jar")).unwrap(),
                b"original",
                "backed_up = {backed_up}"
            );
            assert!(!paths.staging_dir().join(&op.id).exists());
            assert!(journal::list(&db).unwrap().is_empty());
        }
    }

    #[test]
    fn recorded_targets_use_real_names_and_safe_backup_names() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path());
        let db = Db::open_in_memory().unwrap();
        let http = Http::new().unwrap();
        let token = CancellationToken::new();
        let cx = ApplyContext {
            db: &db,
            http: &http,
            paths: &paths,
            game_dir: temp.path(),
            instance_id: "i1",
            token: &token,
            cdn: MODRINTH_CDN,
        };
        let plan = InstallPlan {
            files: vec![planned("d", "d-2.jar", Some("mods/d.jar.disabled"))],
            enable: vec!["mods/c.jar.disabled".into()],
            incompatible_installed: Vec::new(),
        };
        let prepared = prepare(&cx, &plan).unwrap();
        let t = |rel: &str, backup: Option<&str>, created| Target {
            rel_path: rel.into(),
            backup: backup.map(str::to_string),
            created,
        };
        assert_eq!(
            prepared.targets,
            [
                t("mods/d.jar.disabled", Some("0-d.jar.disabled"), false),
                t("mods/d-2.jar.disabled", None, true),
                t("mods/c.jar.disabled", Some("e0-c.jar.disabled"), false),
                t("mods/c.jar", None, true),
            ]
        );
        for target in &prepared.targets {
            assert!(journal::is_safe_relative(&target.rel_path));
            if let Some(b) = &target.backup {
                assert!(journal::is_safe_relative(b) && !b.contains('/'));
            }
        }
    }
}
