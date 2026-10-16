//! インスタンスの作成と取得。

pub mod dir_name;
pub mod lock;

use std::path::PathBuf;

use rusqlite::{OptionalExtension, Row, params};
use serde::Serialize;
use ts_rs::TS;

use crate::db::Db;
use crate::error::CoreError;
use crate::paths::AppPaths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum InstanceState {
    Installing,
    Ready,
    Broken,
}

impl InstanceState {
    fn as_str(self) -> &'static str {
        match self {
            InstanceState::Installing => "installing",
            InstanceState::Ready => "ready",
            InstanceState::Broken => "broken",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "ready" => InstanceState::Ready,
            "installing" => InstanceState::Installing,
            _ => InstanceState::Broken,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub dir_name: String,
    pub mc_version: String,
    pub state: InstanceState,
    pub created_at: i64,
    pub last_played_at: Option<i64>,
    pub play_time_ms: i64,
}

const COLUMNS: &str =
    "id, name, dir_name, mc_version, state, created_at, last_played_at, play_time_ms";

fn from_row(row: &Row<'_>) -> rusqlite::Result<Instance> {
    let state: String = row.get(4)?;
    Ok(Instance {
        id: row.get(0)?,
        name: row.get(1)?,
        dir_name: row.get(2)?,
        mc_version: row.get(3)?,
        state: InstanceState::parse(&state),
        created_at: row.get(5)?,
        last_played_at: row.get(6)?,
        play_time_ms: row.get(7)?,
    })
}

fn not_found(id: &str) -> CoreError {
    CoreError::InstanceNotFound {
        instance_id: id.to_string(),
    }
}

pub fn game_dir(paths: &AppPaths, instance: &Instance) -> PathBuf {
    paths.instances_dir().join(&instance.dir_name)
}

/// 行を追加し、ゲームディレクトリを作る。名前が空白だけならバージョン名を使う。
pub fn create(
    db: &Db,
    paths: &AppPaths,
    name: &str,
    mc_version: &str,
    now_ms: i64,
) -> Result<Instance, CoreError> {
    let name = match name.trim() {
        "" => mc_version.to_string(),
        trimmed => trimmed.to_string(),
    };
    let base = dir_name::sanitize(&name);
    let instances_dir = paths.instances_dir();
    let instance = db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT dir_name FROM instances")?;
        let used: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let dir = dir_name::unique(&base, |candidate| {
            used.iter().any(|u| u.eq_ignore_ascii_case(candidate))
                || instances_dir.join(candidate).exists()
        });
        let instance = Instance {
            id: uuid::Uuid::now_v7().to_string(),
            name,
            dir_name: dir,
            mc_version: mc_version.to_string(),
            state: InstanceState::Installing,
            created_at: now_ms,
            last_played_at: None,
            play_time_ms: 0,
        };
        conn.execute(
            "INSERT INTO instances (id, name, dir_name, mc_version, state, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                instance.id,
                instance.name,
                instance.dir_name,
                instance.mc_version,
                instance.state.as_str(),
                instance.created_at
            ],
        )?;
        Ok(instance)
    })?;
    let dir = game_dir(paths, &instance);
    if let Err(source) = std::fs::create_dir_all(&dir) {
        // フォルダを作れなければ行も残さない
        let _ = db.with_conn(|conn| {
            conn.execute("DELETE FROM instances WHERE id = ?1", [&instance.id])?;
            Ok(())
        });
        return Err(CoreError::Io {
            path: dir.display().to_string(),
            source,
        });
    }
    Ok(instance)
}

pub fn list(db: &Db) -> Result<Vec<Instance>, CoreError> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM instances
             ORDER BY last_played_at IS NULL, last_played_at DESC, created_at DESC"
        ))?;
        let rows = stmt.query_map([], from_row)?.collect::<Result<_, _>>()?;
        Ok(rows)
    })
}

pub fn get(db: &Db, id: &str) -> Result<Instance, CoreError> {
    db.with_conn(|conn| {
        Ok(conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM instances WHERE id = ?1"),
                [id],
                from_row,
            )
            .optional()?)
    })?
    .ok_or_else(|| not_found(id))
}

