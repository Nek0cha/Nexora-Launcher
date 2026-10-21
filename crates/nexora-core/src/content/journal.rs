//! 複数のファイルと DB をまたぐ操作の記録（spec 0 の 3.7 節）と、起動時の復旧。
//! 手順：`begin`（prepared）→ `staging/<id>/` に用意と退避 → 移動と DB の更新と
//! `mark_committed` を 1 つのトランザクションで → `staging/<id>/` を消して `finish`。
//! 記録を書く側（begin、mark_committed、add_backup、prune_backups）は `apply` が使う。

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

/// そのインスタンスに、戻しきれずに残っている prepared の操作があるか。
/// あるうちに中身を変えると、次の起動の復旧が新しい変更を上書きしうる。
pub(crate) fn has_prepared(db: &Db, instance_id: &str) -> Result<bool, CoreError> {
    db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM operations WHERE instance_id = ?1 AND state = ?2)",
            params![instance_id, OpState::Prepared.as_str()],
            |r| r.get::<_, i64>(0),
        )? != 0)
    })
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
    // コピーは不可分ではなく、途中で落ちると戻し先が書きかけになりうる。それでも安全なのは、
    // 退避は消えずに残り、操作の記録と staging も復旧が終わるまで残るため、次の復旧で上書きし直せるから
    std::fs::copy(from, to).map_err(|e| io_err(to, e))?;
    Ok(())
}

/// 中身を置くフォルダー。復旧で触ってよいのはこの下だけ。
const CONTENT_DIRS: [&str; 3] = ["mods", "resourcepacks", "shaderpacks"];

/// `/` 区切りの相対パスとして安全か。空の区切り、`.`、`..`、`\`、`:`（ドライブや代替ストリーム）、
/// 先頭の `/` を含むものは断る。
pub(super) fn is_safe_relative(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|seg| {
            !seg.is_empty() && seg != "." && seg != ".." && !seg.contains(['\\', ':', '\0'])
        })
}

fn unsafe_path(op_id: &str, path: &str) -> CoreError {
    CoreError::Unexpected {
        message: format!("operation {op_id} has an unsafe path: {path}"),
    }
}

/// 記録された対象を、ゲームディレクトリ内の戻し先と退避の場所に直す。
/// 中身のフォルダーの外を指すもの、退避の名前が安全でないものは断る。
fn resolve_target(
    game_dir: &Path,
    backup_dir: &Path,
    op_id: &str,
    t: &Target,
) -> Result<(PathBuf, Option<PathBuf>), CoreError> {
    let top = t.rel_path.split('/').next().unwrap_or("");
    if !is_safe_relative(&t.rel_path) || !CONTENT_DIRS.contains(&top) || !t.rel_path.contains('/') {
        return Err(unsafe_path(op_id, &t.rel_path));
    }
    let backup = match &t.backup {
        Some(name) if !is_safe_relative(name) => return Err(unsafe_path(op_id, name)),
        Some(name) => Some(backup_dir.join(name)),
        None => None,
    };
    Ok((game_dir.join(&t.rel_path), backup))
}

/// 1 件の対象を元に戻す。退避が無ければ、退避の前に止まったものとして、新規のファイルだけを消す。
fn roll_back_target(
    game_dir: &Path,
    backup_dir: &Path,
    op_id: &str,
    t: &Target,
) -> Result<(), CoreError> {
    let (dest, backup) = resolve_target(game_dir, backup_dir, op_id, t)?;
    match backup {
        Some(from) if from.is_file() => restore_file(&from, &dest),
        _ if t.created => remove_file_if_exists(&dest),
        _ => Ok(()),
    }
}

/// prepared の操作のファイルを元に戻す。1 件の対象の失敗で残りを止めず、最初の失敗を返す。
fn roll_back(game_dir: &Path, staging: &Path, op: &Operation) -> Result<(), CoreError> {
    let backup_dir = staging.join("backup");
    let mut first_err = None;
    for t in &op.targets {
        let result = roll_back_target(game_dir, &backup_dir, &op.id, t);
        if let Err(err) = result {
            tracing::warn!(op_id = %op.id, rel_path = %t.rel_path, code = err.code(), "could not roll back a file");
            first_err.get_or_insert(err);
        }
    }
    first_err.map_or(Ok(()), Err)
}

