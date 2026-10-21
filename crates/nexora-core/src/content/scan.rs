//! 中身のフォルダーを走査して、instance_contents と照らし合わせる。

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;

use sha1::{Digest, Sha1};
use sha2::Sha512;
use tokio_util::sync::CancellationToken;

use super::store::{self, Row};
use super::{ContentItem, ContentKind, InstalledAs, Provider};
use crate::db::Db;
use crate::error::CoreError;
use crate::net::hash::hex;

pub(super) const DISABLED_SUFFIX: &str = ".disabled";

/// 走査で行がどれだけ動いたか。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanReport {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
}

pub(super) fn io_error(path: &Path, source: std::io::Error) -> CoreError {
    CoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn check_cancel(cancel: &CancellationToken) -> Result<(), CoreError> {
    if cancel.is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    Ok(())
}

/// ファイルを 64 KiB ずつ 1 回だけ読んで、`(sha1, sha512)` を同時に求める。
pub(super) fn hash_file(path: &Path) -> std::io::Result<(String, String)> {
    let mut file = std::fs::File::open(path)?;
    let mut sha1 = Sha1::new();
    let mut sha512 = Sha512::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha1.update(&buf[..n]);
        sha512.update(&buf[..n]);
    }
    Ok((hex(&sha1.finalize()), hex(&sha512.finalize())))
}

/// 1 ファイル分の読み取りに失敗したら、警告を残して `None` を返す。
/// ほかのプロセスが一時的に開いているだけのこともあるので、走査全体は止めない。
fn skip_unreadable<T>(path: &Path, result: std::io::Result<T>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "skipping a file that could not be read");
            None
        }
    }
}

/// ディスク上の 1 ファイル分。
pub(super) struct DiskFile {
    pub rel_path: String,
    pub enabled: bool,
    pub size: u64,
    pub mtime_ms: i64,
}

/// ファイル名から有効か無効かを決める。種類の拡張子でなければ `None`。
pub(super) fn classify(name: &str, kind: ContentKind) -> Option<bool> {
    let ext = format!(".{}", kind.ext());
    if name.len() > ext.len() && name.ends_with(&ext) {
        return Some(true);
    }
    let disabled = format!("{ext}{DISABLED_SUFFIX}");
    if name.len() > disabled.len() && name.ends_with(&disabled) {
        return Some(false);
    }
    None
}

/// 有効と無効を入れ替えたときのファイル名。
pub(super) fn toggled(rel_path: &str) -> String {
    match rel_path.strip_suffix(DISABLED_SUFFIX) {
        Some(enabled) => enabled.to_string(),
        None => format!("{rel_path}{DISABLED_SUFFIX}"),
    }
}

/// 更新時刻を UNIX 時刻のミリ秒で返す。1970 年より前なら 0。
pub(super) fn mtime_ms(meta: &std::fs::Metadata) -> std::io::Result<i64> {
    Ok(meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64))
}

/// ディスクの一覧。読めずに飛ばしたものも覚えておき、その行を消さないようにする。
#[derive(Default)]
struct DiskListing {
    files: Vec<DiskFile>,
    /// 名前は分かるが読めなかったファイルの `rel_path`。
    unreadable: HashSet<String>,
    /// 名前すら分からないエントリがあった。どの行がディスクに無いかを決められない。
    incomplete: bool,
}

/// `game_dir/<kind.dir()>/` の直下の通常ファイルを集める。ディレクトリが無ければ空。
/// 1 ファイル分の失敗は警告を残して飛ばす。
fn list_disk(
    game_dir: &Path,
    kind: ContentKind,
    cancel: &CancellationToken,
) -> Result<DiskListing, CoreError> {
    let dir = game_dir.join(kind.dir());
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(DiskListing::default()),
        Err(e) => return Err(io_error(&dir, e)),
    };
    let mut listing = DiskListing::default();
    for entry in entries {
        check_cancel(cancel)?;
        let Some(entry) = skip_unreadable(&dir, entry) else {
            listing.incomplete = true;
            continue;
        };
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            tracing::warn!(path = %path.display(), "skipping a file with a non UTF-8 name");
            continue;
        };
        let Some(enabled) = classify(&name, kind) else {
            continue;
        };
        let rel_path = format!("{}/{name}", kind.dir());
        // DirEntry::file_type はリンクをたどらないので、シンボリックリンクはここで落ちる
        let Some(file_type) = skip_unreadable(&path, entry.file_type()) else {
            listing.unreadable.insert(rel_path);
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let Some((size, mtime_ms)) = skip_unreadable(
            &path,
            entry
                .metadata()
                .and_then(|meta| Ok((meta.len(), mtime_ms(&meta)?))),
        ) else {
            listing.unreadable.insert(rel_path);
            continue;
        };
        listing.files.push(DiskFile {
            rel_path,
            enabled,
            size,
            mtime_ms,
        });
    }
    Ok(listing)
}

