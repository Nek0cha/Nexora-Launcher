//! 中身のファイルの切り替え、削除、追加。

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::scan::{
    DISABLED_SUFFIX, DiskFile, classify, hash_file, io_error, local_row, mtime_ms, toggled,
};
use super::store;
use super::{ContentItem, ContentKind};
use crate::db::Db;
use crate::error::CoreError;

/// 追加を受け付けなかった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RejectReason {
    NotAFile,
    WrongExtension,
    Unreadable,
}

/// 受け付けなかった 1 件。`name` はコピー元のファイル名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Rejection {
    pub name: String,
    pub reason: RejectReason,
}

/// 追加の結果。`conflicts` は同じ名前がすでにあって入れなかったファイル名。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddReport {
    pub added: Vec<ContentItem>,
    pub conflicts: Vec<String>,
    pub rejected: Vec<Rejection>,
}

/// `rel_path` を確かめて、ファイル名の部分を返す。
///
/// `<kind.dir()>/<name>` の形で、`name` が 1 つの部分からなり、種類の拡張子
/// （`.disabled` 付きを含む）を持つものに限る。保存先の外や別の種類の保存先を指すものは拒む。
pub(super) fn content_name(kind: ContentKind, rel_path: &str) -> Result<&str, CoreError> {
    let invalid = || CoreError::InvalidInstanceSetting {
        field: "rel_path".into(),
    };
    let name = rel_path
        .strip_prefix(kind.dir())
        .and_then(|rest| rest.strip_prefix('/'))
        .ok_or_else(invalid)?;
    // `:` は Windows のドライブ指定と代替データストリームを防ぐ
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':']) {
        return Err(invalid());
    }
    if classify(name, kind).is_none() {
        return Err(invalid());
    }
    Ok(name)
}

/// ファイル名の末尾の `.disabled` を付け外しして、有効か無効かを切り替える。
///
/// すでにその状態なら何もしない。変更先の名前がすでにあれば `ContentExists` で、何も変えない。
pub fn set_enabled(
    db: &Db,
    game_dir: &Path,
    instance_id: &str,
    kind: ContentKind,
    rel_path: &str,
    enabled: bool,
) -> Result<(), CoreError> {
    let name = content_name(kind, rel_path)?;
    if classify(name, kind) == Some(enabled) {
        return Ok(());
    }
    let target = toggled(rel_path);
    let from = game_dir.join(rel_path);
    let to = game_dir.join(&target);
    if to.symlink_metadata().is_ok() {
        return Err(CoreError::ContentExists { path: target });
    }
    std::fs::rename(&from, &to).map_err(|e| io_error(&from, e))?;
    store::rename(db, instance_id, rel_path, &target, enabled)
}

/// ファイルを消してから行を消す。ファイルがすでに無ければ行だけ消す。
pub fn remove(
    db: &Db,
    game_dir: &Path,
    instance_id: &str,
    kind: ContentKind,
    rel_path: &str,
) -> Result<(), CoreError> {
    content_name(kind, rel_path)?;
    remove_if_exists(&game_dir.join(rel_path))?;
    store::delete(db, instance_id, rel_path)
}

/// ファイルを種類の保存先にコピーして、`local` の行を作る。
///
/// ファイルごとに独立に扱い、受け付けなかったものと名前がぶつかったものは結果に入れて続ける。
/// `overwrite` なら、同じ名前の有効側と無効側の両方を置き換える。
/// 保存先の書き込みや DB の失敗は `Err` で止める。
pub fn add_files(
    db: &Db,
    game_dir: &Path,
    instance_id: &str,
    kind: ContentKind,
    sources: &[PathBuf],
    overwrite: bool,
) -> Result<AddReport, CoreError> {
    let dir = game_dir.join(kind.dir());
    let mut report = AddReport::default();
    for source in sources {
        match add_one(db, &dir, instance_id, kind, source, overwrite)? {
            Outcome::Added(item) => report.added.push(*item),
            Outcome::Conflict(name) => report.conflicts.push(name),
            Outcome::Rejected(rejection) => report.rejected.push(rejection),
        }
    }
    Ok(report)
}

/// 1 ファイル分の結果。
enum Outcome {
    Added(Box<ContentItem>),
    Conflict(String),
    Rejected(Rejection),
}

