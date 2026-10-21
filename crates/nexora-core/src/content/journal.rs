//! 複数のファイルと DB をまたぐ操作の記録（spec 0 の 3.7 節）と、起動時の復旧。
//! 手順：`begin`（prepared）→ `staging/<id>/` に用意と退避 → 移動と DB の更新と
//! `mark_committed` を 1 つのトランザクションで → `staging/<id>/` を消して `finish`。
// 記録を書く側（begin、mark_committed、add_backup、prune_backups）は導入と更新のタスクで使う
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::CoreError;
use crate::instance;
use crate::paths::AppPaths;

/// 操作の状態。DB には `prepared`、`committed` で持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpState {
    Prepared,
    Committed,
}

impl OpState {
    fn as_str(self) -> &'static str {
        match self {
            OpState::Prepared => "prepared",
            OpState::Committed => "committed",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "prepared" => Some(OpState::Prepared),
            "committed" => Some(OpState::Committed),
            _ => None,
        }
    }
}

/// 変更の対象 1 件。`rel_path` はゲームディレクトリからの相対（`mods/a.jar` など）。
/// `backup` は `staging/<id>/backup/` からの相対名、`created` は新規に置くファイルかどうか。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Target {
    pub rel_path: String,
    pub backup: Option<String>,
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Operation {
    pub id: String,
    pub kind: String,
    pub instance_id: String,
    pub state: OpState,
    pub targets: Vec<Target>,
    pub created_at: i64,
}

fn encode_targets(targets: &[Target]) -> Result<String, CoreError> {
    serde_json::to_string(targets).map_err(|e| CoreError::Unexpected {
        message: e.to_string(),
    })
}

/// 操作を prepared として記録する。ファイルに触る前に呼ぶ。
pub(crate) fn begin(
    db: &Db,
    kind: &str,
    instance_id: &str,
    targets: &[Target],
    now_ms: i64,
) -> Result<Operation, CoreError> {
    let op = Operation {
        id: uuid::Uuid::new_v4().simple().to_string(),
        kind: kind.to_string(),
        instance_id: instance_id.to_string(),
        state: OpState::Prepared,
        targets: targets.to_vec(),
        created_at: now_ms,
    };
    let encoded = encode_targets(targets)?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO operations (id, kind, instance_id, state, targets, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                op.id,
                op.kind,
                op.instance_id,
                op.state.as_str(),
                encoded,
                op.created_at
            ],
        )?;
        Ok(())
    })?;
    Ok(op)
}

/// 操作を committed にする。ファイルの移動に続く DB の更新と同じトランザクションの中で呼ぶ。
pub(crate) fn mark_committed(conn: &Connection, op_id: &str) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE operations SET state = ?2 WHERE id = ?1",
        params![op_id, OpState::Committed.as_str()],
    )?;
    Ok(())
}

/// 操作の記録を消す。`staging/<id>/` を消した後に呼ぶ。
pub(crate) fn finish(db: &Db, op_id: &str) -> Result<(), CoreError> {
    db.with_conn(|conn| {
        conn.execute("DELETE FROM operations WHERE id = ?1", [op_id])?;
        Ok(())
    })
}

/// 残っている操作（古い順）。状態か対象の一覧が読めない行は飛ばして警告に残す
/// （行と staging は消さずに置き、退避したファイルを失わないようにする）。
pub(crate) fn list(db: &Db) -> Result<Vec<Operation>, CoreError> {
    let raw = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, kind, instance_id, state, targets, created_at
             FROM operations ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })?;
    let mut out = Vec::new();
    for (id, kind, instance_id, state, targets, created_at) in raw {
        let state = OpState::parse(&state);
        let targets = serde_json::from_str::<Vec<Target>>(&targets).ok();
        match (state, targets) {
            (Some(state), Some(targets)) => out.push(Operation {
                id,
                kind,
                instance_id,
                state,
                targets,
                created_at,
            }),
            _ => tracing::warn!(op_id = %id, "skipping an unreadable operation record"),
        }
    }
    Ok(out)
}

/// バックアップを記録する。`path` はファイルの置き場所。
pub(crate) fn add_backup(
    db: &Db,
    instance_id: &str,
    reason: &str,
    path: &str,
    size: u64,
    now_ms: i64,
) -> Result<(), CoreError> {
    let size = i64::try_from(size).unwrap_or(i64::MAX);
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO backups (instance_id, created_at, reason, path, size)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![instance_id, now_ms, reason, path, size],
        )?;
        Ok(())
    })
}

