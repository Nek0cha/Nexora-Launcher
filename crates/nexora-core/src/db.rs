//! SQLite の接続とマイグレーション。接続は 1 本だけを Mutex で守る。
//! 呼び出し側は spawn_blocking の中で with_conn を使い、.await をまたいでロックを持たない。

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

use crate::error::CoreError;

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("../migrations/0001_init.sql")),
        M::up(include_str!("../migrations/0002_instances.sql")),
        M::up(include_str!("../migrations/0003_accounts.sql")),
    ])
}

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, CoreError> {
        Self::open_with(path, &migrations())
    }

    pub fn open_in_memory() -> Result<Self, CoreError> {
        let mut conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        migrations().to_latest(&mut conn)?;
        Ok(Self::wrap(conn))
    }

    pub(crate) fn open_with(path: &Path, migrations: &Migrations<'_>) -> Result<Self, CoreError> {
        let mut conn = Connection::open(path)?;
        Self::configure(&conn)?;
        migrations.to_latest(&mut conn)?;
        Ok(Self::wrap(conn))
    }

    fn configure(conn: &Connection) -> Result<(), CoreError> {
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(())
    }

    fn wrap(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// 接続を借りて処理する。必ず同期の処理の中で使い、.await をまたがない。
    pub fn with_conn<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let mut guard = self.conn.lock().expect("database mutex poisoned");
        f(&mut guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite_migration::{M, Migrations};

    fn table_exists(conn: &rusqlite::Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
            == 1
    }

    #[test]
    fn open_applies_migrations() {
        let db = Db::open_in_memory().unwrap();
        for table in [
            "accounts",
            "app_settings",
            "instances",
            "java_runtimes",
            "running_sessions",
        ] {
            let exists = db.with_conn(|conn| Ok(table_exists(conn, table))).unwrap();
            assert!(exists, "{table} was not created");
        }
    }

    #[test]
    fn reopening_an_existing_db_is_a_no_op() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nexora.db");
        {
            let db = Db::open(&path).unwrap();
            db.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO app_settings (key, value, updated_at) VALUES ('k', '\"v\"', 1)",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        }
        let db = Db::open(&path).unwrap();
        let value: String = db
            .with_conn(|conn| {
                Ok(
                    conn.query_row("SELECT value FROM app_settings WHERE key = 'k'", [], |r| {
                        r.get(0)
                    })?,
                )
            })
            .unwrap();
        assert_eq!(value, "\"v\"");
    }

    #[test]
    fn failed_migration_leaves_db_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nexora.db");
        drop(Db::open(&path).unwrap());

        let broken = Migrations::new(vec![
            M::up(include_str!("../migrations/0001_init.sql")),
            M::up(include_str!("../migrations/0002_instances.sql")),
            M::up(include_str!("../migrations/0003_accounts.sql")),
            M::up("CREATE TABLE half_done (x INTEGER); THIS IS NOT SQL;"),
        ]);
        let result = Db::open_with(&path, &broken);
        assert!(matches!(result, Err(CoreError::Migration(_))));

        let conn = rusqlite::Connection::open(&path).unwrap();
        assert!(
            !table_exists(&conn, "half_done"),
            "partial migration was not rolled back"
        );
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nexora.db");
        std::fs::write(
            &path,
            b"this is not a sqlite database at all, just garbage bytes",
        )
        .unwrap();
        assert!(Db::open(&path).is_err());
    }

    #[test]
    fn corrupt_file_maps_to_db_failed_code() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nexora.db");
        std::fs::write(
            &path,
            b"this is not a sqlite database at all, just garbage bytes",
        )
        .unwrap();
        let err = Db::open(&path).err().unwrap();
        assert_eq!(err.to_payload().code, "db.failed");
    }
}