/// まだ特定されていない行を作る。行の組み立てはここに集める。
pub(super) fn local_row(kind: ContentKind, file: &DiskFile, sha1: String, sha512: String) -> Row {
    let file_name = file
        .rel_path
        .rsplit('/')
        .next()
        .unwrap_or(&file.rel_path)
        .to_string();
    Row {
        item: ContentItem {
            kind,
            rel_path: file.rel_path.clone(),
            file_name,
            enabled: file.enabled,
            size: file.size,
            sha1,
            provider: Provider::Local,
            project_id: None,
            version_id: None,
            pinned: false,
            installed_as: InstalledAs::Explicit,
            name: None,
            version_number: None,
            description: None,
            icon_url: None,
            has_local_icon: false,
            client_side: None,
            server_side: None,
            categories: Vec::new(),
            dependencies: Vec::new(),
            identified: false,
        },
        mtime_ms: file.mtime_ms,
        sha512,
        identified_at: None,
        local_icon_key: None,
    }
}

/// `game_dir/<kind.dir()>/` を走査して、instance_contents をディスクに合わせる。
///
/// 同期で動くので、呼び出し側が `spawn_blocking` で呼ぶ。
/// `rel_path`、`size`、`mtime_ms` が同じ行はハッシュを計算し直さない。
/// サイズか更新時刻が変わった行は、特定の結果を捨てて `local` に戻す。
/// 外で `.disabled` を付け外ししただけ（同じ sha1、size、mtime_ms）なら、特定の結果を保つ。
/// 取り消しはファイルごとに確かめる。
/// 読めないファイル（ほかのプロセスが開いているなど）は飛ばし、その行は次の走査まで手を付けない。
pub fn scan(
    db: &Db,
    game_dir: &Path,
    instance_id: &str,
    kind: ContentKind,
    cancel: &CancellationToken,
) -> Result<ScanReport, CoreError> {
    check_cancel(cancel)?;
    let DiskListing {
        files: disk,
        mut unreadable,
        incomplete,
    } = list_disk(game_dir, kind, cancel)?;
    let mut rows: HashMap<String, Row> = store::rows(db, instance_id, kind)?
        .into_iter()
        .map(|r| (r.item.rel_path.clone(), r))
        .collect();
    let on_disk: HashSet<&str> = disk.iter().map(|f| f.rel_path.as_str()).collect();
    let mut report = ScanReport::default();

    for file in &disk {
        check_cancel(cancel)?;
        let path = game_dir.join(&file.rel_path);
        if let Some(old) = rows.remove(&file.rel_path) {
            if old.item.size == file.size && old.mtime_ms == file.mtime_ms {
                continue;
            }
            // 読めなければ古い行をそのまま残す（rows から外したので最後にも消えない）
            let Some((sha1, sha512)) = skip_unreadable(&path, hash_file(&path)) else {
                continue;
            };
            let mut row = local_row(kind, file, sha1, sha512);
            // 入れた経緯と固定は、中身が変わっても利用者の意図として残す
            row.item.pinned = old.item.pinned;
            row.item.installed_as = old.item.installed_as;
            store::upsert(db, instance_id, &row)?;
            report.changed += 1;
            continue;
        }

        let Some((sha1, sha512)) = skip_unreadable(&path, hash_file(&path)) else {
            // 外で名前を変えただけかもしれないので、相方の行も残す
            unreadable.insert(file.rel_path.clone());
            continue;
        };
        let counterpart = toggled(&file.rel_path);
        let renamed = !on_disk.contains(counterpart.as_str())
            && rows.get(&counterpart).is_some_and(|old| {
                old.item.size == file.size && old.mtime_ms == file.mtime_ms && old.item.sha1 == sha1
            });
        if renamed {
            rows.remove(&counterpart);
            store::rename(db, instance_id, &counterpart, &file.rel_path, file.enabled)?;
            report.changed += 1;
        } else {
            store::upsert(db, instance_id, &local_row(kind, file, sha1, sha512))?;
            report.added += 1;
        }
    }

    // 一覧を取りきれなかったときは、どの行がディスクに無いのか決められないので消さない
    if incomplete {
        return Ok(report);
    }
    // 残った行はディスクに無い。ただし読めずに飛ばしたファイル（とその相方）の行は残す
    for rel_path in rows.keys() {
        if unreadable.contains(rel_path) || unreadable.contains(&toggled(rel_path)) {
            continue;
        }
        store::delete(db, instance_id, rel_path)?;
        report.removed += 1;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Provider;
    use crate::content::store::{self, Row};
    use crate::net::hash::sha1_hex;

    fn db_with_instance() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO instances (id, name, dir_name, mc_version, state, created_at)
                 VALUES ('i1', 'n', 'd', '1.21', 'ready', 0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        db
    }

    fn put(game: &Path, rel: &str, bytes: &[u8]) {
        let path = game.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn run(db: &Db, game: &Path, kind: ContentKind) -> ScanReport {
        scan(db, game, "i1", kind, &CancellationToken::new()).unwrap()
    }

    fn rows(db: &Db, kind: ContentKind) -> Vec<Row> {
        store::rows(db, "i1", kind).unwrap()
    }

    /// 特定済みの行に見せかける。
    fn identify(db: &Db, rel_path: &str) {
        let mut row = rows(db, ContentKind::Mod)
            .into_iter()
            .find(|r| r.item.rel_path == rel_path)
            .unwrap();
        row.item.provider = Provider::Modrinth;
        row.item.project_id = Some("AANobbMI".into());
        row.item.version_id = Some("v1".into());
        row.item.name = Some("Sodium".into());
        row.item.categories = vec!["optimization".into()];
        row.local_icon_key = Some("k".into());
        row.identified_at = Some(456);
        row.item.identified = true;
        row.item.has_local_icon = true;
        store::upsert(db, "i1", &row).unwrap();
    }

    #[test]
    fn hash_file_returns_sha1_and_sha512_of_the_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("f");
        std::fs::write(&path, b"abc").unwrap();
        let (sha1, sha512) = hash_file(&path).unwrap();
        assert_eq!(sha1, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            sha512,
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }

    #[test]
    fn scan_adds_rows_for_files_with_the_kinds_extension() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        put(game, "mods/a.jar", b"aaa");
        put(game, "mods/b.jar.disabled", b"bbbb");
        put(game, "mods/c.txt", b"c");
        put(game, "mods/sub/d.jar", b"d");
        let db = db_with_instance();

        let report = run(&db, game, ContentKind::Mod);

        assert_eq!(
            report,
            ScanReport {
                added: 2,
                changed: 0,
                removed: 0
            }
        );
        let rows = rows(&db, ContentKind::Mod);
        assert_eq!(rows.len(), 2);
        let a = &rows[0];
        assert_eq!(a.item.rel_path, "mods/a.jar");
        assert_eq!(a.item.file_name, "a.jar");
        assert!(a.item.enabled);
        assert_eq!(a.item.size, 3);
        assert_eq!(a.item.sha1, sha1_hex(b"aaa"));
        assert_eq!(a.sha512.len(), 128);
        assert_eq!(a.item.provider, Provider::Local);
        assert!(!a.item.identified);
        assert!(a.mtime_ms > 0);
        let b = &rows[1];
        assert_eq!(b.item.rel_path, "mods/b.jar.disabled");
        assert!(!b.item.enabled);
        assert_eq!(b.item.size, 4);
    }

    #[test]
    fn scan_ignores_a_missing_directory_and_does_not_create_it() {
        let temp = tempfile::tempdir().unwrap();
        let db = db_with_instance();

        let report = run(&db, temp.path(), ContentKind::Mod);

        assert_eq!(report, ScanReport::default());
        assert!(rows(&db, ContentKind::Mod).is_empty());
        assert!(!temp.path().join("mods").exists());
    }

    #[test]
    fn scan_does_not_rehash_unchanged_files() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        put(game, "mods/a.jar", b"aaa");
        let db = db_with_instance();
        run(&db, game, ContentKind::Mod);
        // ハッシュを計算し直したら元の値に戻るので、書き換えた値で見分ける
        let mut row = rows(&db, ContentKind::Mod).remove(0);
        row.item.sha1 = "f".repeat(40);
        store::upsert(&db, "i1", &row).unwrap();

        let report = run(&db, game, ContentKind::Mod);

        assert_eq!(report, ScanReport::default());
        assert_eq!(rows(&db, ContentKind::Mod)[0].item.sha1, "f".repeat(40));
    }

    #[test]
    fn scan_rehashes_and_forgets_identification_when_the_file_changed() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        put(game, "mods/a.jar", b"aaa");
        let db = db_with_instance();
        run(&db, game, ContentKind::Mod);
        identify(&db, "mods/a.jar");
        put(game, "mods/a.jar", b"aaaaaa");

        let report = run(&db, game, ContentKind::Mod);

        assert_eq!(
            report,
            ScanReport {
                added: 0,
                changed: 1,
                removed: 0
            }
        );
        let row = rows(&db, ContentKind::Mod).remove(0);
        assert_eq!(row.item.provider, Provider::Local);
        assert_eq!(row.identified_at, None);
        assert!(!row.item.identified);
        assert_eq!(row.item.project_id, None);
        assert_eq!(row.item.version_id, None);
        assert_eq!(row.item.name, None);
        assert!(row.item.categories.is_empty());
        assert_eq!(row.local_icon_key, None);
        assert_eq!(row.item.size, 6);
        assert_eq!(row.item.sha1, sha1_hex(b"aaaaaa"));
    }

    #[test]
    fn scan_removes_rows_for_files_deleted_outside_the_launcher() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        put(game, "mods/a.jar", b"aaa");
        put(game, "mods/b.jar", b"bbb");
        let db = db_with_instance();
        run(&db, game, ContentKind::Mod);
        std::fs::remove_file(game.join("mods/a.jar")).unwrap();

        let report = run(&db, game, ContentKind::Mod);

        assert_eq!(
            report,
            ScanReport {
                added: 0,
                changed: 0,
                removed: 1
            }
        );
        let paths: Vec<_> = rows(&db, ContentKind::Mod)
            .into_iter()
            .map(|r| r.item.rel_path)
            .collect();
        assert_eq!(paths, ["mods/b.jar"]);
    }

    #[test]
    fn scan_follows_an_outside_rename_to_dot_disabled() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        put(game, "mods/a.jar", b"aaa");
        let db = db_with_instance();
        run(&db, game, ContentKind::Mod);
        identify(&db, "mods/a.jar");
        std::fs::rename(game.join("mods/a.jar"), game.join("mods/a.jar.disabled")).unwrap();

        let report = run(&db, game, ContentKind::Mod);

        assert_eq!(
            report,
            ScanReport {
                added: 0,
                changed: 1,
                removed: 0
            }
        );
        let rows = rows(&db, ContentKind::Mod);
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.item.rel_path, "mods/a.jar.disabled");
        assert!(!row.item.enabled);
        assert_eq!(row.item.provider, Provider::Modrinth);
        assert_eq!(row.item.project_id.as_deref(), Some("AANobbMI"));
        assert_eq!(row.identified_at, Some(456));
        assert_eq!(row.local_icon_key.as_deref(), Some("k"));
    }

    #[test]
    fn scan_stops_when_cancelled() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        put(game, "mods/a.jar", b"aaa");
        let db = db_with_instance();
        let cancel = CancellationToken::new();
        cancel.cancel();

        let err = scan(&db, game, "i1", ContentKind::Mod, &cancel).unwrap_err();

        assert!(matches!(err, CoreError::Cancelled));
        assert!(rows(&db, ContentKind::Mod).is_empty());
    }

    #[test]
    fn skip_unreadable_keeps_ok_and_drops_err() {
        let path = Path::new("mods/a.jar");
        assert_eq!(skip_unreadable(path, Ok(3)), Some(3));
        let err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        assert_eq!(skip_unreadable::<i32>(path, Err(err)), None);
    }

    /// ほかのプロセスが共有なしで開いているファイルに見せかける（Windows だけで再現できる）。
    #[cfg(windows)]
    fn lock(path: &Path) -> std::fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn scan_keeps_the_old_row_when_a_changed_file_cannot_be_read() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        put(game, "mods/a.jar", b"aaa");
        put(game, "mods/b.jar", b"bbb");
        let db = db_with_instance();
        run(&db, game, ContentKind::Mod);
        identify(&db, "mods/a.jar");
        let before = rows(&db, ContentKind::Mod);
        put(game, "mods/a.jar", b"aaaaaa");
        let _guard = lock(&game.join("mods/a.jar"));

        let report = run(&db, game, ContentKind::Mod);

        assert_eq!(report, ScanReport::default());
        assert_eq!(rows(&db, ContentKind::Mod), before);
    }

    #[cfg(windows)]
    #[test]
    fn scan_skips_a_new_file_that_cannot_be_read_and_adds_it_later() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        put(game, "mods/a.jar", b"aaa");
        put(game, "mods/b.jar", b"bbb");
        let db = db_with_instance();
        let guard = lock(&game.join("mods/a.jar"));

        let report = run(&db, game, ContentKind::Mod);

        assert_eq!(report.added, 1);
        let paths: Vec<_> = rows(&db, ContentKind::Mod)
            .into_iter()
            .map(|r| r.item.rel_path)
            .collect();
        assert_eq!(paths, ["mods/b.jar"]);

        drop(guard);
        assert_eq!(run(&db, game, ContentKind::Mod).added, 1);
        assert_eq!(rows(&db, ContentKind::Mod).len(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn scan_keeps_the_counterpart_row_when_a_renamed_file_cannot_be_read() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        put(game, "mods/a.jar", b"aaa");
        let db = db_with_instance();
        run(&db, game, ContentKind::Mod);
        identify(&db, "mods/a.jar");
        let before = rows(&db, ContentKind::Mod);
        std::fs::rename(game.join("mods/a.jar"), game.join("mods/a.jar.disabled")).unwrap();
        let guard = lock(&game.join("mods/a.jar.disabled"));

        let report = run(&db, game, ContentKind::Mod);

        assert_eq!(report, ScanReport::default());
        assert_eq!(rows(&db, ContentKind::Mod), before);

        // 読めるようになれば、外での名前の変更として特定の結果ごと引き継ぐ
        drop(guard);
        let report = run(&db, game, ContentKind::Mod);
        assert_eq!(report.changed, 1);
        let row = rows(&db, ContentKind::Mod).remove(0);
        assert_eq!(row.item.rel_path, "mods/a.jar.disabled");
        assert_eq!(row.item.provider, Provider::Modrinth);
    }

    #[test]
    fn scan_covers_resource_packs_and_shaders_with_zip() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path();
        put(game, "resourcepacks/p.zip", b"p");
        put(game, "shaderpacks/s.zip.disabled", b"s");
        put(game, "shaderpacks/t.jar", b"t");
        let db = db_with_instance();

        assert_eq!(run(&db, game, ContentKind::ResourcePack).added, 1);
        assert_eq!(run(&db, game, ContentKind::Shader).added, 1);

        let packs = rows(&db, ContentKind::ResourcePack);
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].item.rel_path, "resourcepacks/p.zip");
        assert_eq!(packs[0].item.kind, ContentKind::ResourcePack);
        assert!(packs[0].item.enabled);
        let shaders = rows(&db, ContentKind::Shader);
        assert_eq!(shaders.len(), 1);
        assert_eq!(shaders[0].item.rel_path, "shaderpacks/s.zip.disabled");
        assert_eq!(shaders[0].item.kind, ContentKind::Shader);
        assert!(!shaders[0].item.enabled);
        assert!(rows(&db, ContentKind::Mod).is_empty());
    }
}