fn add_one(
    db: &Db,
    dir: &Path,
    instance_id: &str,
    kind: ContentKind,
    source: &Path,
    overwrite: bool,
) -> Result<Outcome, CoreError> {
    let shown = source.file_name().map_or_else(
        || source.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let reject = |reason| {
        Ok(Outcome::Rejected(Rejection {
            name: shown.clone(),
            reason,
        }))
    };
    let Ok(meta) = std::fs::metadata(source) else {
        return reject(RejectReason::Unreadable);
    };
    if !meta.is_file() {
        return reject(RejectReason::NotAFile);
    }
    // UTF-8 でない名前は拡張子を確かめられないので、同じ扱いにする
    let Some(name) = source.file_name().and_then(|n| n.to_str()) else {
        return reject(RejectReason::WrongExtension);
    };
    if classify(name, kind) != Some(true) {
        return reject(RejectReason::WrongExtension);
    }

    let enabled_path = dir.join(name);
    let disabled_name = format!("{name}{DISABLED_SUFFIX}");
    let disabled_path = dir.join(&disabled_name);
    let exists = |p: &Path| p.symlink_metadata().is_ok();
    if !overwrite && (exists(&enabled_path) || exists(&disabled_path)) {
        return Ok(Outcome::Conflict(name.to_string()));
    }

    std::fs::create_dir_all(dir).map_err(|e| io_error(dir, e))?;
    // 種類の拡張子でも `.disabled` でもない名前にして、走査に拾われないようにする
    let temp = dir.join(format!(".nexora-{}.tmp", uuid::Uuid::new_v4().simple()));
    match copy_to(source, &temp) {
        Ok(()) => {}
        Err(CopyError::Read(e)) => {
            tracing::warn!(path = %source.display(), error = %e, "could not read a file to add");
            let _ = std::fs::remove_file(&temp);
            return reject(RejectReason::Unreadable);
        }
        Err(CopyError::Write(e)) => {
            let _ = std::fs::remove_file(&temp);
            return Err(io_error(&temp, e));
        }
    }
    // 有効側があれば rename が置き換える。置けてから無効側を消すので、失敗しても無効側は残る
    if let Err(e) = std::fs::rename(&temp, &enabled_path) {
        let _ = std::fs::remove_file(&temp);
        return Err(io_error(&enabled_path, e));
    }
    if overwrite {
        remove_if_exists(&disabled_path)?;
    }

    let prefix = kind.dir();
    if overwrite {
        store::delete(db, instance_id, &format!("{prefix}/{disabled_name}"))?;
    }
    let meta = std::fs::metadata(&enabled_path).map_err(|e| io_error(&enabled_path, e))?;
    let file = DiskFile {
        rel_path: format!("{prefix}/{name}"),
        enabled: true,
        size: meta.len(),
        mtime_ms: mtime_ms(&meta).map_err(|e| io_error(&enabled_path, e))?,
    };
    let (sha1, sha512) = hash_file(&enabled_path).map_err(|e| io_error(&enabled_path, e))?;
    let row = local_row(kind, &file, sha1, sha512);
    store::upsert(db, instance_id, &row)?;
    Ok(Outcome::Added(Box::new(row.item)))
}

enum CopyError {
    Read(std::io::Error),
    Write(std::io::Error),
}

/// `source` を新しい `dest` にコピーする。読み込みと書き込みのどちらで失敗したかを分ける。
fn copy_to(source: &Path, dest: &Path) -> Result<(), CopyError> {
    let mut input = File::open(source).map_err(CopyError::Read)?;
    let mut output = File::create_new(dest).map_err(CopyError::Write)?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = input.read(&mut buf).map_err(CopyError::Read)?;
        if n == 0 {
            return Ok(());
        }
        output.write_all(&buf[..n]).map_err(CopyError::Write)?;
    }
}