fn update(db: &Db, id: &str, sql: &str, value: &dyn rusqlite::ToSql) -> Result<(), CoreError> {
    let changed = db.with_conn(|conn| Ok(conn.execute(sql, params![value, id])?))?;
    if changed == 0 {
        return Err(not_found(id));
    }
    Ok(())
}

pub fn set_state(db: &Db, id: &str, state: InstanceState) -> Result<(), CoreError> {
    update(
        db,
        id,
        "UPDATE instances SET state = ?1 WHERE id = ?2",
        &state.as_str(),
    )
}

pub fn record_launch(db: &Db, id: &str, now_ms: i64) -> Result<(), CoreError> {
    update(
        db,
        id,
        "UPDATE instances SET last_played_at = ?1 WHERE id = ?2",
        &now_ms,
    )
}

pub fn add_play_time(db: &Db, id: &str, ms: i64) -> Result<(), CoreError> {
    update(
        db,
        id,
        "UPDATE instances SET play_time_ms = play_time_ms + ?1 WHERE id = ?2",
        &ms.max(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, AppPaths, Db) {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("data"));
        paths.ensure_dirs().unwrap();
        (temp, paths, Db::open_in_memory().unwrap())
    }

    #[test]
    fn create_inserts_row_and_directory() {
        let (_t, paths, db) = setup();
        let inst = create(&db, &paths, "日本語", "1.21.5", 100).unwrap();
        assert_eq!(inst.name, "日本語");
        assert_eq!(inst.dir_name, "日本語");
        assert_eq!(inst.mc_version, "1.21.5");
        assert_eq!(inst.state, InstanceState::Installing);
        assert_eq!(inst.created_at, 100);
        assert_eq!(inst.last_played_at, None);
        assert_eq!(inst.play_time_ms, 0);
        assert!(game_dir(&paths, &inst).is_dir());
        assert_eq!(get(&db, &inst.id).unwrap(), inst);
    }

    #[test]
    fn blank_name_defaults_to_version() {
        let (_t, paths, db) = setup();
        let inst = create(&db, &paths, "   ", "1.8.9", 1).unwrap();
        assert_eq!(inst.name, "1.8.9");
        assert_eq!(inst.dir_name, "1.8.9");
    }

    #[test]
    fn duplicate_names_get_distinct_directories() {
        let (_t, paths, db) = setup();
        let a = create(&db, &paths, "World", "1.21.5", 1).unwrap();
        let b = create(&db, &paths, "world", "1.21.5", 2).unwrap();
        assert_eq!(a.dir_name, "World");
        assert_eq!(b.dir_name, "world (2)");
    }

    #[test]
    fn existing_folder_on_disk_is_avoided() {
        let (_t, paths, db) = setup();
        std::fs::create_dir_all(paths.instances_dir().join("Taken")).unwrap();
        let inst = create(&db, &paths, "Taken", "1.21.5", 1).unwrap();
        assert_eq!(inst.dir_name, "Taken (2)");
    }

    #[test]
    fn list_orders_by_last_played_then_created() {
        let (_t, paths, db) = setup();
        let old = create(&db, &paths, "old", "1.21.5", 1).unwrap();
        let new = create(&db, &paths, "new", "1.21.5", 2).unwrap();
        let played = create(&db, &paths, "played", "1.21.5", 0).unwrap();
        record_launch(&db, &played.id, 50).unwrap();
        let ids: Vec<_> = list(&db).unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![played.id, new.id, old.id]);
    }

    #[test]
    fn state_and_play_time_updates() {
        let (_t, paths, db) = setup();
        let inst = create(&db, &paths, "x", "1.21.5", 1).unwrap();
        set_state(&db, &inst.id, InstanceState::Ready).unwrap();
        add_play_time(&db, &inst.id, 1500).unwrap();
        add_play_time(&db, &inst.id, 500).unwrap();
        let after = get(&db, &inst.id).unwrap();
        assert_eq!(after.state, InstanceState::Ready);
        assert_eq!(after.play_time_ms, 2000);
    }

    #[test]
    fn missing_instance_is_not_found() {
        let (_t, _paths, db) = setup();
        assert_eq!(get(&db, "nope").unwrap_err().code(), "instance.not_found");
        assert_eq!(
            set_state(&db, "nope", InstanceState::Ready)
                .unwrap_err()
                .code(),
            "instance.not_found"
        );
    }
}