/// インスタンスと理由ごとに、新しい `keep` 件を残して記録を消し、消した `path` を返す。
/// ファイルは消さない（呼び出し側が返った `path` を消す）。
pub(crate) fn prune_backups(
    db: &Db,
    instance_id: &str,
    reason: &str,
    keep: usize,
) -> Result<Vec<String>, CoreError> {
    db.with_conn(|conn| {
        let tx = conn.transaction()?;
        let old: Vec<(i64, String)> = {
            let mut stmt = tx.prepare(
                "SELECT id, path FROM backups WHERE instance_id = ?1 AND reason = ?2
                 ORDER BY created_at DESC, id DESC",
            )?;
            let rows =
                stmt.query_map(params![instance_id, reason], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.skip(keep).collect::<Result<_, _>>()?
        };
        for (id, _) in &old {
            tx.execute("DELETE FROM backups WHERE id = ?1", [id])?;
        }
        tx.commit()?;
        Ok(old.into_iter().map(|(_, path)| path).collect())
    })
}

fn io_err(path: &Path, source: std::io::Error) -> CoreError {
    CoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// 無ければ何もしない削除。
fn remove_file_if_exists(path: &Path) -> Result<(), CoreError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(path, e)),
    }
}

fn remove_dir_if_exists(path: &Path) -> Result<(), CoreError> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(path, e)),
    }
}

/// 退避したファイルを元の場所に戻す。移動できなければ（別のドライブなど）コピーする。
fn restore_file(from: &Path, to: &Path) -> Result<(), CoreError> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).map_err(|e| io_err(to, e))?;
    Ok(())
}

/// prepared の操作のファイルを元に戻す。退避が無い対象は、退避の前に止まったものとして触らない。
fn roll_back(game_dir: &Path, staging: &Path, op: &Operation) -> Result<(), CoreError> {
    let backup_dir = staging.join("backup");
    let mut first_err = None;
    for t in &op.targets {
        let dest = game_dir.join(&t.rel_path);
        let result = match &t.backup {
            Some(name) => {
                let from = backup_dir.join(name);
                if from.is_file() {
                    restore_file(&from, &dest)
                } else if t.created {
                    remove_file_if_exists(&dest)
                } else {
                    Ok(())
                }
            }
            None if t.created => remove_file_if_exists(&dest),
            None => Ok(()),
        };
        if let Err(err) = result {
            tracing::warn!(op_id = %op.id, rel_path = %t.rel_path, code = err.code(), "could not roll back a file");
            first_err.get_or_insert(err);
        }
    }
    first_err.map_or(Ok(()), Err)
}

/// 1 件の操作を復旧する。戻しきれなければ記録と staging を残し、次の起動でやり直す。
fn recover_one(db: &Db, paths: &AppPaths, op: &Operation) -> Result<(), CoreError> {
    let staging: PathBuf = paths.staging_dir().join(&op.id);
    if op.state == OpState::Prepared {
        match instance::get(db, &op.instance_id) {
            Ok(inst) => roll_back(&instance::game_dir(paths, &inst), &staging, op)?,
            // インスタンスが消えていれば戻す先が無い。staging と記録だけ片付ける
            Err(CoreError::InstanceNotFound { .. }) => {}
            Err(err) => return Err(err),
        }
    }
    remove_dir_if_exists(&staging)?;
    finish(db, &op.id)
}