fn remove_if_exists(path: &Path) -> Result<(), CoreError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_error(path, e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::store::{self, Row};
    use crate::content::{InstalledAs, Provider};
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

    fn put(dir: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn rows(db: &Db, kind: ContentKind) -> Vec<Row> {
        store::rows(db, "i1", kind).unwrap()
    }

    fn paths(db: &Db, kind: ContentKind) -> Vec<String> {
        rows(db, kind)
            .into_iter()
            .map(|r| r.item.rel_path)
            .collect()
    }

    /// `mods/a.jar` をディスクと DB の両方に用意する。
    fn setup_a() -> (tempfile::TempDir, Db) {
        let temp = tempfile::tempdir().unwrap();
        let db = db_with_instance();
        let source = put(temp.path(), "src/a.jar", b"aaa");
        let game = temp.path().join("game");
        add_files(&db, &game, "i1", ContentKind::Mod, &[source], false).unwrap();
        (temp, db)
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn set_enabled_renames_the_file_and_the_row() {
        let (temp, db) = setup_a();
        let game = temp.path().join("game");

        set_enabled(&db, &game, "i1", ContentKind::Mod, "mods/a.jar", false).unwrap();

        assert_eq!(names_in(&game.join("mods")), ["a.jar.disabled"]);
        let row = rows(&db, ContentKind::Mod).remove(0);
        assert_eq!(row.item.rel_path, "mods/a.jar.disabled");
        assert_eq!(row.item.file_name, "a.jar.disabled");
        assert!(!row.item.enabled);

        set_enabled(
            &db,
            &game,
            "i1",
            ContentKind::Mod,
            "mods/a.jar.disabled",
            true,
        )
        .unwrap();

        assert_eq!(names_in(&game.join("mods")), ["a.jar"]);
        let row = rows(&db, ContentKind::Mod).remove(0);
        assert_eq!(row.item.rel_path, "mods/a.jar");
        assert!(row.item.enabled);
    }

    #[test]
    fn set_enabled_is_a_no_op_when_already_in_that_state() {
        let (temp, db) = setup_a();
        let game = temp.path().join("game");
        let before = rows(&db, ContentKind::Mod);

        set_enabled(&db, &game, "i1", ContentKind::Mod, "mods/a.jar", true).unwrap();

        assert_eq!(names_in(&game.join("mods")), ["a.jar"]);
        assert_eq!(rows(&db, ContentKind::Mod), before);
    }

    #[test]
    fn set_enabled_refuses_when_the_target_name_exists_and_changes_nothing() {
        let (temp, db) = setup_a();
        let game = temp.path().join("game");
        put(&game, "mods/a.jar.disabled", b"other");
        let before = rows(&db, ContentKind::Mod);

        let err = set_enabled(&db, &game, "i1", ContentKind::Mod, "mods/a.jar", false).unwrap_err();

        match err {
            CoreError::ContentExists { path } => assert_eq!(path, "mods/a.jar.disabled"),
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(std::fs::read(game.join("mods/a.jar")).unwrap(), b"aaa");
        assert_eq!(
            std::fs::read(game.join("mods/a.jar.disabled")).unwrap(),
            b"other"
        );
        assert_eq!(rows(&db, ContentKind::Mod), before);
    }

    #[test]
    fn set_enabled_rejects_paths_outside_the_content_folder() {
        let (temp, db) = setup_a();
        let game = temp.path().join("game");
        put(&game, "resourcepacks/a.zip", b"r");
        put(temp.path(), "x.jar", b"x");

        for rel in [
            "../x.jar",
            "mods/../x.jar",
            "resourcepacks/a.zip",
            "mods/",
            "mods",
            "mods/sub/a.jar",
            "mods\\a.jar",
            "mods/..",
            "/mods/a.jar",
        ] {
            let err = set_enabled(&db, &game, "i1", ContentKind::Mod, rel, false).unwrap_err();
            match err {
                CoreError::InvalidInstanceSetting { field } => assert_eq!(field, "rel_path"),
                other => panic!("{rel}: unexpected error: {other:?}"),
            }
        }
        assert_eq!(names_in(&game.join("mods")), ["a.jar"]);
        assert_eq!(names_in(&game.join("resourcepacks")), ["a.zip"]);
        assert!(temp.path().join("x.jar").exists());
    }

    #[test]
    fn remove_deletes_the_file_and_the_row() {
        let (temp, db) = setup_a();
        let game = temp.path().join("game");

        remove(&db, &game, "i1", ContentKind::Mod, "mods/a.jar").unwrap();

        assert!(names_in(&game.join("mods")).is_empty());
        assert!(rows(&db, ContentKind::Mod).is_empty());
    }

    #[test]
    fn remove_tolerates_a_missing_file() {
        let (temp, db) = setup_a();
        let game = temp.path().join("game");
        std::fs::remove_file(game.join("mods/a.jar")).unwrap();

        remove(&db, &game, "i1", ContentKind::Mod, "mods/a.jar").unwrap();

        assert!(rows(&db, ContentKind::Mod).is_empty());
    }

    #[test]
    fn remove_rejects_paths_outside_the_content_folder() {
        let (temp, db) = setup_a();
        let game = temp.path().join("game");
        put(temp.path(), "x.jar", b"x");

        let err = remove(&db, &game, "i1", ContentKind::Mod, "mods/../../x.jar").unwrap_err();

        assert!(matches!(err, CoreError::InvalidInstanceSetting { .. }));
        assert!(temp.path().join("x.jar").exists());
        assert_eq!(rows(&db, ContentKind::Mod).len(), 1);
    }

    #[test]
    fn add_files_copies_and_records_a_local_row() {
        let temp = tempfile::tempdir().unwrap();
        let db = db_with_instance();
        let source = put(temp.path(), "src/sodium.jar", b"jar bytes");
        let game = temp.path().join("game");

        let report = add_files(
            &db,
            &game,
            "i1",
            ContentKind::Mod,
            std::slice::from_ref(&source),
            false,
        )
        .unwrap();

        assert!(report.conflicts.is_empty());
        assert!(report.rejected.is_empty());
        assert_eq!(report.added.len(), 1);
        assert_eq!(
            std::fs::read(game.join("mods/sodium.jar")).unwrap(),
            b"jar bytes"
        );
        // コピー元は残す
        assert!(source.exists());
        let row = rows(&db, ContentKind::Mod).remove(0);
        assert_eq!(report.added[0], row.item);
        assert_eq!(row.item.rel_path, "mods/sodium.jar");
        assert_eq!(row.item.file_name, "sodium.jar");
        assert!(row.item.enabled);
        assert_eq!(row.item.size, 9);
        assert_eq!(row.item.sha1, sha1_hex(b"jar bytes"));
        assert_eq!(row.sha512.len(), 128);
        assert!(row.mtime_ms > 0);
        assert_eq!(row.item.provider, Provider::Local);
        assert_eq!(row.item.installed_as, InstalledAs::Explicit);
        assert_eq!(row.identified_at, None);
        assert!(!row.item.identified);
    }

    #[test]
    fn add_files_does_not_change_rows_on_a_rescan() {
        let (temp, db) = setup_a();
        let game = temp.path().join("game");
        let before = rows(&db, ContentKind::Mod);

        let report = crate::content::scan::scan(
            &db,
            &game,
            "i1",
            ContentKind::Mod,
            &tokio_util::sync::CancellationToken::new(),
        )
        .unwrap();

        assert_eq!(report, crate::content::scan::ScanReport::default());
        assert_eq!(rows(&db, ContentKind::Mod), before);
    }

    #[test]
    fn add_files_rejects_a_wrong_extension_a_directory_and_a_missing_file_but_adds_the_rest() {
        let temp = tempfile::tempdir().unwrap();
        let db = db_with_instance();
        let good = put(temp.path(), "src/good.jar", b"g");
        let wrong = put(temp.path(), "src/pack.zip", b"z");
        let disabled = put(temp.path(), "src/old.jar.disabled", b"o");
        let dir = temp.path().join("src/folder.jar");
        std::fs::create_dir_all(&dir).unwrap();
        let missing = temp.path().join("src/missing.jar");
        let game = temp.path().join("game");

        let report = add_files(
            &db,
            &game,
            "i1",
            ContentKind::Mod,
            &[wrong, dir, good, missing, disabled],
            false,
        )
        .unwrap();

        assert_eq!(
            report.rejected,
            [
                Rejection {
                    name: "pack.zip".into(),
                    reason: RejectReason::WrongExtension
                },
                Rejection {
                    name: "folder.jar".into(),
                    reason: RejectReason::NotAFile
                },
                Rejection {
                    name: "missing.jar".into(),
                    reason: RejectReason::Unreadable
                },
                Rejection {
                    name: "old.jar.disabled".into(),
                    reason: RejectReason::WrongExtension
                },
            ]
        );
        assert!(report.conflicts.is_empty());
        assert_eq!(report.added.len(), 1);
        assert_eq!(names_in(&game.join("mods")), ["good.jar"]);
        assert_eq!(paths(&db, ContentKind::Mod), ["mods/good.jar"]);
    }

    #[test]
    fn add_files_reports_a_name_conflict_without_overwriting() {
        for existing in ["a.jar", "a.jar.disabled"] {
            let temp = tempfile::tempdir().unwrap();
            let db = db_with_instance();
            let game = temp.path().join("game");
            put(&game, &format!("mods/{existing}"), b"old");
            let source = put(temp.path(), "src/a.jar", b"new");

            let report = add_files(&db, &game, "i1", ContentKind::Mod, &[source], false).unwrap();

            assert_eq!(report.conflicts, ["a.jar"], "{existing}");
            assert!(report.added.is_empty(), "{existing}");
            assert!(report.rejected.is_empty(), "{existing}");
            assert_eq!(names_in(&game.join("mods")), [existing]);
            assert_eq!(
                std::fs::read(game.join("mods").join(existing)).unwrap(),
                b"old"
            );
            assert!(rows(&db, ContentKind::Mod).is_empty(), "{existing}");
        }
    }

    #[test]
    fn add_files_overwrites_both_variants_when_asked() {
        let (temp, db) = setup_a();
        let game = temp.path().join("game");
        put(&game, "mods/a.jar.disabled", b"disabled");
        crate::content::scan::scan(
            &db,
            &game,
            "i1",
            ContentKind::Mod,
            &tokio_util::sync::CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(
            paths(&db, ContentKind::Mod),
            ["mods/a.jar", "mods/a.jar.disabled"]
        );
        let source = put(temp.path(), "src2/a.jar", b"replacement");

        let report = add_files(&db, &game, "i1", ContentKind::Mod, &[source], true).unwrap();

        assert!(report.conflicts.is_empty());
        assert_eq!(report.added.len(), 1);
        assert_eq!(names_in(&game.join("mods")), ["a.jar"]);
        assert_eq!(
            std::fs::read(game.join("mods/a.jar")).unwrap(),
            b"replacement"
        );
        let rows = rows(&db, ContentKind::Mod);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].item.rel_path, "mods/a.jar");
        assert_eq!(rows[0].item.sha1, sha1_hex(b"replacement"));
        assert_eq!(rows[0].item.size, 11);
    }

    #[test]
    fn add_files_keeps_the_disabled_copy_when_the_overwrite_cannot_be_placed() {
        let temp = tempfile::tempdir().unwrap();
        let db = db_with_instance();
        let game = temp.path().join("game");
        put(&game, "mods/a.jar.disabled", b"disabled");
        // 有効側の名前にディレクトリがあると、rename は必ず失敗する
        std::fs::create_dir_all(game.join("mods/a.jar/inner")).unwrap();
        let source = put(temp.path(), "src/a.jar", b"replacement");

        let result = add_files(&db, &game, "i1", ContentKind::Mod, &[source], true);

        assert!(result.is_err());
        assert_eq!(
            std::fs::read(game.join("mods/a.jar.disabled")).unwrap(),
            b"disabled"
        );
        assert_eq!(names_in(&game.join("mods")), ["a.jar", "a.jar.disabled"]);
    }

    #[test]
    fn add_files_leaves_no_temp_file_behind() {
        let temp = tempfile::tempdir().unwrap();
        let db = db_with_instance();
        let game = temp.path().join("game");
        let a = put(temp.path(), "src/a.zip", b"a");
        let b = put(temp.path(), "src/b.zip", b"b");

        let report =
            add_files(&db, &game, "i1", ContentKind::ResourcePack, &[a, b], false).unwrap();

        assert_eq!(report.added.len(), 2);
        assert_eq!(names_in(&game.join("resourcepacks")), ["a.zip", "b.zip"]);
        assert_eq!(
            paths(&db, ContentKind::ResourcePack),
            ["resourcepacks/a.zip", "resourcepacks/b.zip"]
        );
    }
}