/// 1 件の操作を復旧する。戻しきれなければ記録と staging を残し、次の起動でやり直す。
fn recover_one(db: &Db, paths: &AppPaths, op: &Operation) -> Result<(), CoreError> {
    // ID は staging の下のフォルダー名に使う。1 つの区切りでないものは staging の外を指しうるので断る
    if !is_safe_relative(&op.id) || op.id.contains('/') {
        return Err(unsafe_path(&op.id, &op.id));
    }
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

    /// `staging/<id>/backup/<name>` に退避を置く。
    fn put_backup(f: &Fixture, op_id: &str, name: &str, body: &[u8]) -> PathBuf {
        let staging = f.paths.staging_dir().join(op_id);
        std::fs::create_dir_all(staging.join("backup")).unwrap();
        std::fs::write(staging.join("backup").join(name), body).unwrap();
        staging
    }

    fn read(path: &Path) -> Vec<u8> {
        std::fs::read(path).unwrap()
    }

    #[test]
    fn recover_rejects_targets_outside_the_content_dirs() {
        let f = fixture();
        let outside = f.game_dir.parent().unwrap().join("outside.jar");
        let absolute = f._temp.path().join("abs.jar");
        let config = f.game_dir.join("config").join("x.txt");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        for p in [&outside, &absolute, &config] {
            std::fs::write(p, b"keep").unwrap();
        }
        // 退避の名前で staging の外を指すもの
        let evil = f.paths.staging_dir().join("evil.jar");
        std::fs::write(&evil, b"evil").unwrap();
        std::fs::write(f.game_dir.join("mods").join("c.jar"), b"keep").unwrap();
        std::fs::write(f.game_dir.join("mods").join("a.jar"), b"new").unwrap();
        let op = begin(
            &f.db,
            "install",
            &f.id,
            &[
                target("../outside.jar", None, true),
                target("mods/../../outside.jar", None, true),
                target(&absolute.display().to_string(), None, true),
                target("config/x.txt", None, true),
                target("mods/c.jar", Some("../../evil.jar"), false),
                target("mods/a.jar", Some("a.jar"), false),
            ],
            1,
        )
        .unwrap();
        let staging = put_backup(&f, &op.id, "a.jar", b"old");

        recover(&f.db, &f.paths).unwrap();

        for p in [&outside, &absolute, &config] {
            assert_eq!(read(p), b"keep", "{} was touched", p.display());
        }
        assert_eq!(read(&f.game_dir.join("mods").join("c.jar")), b"keep");
        assert_eq!(read(&evil), b"evil");
        // 安全な対象は戻し、記録と staging は残す
        assert_eq!(read(&f.game_dir.join("mods").join("a.jar")), b"old");
        assert!(staging.exists());
        assert_eq!(list(&f.db).unwrap().len(), 1);
    }

    #[test]
    fn recover_keeps_a_partly_restored_operation_and_finishes_it_on_retry() {
        let f = fixture();
        let mods = f.game_dir.join("mods");
        std::fs::write(mods.join("a.jar"), b"new").unwrap();
        // 戻し先がフォルダーなので、b.jar は戻せない
        std::fs::create_dir_all(mods.join("b.jar")).unwrap();
        let op = begin(
            &f.db,
            "update",
            &f.id,
            &[
                target("mods/b.jar", Some("b.jar"), false),
                target("mods/a.jar", Some("a.jar"), false),
            ],
            1,
        )
        .unwrap();
        let staging = put_backup(&f, &op.id, "a.jar", b"old-a");
        put_backup(&f, &op.id, "b.jar", b"old-b");

        recover(&f.db, &f.paths).unwrap();

        assert_eq!(read(&mods.join("a.jar")), b"old-a");
        assert!(staging.join("backup").join("b.jar").is_file());
        assert_eq!(list(&f.db).unwrap().len(), 1);

        std::fs::remove_dir(mods.join("b.jar")).unwrap();
        recover(&f.db, &f.paths).unwrap();

        assert_eq!(read(&mods.join("a.jar")), b"old-a");
        assert_eq!(read(&mods.join("b.jar")), b"old-b");
        assert!(!staging.exists());
        assert!(list(&f.db).unwrap().is_empty());
    }

    #[test]
    fn one_failing_operation_does_not_stop_the_others() {
        let f = fixture();
        let mods = f.game_dir.join("mods");
        std::fs::create_dir_all(mods.join("bad.jar")).unwrap();
        std::fs::write(mods.join("a.jar"), b"new").unwrap();
        let bad = begin(
            &f.db,
            "install",
            &f.id,
            &[target("mods/bad.jar", Some("bad.jar"), false)],
            1,
        )
        .unwrap();
        put_backup(&f, &bad.id, "bad.jar", b"old");
        let good = begin(
            &f.db,
            "install",
            &f.id,
            &[target("mods/a.jar", Some("a.jar"), false)],
            2,
        )
        .unwrap();
        let good_staging = put_backup(&f, &good.id, "a.jar", b"old-a");

        recover(&f.db, &f.paths).unwrap();

        assert_eq!(read(&mods.join("a.jar")), b"old-a");
        assert!(!good_staging.exists());
        let left = list(&f.db).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, bad.id);
        assert!(f.paths.staging_dir().join(&bad.id).exists());
    }

    #[test]
    fn recover_leaves_a_replaced_target_without_backup_untouched() {
        let f = fixture();
        let mods = f.game_dir.join("mods");
        std::fs::write(mods.join("a.jar"), b"current").unwrap();
        let op = begin(
            &f.db,
            "update",
            &f.id,
            &[target("mods/a.jar", None, false)],
            1,
        )
        .unwrap();
        let staging = f.paths.staging_dir().join(&op.id);
        std::fs::create_dir_all(&staging).unwrap();

        recover(&f.db, &f.paths).unwrap();

        assert_eq!(read(&mods.join("a.jar")), b"current");
        assert!(!staging.exists());
        assert!(list(&f.db).unwrap().is_empty());
    }
}