/// 起動時の復旧。prepared は退避を戻し、committed は staging を消すだけにする。
/// 1 件の失敗で残りを止めず、警告に残す。
pub(crate) fn recover(db: &Db, paths: &AppPaths) -> Result<(), CoreError> {
    for op in list(db)? {
        if let Err(err) = recover_one(db, paths, &op) {
            tracing::warn!(op_id = %op.id, kind = %op.kind, code = err.code(), "could not recover an operation");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::{self, NewInstance};
    use crate::paths::AppPaths;

    struct Fixture {
        _temp: tempfile::TempDir,
        db: Db,
        paths: AppPaths,
        id: String,
        game_dir: std::path::PathBuf,
    }

    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path());
        paths.ensure_dirs().unwrap();
        let db = Db::open_in_memory().unwrap();
        let inst =
            instance::create(&db, &paths, &NewInstance::vanilla("x", "1.99-test"), 0).unwrap();
        let game_dir = instance::game_dir(&paths, &inst);
        std::fs::create_dir_all(game_dir.join("mods")).unwrap();
        Fixture {
            _temp: temp,
            db,
            paths,
            id: inst.id,
            game_dir,
        }
    }

    fn target(rel_path: &str, backup: Option<&str>, created: bool) -> Target {
        Target {
            rel_path: rel_path.to_string(),
            backup: backup.map(str::to_string),
            created,
        }
    }

    #[test]
    fn begin_then_list_returns_prepared_operation() {
        let f = fixture();
        let targets = vec![
            target("mods/a.jar", Some("a.jar"), false),
            target("mods/b.jar", None, true),
        ];
        let op = begin(&f.db, "install", &f.id, &targets, 42).unwrap();
        assert_eq!(op.state, OpState::Prepared);
        let listed = list(&f.db).unwrap();
        assert_eq!(listed, vec![op.clone()]);
        assert_eq!(listed[0].targets, targets);
        assert_eq!(listed[0].kind, "install");
        assert_eq!(listed[0].instance_id, f.id);
        assert_eq!(listed[0].created_at, 42);
    }

    #[test]
    fn finish_removes_the_record() {
        let f = fixture();
        let op = begin(&f.db, "install", &f.id, &[], 1).unwrap();
        finish(&f.db, &op.id).unwrap();
        assert!(list(&f.db).unwrap().is_empty());
    }

    #[test]
    fn prune_backups_keeps_newest_n_and_returns_removed_paths() {
        let f = fixture();
        for i in 1..=6 {
            add_backup(&f.db, &f.id, "pre-update", &format!("b{i}.zip"), 10, i).unwrap();
        }
        // 別の理由のものは数に入れず、消さない
        add_backup(&f.db, &f.id, "manual", "m.zip", 10, 0).unwrap();
        let removed = prune_backups(&f.db, &f.id, "pre-update", 5).unwrap();
        assert_eq!(removed, vec!["b1.zip".to_string()]);
        let remaining: Vec<String> =
            f.db.with_conn(|conn| {
                let mut stmt = conn.prepare("SELECT path FROM backups ORDER BY path")?;
                let rows = stmt.query_map([], |r| r.get(0))?;
                Ok(rows.collect::<Result<_, _>>()?)
            })
            .unwrap();
        assert_eq!(
            remaining,
            ["b2.zip", "b3.zip", "b4.zip", "b5.zip", "b6.zip", "m.zip"]
        );
    }

    #[test]
    fn recover_restores_backups_of_prepared_operation() {
        let f = fixture();
        let op = begin(
            &f.db,
            "install",
            &f.id,
            &[
                target("mods/a.jar", Some("a.jar"), false),
                target("mods/new.jar", None, true),
            ],
            1,
        )
        .unwrap();
        let staging = f.paths.staging_dir().join(&op.id);
        std::fs::create_dir_all(staging.join("backup")).unwrap();
        std::fs::write(staging.join("backup").join("a.jar"), b"old").unwrap();
        std::fs::write(f.game_dir.join("mods").join("a.jar"), b"new").unwrap();
        std::fs::write(f.game_dir.join("mods").join("new.jar"), b"added").unwrap();

        recover(&f.db, &f.paths).unwrap();

        assert_eq!(
            std::fs::read(f.game_dir.join("mods").join("a.jar")).unwrap(),
            b"old"
        );
        assert!(!f.game_dir.join("mods").join("new.jar").exists());
        assert!(!staging.exists());
        assert!(list(&f.db).unwrap().is_empty());
    }

    #[test]
    fn recover_only_cleans_staging_of_committed_operation() {
        let f = fixture();
        let op = begin(
            &f.db,
            "install",
            &f.id,
            &[
                target("mods/a.jar", Some("a.jar"), false),
                target("mods/new.jar", None, true),
            ],
            1,
        )
        .unwrap();
        f.db.with_conn(|conn| mark_committed(conn, &op.id)).unwrap();
        let staging = f.paths.staging_dir().join(&op.id);
        std::fs::create_dir_all(staging.join("backup")).unwrap();
        std::fs::write(staging.join("backup").join("a.jar"), b"old").unwrap();
        std::fs::write(f.game_dir.join("mods").join("a.jar"), b"new").unwrap();
        std::fs::write(f.game_dir.join("mods").join("new.jar"), b"added").unwrap();

        recover(&f.db, &f.paths).unwrap();

        assert_eq!(
            std::fs::read(f.game_dir.join("mods").join("a.jar")).unwrap(),
            b"new"
        );
        assert!(f.game_dir.join("mods").join("new.jar").exists());
        assert!(!staging.exists());
        assert!(list(&f.db).unwrap().is_empty());
    }
}
