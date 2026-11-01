//! Modrinth App のインスタンスを読む。`app.db` は一時フォルダへコピーしてから読み、元には触れない。
//! 保存形式は `docs/research/05-modrinth-app-format.md` にある。

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};

use super::{ImportKind, ImportSource, SourceInstance};
use crate::error::CoreError;
use crate::meta::loader::LoaderKind;
use crate::mrpack::safe_path::{check_rel_path, is_link};

const APP_FOLDER: &str = "ModrinthApp";
const DB_FILE: &str = "app.db";
const PROFILES_DIR: &str = "profiles";
/// 導入の途中で、遊べる状態ではない `install_stage`。
const INCOMPLETE_STAGES: [&str; 4] = [
    "not_installed",
    "installing",
    "minecraft_installing",
    "pack_installing",
];

/// Modrinth App のデータフォルダ（`app.db` があるところ）。
pub struct ModrinthApp {
    root: PathBuf,
}

fn unavailable(reason: impl Into<String>) -> CoreError {
    CoreError::ImportUnavailable {
        reason: reason.into(),
    }
}

/// コピーを置く一時フォルダ。捨てるときに中身ごと消す。
struct TempCopy(PathBuf);

impl TempCopy {
    fn new() -> std::io::Result<Self> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("nexora-import-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for TempCopy {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `app.db` から読んだ 1 行。
struct Row {
    path: String,
    name: String,
    stage: String,
    mc_version: String,
    loader: String,
    loader_version: Option<String>,
}

impl ModrinthApp {
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// 既定の場所に `app.db` があれば、そのフォルダ。
    pub fn locate() -> Option<PathBuf> {
        let root = dirs::data_dir()?.join(APP_FOLDER);
        root.join(DB_FILE).is_file().then_some(root)
    }

    /// `app.db` を（あれば `-wal`、`-shm` ごと）一時フォルダへコピーして開く。
    fn open_copy(&self) -> Result<(TempCopy, Connection), CoreError> {
        let source = self.root.join(DB_FILE);
        if !source.is_file() {
            return Err(unavailable(format!("{} was not found", source.display())));
        }
        let dir = TempCopy::new().map_err(|e| unavailable(e.to_string()))?;
        for suffix in ["", "-wal", "-shm"] {
            let from = self.root.join(format!("{DB_FILE}{suffix}"));
            if from.is_file() {
                fs::copy(&from, dir.0.join(format!("{DB_FILE}{suffix}")))
                    .map_err(|e| unavailable(format!("{}: {e}", from.display())))?;
            }
        }
        let conn = Connection::open(dir.0.join(DB_FILE)).map_err(|e| unavailable(e.to_string()))?;
        Ok((dir, conn))
    }

    fn rows(conn: &Connection) -> Result<Vec<Row>, rusqlite::Error> {
        let has = |table: &str| -> Result<bool, rusqlite::Error> {
            conn.query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |_| Ok(()),
            )
            .optional()
            .map(|r| r.is_some())
        };
        let sql = if has("instances")? && has("instance_content_sets")? {
            "SELECT i.path, i.name, i.install_stage, c.game_version, c.loader, c.loader_version
             FROM instances i
             JOIN instance_content_sets c ON c.id = i.applied_content_set_id AND c.instance_id = i.id
             ORDER BY i.name"
        } else if has("profiles")? {
            "SELECT path, name, install_stage, game_version, mod_loader, mod_loader_version
             FROM profiles ORDER BY name"
        } else {
            return Err(rusqlite::Error::InvalidQuery);
        };
        let mut stmt = conn.prepare(sql)?;
        stmt.query_map([], |r| {
            Ok(Row {
                path: r.get(0)?,
                name: r.get(1)?,
                stage: r.get(2)?,
                mc_version: r.get(3)?,
                loader: r.get(4)?,
                loader_version: r.get(5)?,
            })
        })?
        .collect()
    }

    /// インスタンスのフォルダがある `profiles` フォルダ。設定に `custom_dir` があればその下。
    fn profiles_dir(&self, conn: &Connection) -> PathBuf {
        let custom: Option<String> = conn
            .query_row("SELECT custom_dir FROM settings WHERE id = 0", [], |r| {
                r.get(0)
            })
            .ok()
            .flatten();
        match custom.filter(|c| !c.trim().is_empty()) {
            Some(dir) => PathBuf::from(dir).join(PROFILES_DIR),
            None => self.root.join(PROFILES_DIR),
        }
    }

    fn profiles_dir_of_db(&self) -> Result<PathBuf, CoreError> {
        let (_dir, conn) = self.open_copy()?;
        Ok(self.profiles_dir(&conn))
    }
}

fn parse_loader(value: &str) -> Option<LoaderKind> {
    match value {
        "vanilla" => Some(LoaderKind::Vanilla),
        "fabric" => Some(LoaderKind::Fabric),
        "quilt" => Some(LoaderKind::Quilt),
        "forge" => Some(LoaderKind::Forge),
        "neoforge" => Some(LoaderKind::Neoforge),
        _ => None,
    }
}

/// フォルダがあり、中に何か入っていて、リンクではないか。
fn has_content(dir: &Path) -> bool {
    !is_link(dir)
        && fs::read_dir(dir)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
}

impl ImportSource for ModrinthApp {
    fn list(&self) -> Result<Vec<SourceInstance>, CoreError> {
        let (_dir, conn) = self.open_copy()?;
        let rows =
            Self::rows(&conn).map_err(|e| unavailable(format!("app.db could not be read: {e}")))?;
        let profiles = self.profiles_dir(&conn);
        Ok(rows
            .into_iter()
            .map(|row| {
                let loader = parse_loader(&row.loader);
                let unsupported = if check_rel_path(&row.path).is_err() {
                    Some(format!("{} is not a safe folder name", row.path))
                } else if loader.is_none() {
                    Some(format!("the loader {} is not supported", row.loader))
                } else if row.mc_version.trim().is_empty() {
                    Some("the Minecraft version is missing".to_string())
                } else if INCOMPLETE_STAGES.contains(&row.stage.as_str()) {
                    Some("the installation in Modrinth App is not finished".to_string())
                } else {
                    None
                };
                let available = if unsupported.is_none() {
                    ImportKind::ALL
                        .into_iter()
                        .filter(|k| has_content(&profiles.join(&row.path).join(k.dir_name())))
                        .collect()
                } else {
                    Vec::new()
                };
                SourceInstance {
                    id: row.path,
                    name: row.name,
                    mc_version: row.mc_version,
                    loader: loader.unwrap_or(LoaderKind::Vanilla),
                    loader_version: row.loader_version.filter(|v| !v.trim().is_empty()),
                    available,
                    unsupported,
                }
            })
            .collect())
    }

    fn folder(&self, id: &str, kind: ImportKind) -> Result<Option<PathBuf>, CoreError> {
        let rel = check_rel_path(id)?;
        let dir = self.profiles_dir_of_db()?.join(rel).join(kind.dir_name());
        Ok((dir.is_dir() && !is_link(&dir)).then_some(dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::loader::LoaderKind;
    use rusqlite::Connection;
    use std::fs;
    use std::path::Path;

    const NEW_SCHEMA: &str = "
        CREATE TABLE settings (id INTEGER PRIMARY KEY, custom_dir TEXT NULL);
        INSERT INTO settings (id) VALUES (0);
        CREATE TABLE instances (id TEXT PRIMARY KEY, path TEXT, applied_content_set_id TEXT,
            install_stage TEXT, name TEXT);
        CREATE TABLE instance_content_sets (id TEXT PRIMARY KEY, instance_id TEXT, game_version TEXT,
            loader TEXT, loader_version TEXT);";
    const OLD_SCHEMA: &str = "
        CREATE TABLE settings (id INTEGER PRIMARY KEY, custom_dir TEXT NULL);
        INSERT INTO settings (id) VALUES (0);
        CREATE TABLE profiles (path TEXT PRIMARY KEY, install_stage TEXT, name TEXT,
            game_version TEXT, mod_loader TEXT, mod_loader_version TEXT);";

    fn db(root: &Path, schema: &str, rows: &str) {
        fs::create_dir_all(root).unwrap();
        let conn = Connection::open(root.join("app.db")).unwrap();
        conn.execute_batch(schema).unwrap();
        conn.execute_batch(rows).unwrap();
    }

    fn new_row(id: &str, path: &str, name: &str, stage: &str, mc: &str, loader: &str) -> String {
        format!(
            "INSERT INTO instance_content_sets VALUES ('c-{id}', '{id}', '{mc}', '{loader}', '1.0');
             INSERT INTO instances VALUES ('{id}', '{path}', 'c-{id}', '{stage}', '{name}');"
        )
    }

    fn file(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn lists_instances_from_the_current_schema_with_the_content_that_exists() {
        let tmp = tempfile::tempdir().unwrap();
        db(
            tmp.path(),
            NEW_SCHEMA,
            &new_row(
                "a",
                "Survival",
                "Survival 2024",
                "installed",
                "1.21.1",
                "fabric",
            ),
        );
        file(&tmp.path().join("profiles/Survival/mods/a.jar"));
        file(&tmp.path().join("profiles/Survival/saves/w/level.dat"));
        fs::create_dir_all(tmp.path().join("profiles/Survival/screenshots")).unwrap();
        let list = ModrinthApp::at(tmp.path().to_path_buf()).list().unwrap();
        assert_eq!(list.len(), 1);
        let i = &list[0];
        assert_eq!(
            (i.name.as_str(), i.mc_version.as_str()),
            ("Survival 2024", "1.21.1")
        );
        assert_eq!(i.loader, LoaderKind::Fabric);
        assert_eq!(i.loader_version.as_deref(), Some("1.0"));
        assert_eq!(i.available, [ImportKind::Mods, ImportKind::Saves]);
        assert_eq!(i.unsupported, None);
    }

    #[test]
    fn falls_back_to_the_profiles_table_of_older_versions() {
        let tmp = tempfile::tempdir().unwrap();
        db(
            tmp.path(),
            OLD_SCHEMA,
            "INSERT INTO profiles VALUES ('Old', 'installed', 'Old pack', '1.20.1', 'forge', '47.2.0');",
        );
        let list = ModrinthApp::at(tmp.path().to_path_buf()).list().unwrap();
        assert_eq!(list[0].name, "Old pack");
        assert_eq!(list[0].loader, LoaderKind::Forge);
        assert_eq!(list[0].loader_version.as_deref(), Some("47.2.0"));
    }

    #[test]
    fn rows_that_cannot_be_imported_say_why() {
        let tmp = tempfile::tempdir().unwrap();
        let rows = [
            new_row("a", "Odd", "Odd", "installed", "1.21", "liteloader"),
            new_row("b", "Half", "Half", "not_installed", "1.21", "fabric"),
            new_row("c", "../escape", "Escape", "installed", "1.21", "fabric"),
        ]
        .concat();
        db(tmp.path(), NEW_SCHEMA, &rows);
        let list = ModrinthApp::at(tmp.path().to_path_buf()).list().unwrap();
        assert_eq!(list.len(), 3);
        assert!(list.iter().all(|i| i.unsupported.is_some()), "{list:?}");
        assert!(list.iter().all(|i| i.available.is_empty()));
    }

    #[test]
    fn folder_returns_existing_directories_only() {
        let tmp = tempfile::tempdir().unwrap();
        db(
            tmp.path(),
            NEW_SCHEMA,
            &new_row("a", "Survival", "S", "installed", "1.21", "vanilla"),
        );
        file(&tmp.path().join("profiles/Survival/mods/a.jar"));
        let app = ModrinthApp::at(tmp.path().to_path_buf());
        assert_eq!(
            app.folder("Survival", ImportKind::Mods).unwrap(),
            Some(tmp.path().join("profiles").join("Survival").join("mods"))
        );
        assert_eq!(app.folder("Survival", ImportKind::Saves).unwrap(), None);
    }

    #[test]
    fn folder_refuses_an_id_that_leaves_the_profiles_directory() {
        let tmp = tempfile::tempdir().unwrap();
        db(tmp.path(), NEW_SCHEMA, "");
        let err = ModrinthApp::at(tmp.path().to_path_buf())
            .folder("../outside", ImportKind::Mods)
            .unwrap_err();
        assert_eq!(err.code(), "mrpack.unsafe_path");
    }

    #[test]
    fn a_broken_or_foreign_database_is_unavailable() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("app.db"), b"not a database at all").unwrap();
        let err = ModrinthApp::at(tmp.path().to_path_buf())
            .list()
            .unwrap_err();
        assert_eq!(err.code(), "import.unavailable");

        let other = tempfile::tempdir().unwrap();
        db(other.path(), "CREATE TABLE unrelated (x INTEGER);", "");
        let err = ModrinthApp::at(other.path().to_path_buf())
            .list()
            .unwrap_err();
        assert_eq!(err.code(), "import.unavailable");

        let empty = tempfile::tempdir().unwrap();
        let err = ModrinthApp::at(empty.path().to_path_buf())
            .list()
            .unwrap_err();
        assert_eq!(err.code(), "import.unavailable");
    }

    #[test]
    fn reading_leaves_the_original_database_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        db(
            tmp.path(),
            NEW_SCHEMA,
            &new_row("a", "S", "S", "installed", "1.21", "vanilla"),
        );
        let before = fs::read(tmp.path().join("app.db")).unwrap();
        ModrinthApp::at(tmp.path().to_path_buf()).list().unwrap();
        assert_eq!(fs::read(tmp.path().join("app.db")).unwrap(), before);
        assert!(!tmp.path().join("app.db-wal").exists());
    }

    #[test]
    fn a_custom_directory_in_the_settings_holds_the_profiles() {
        let tmp = tempfile::tempdir().unwrap();
        let custom = tempfile::tempdir().unwrap();
        db(
            tmp.path(),
            NEW_SCHEMA,
            &new_row("a", "S", "S", "installed", "1.21", "vanilla"),
        );
        Connection::open(tmp.path().join("app.db"))
            .unwrap()
            .execute(
                "UPDATE settings SET custom_dir = ?1",
                [custom.path().to_str().unwrap()],
            )
            .unwrap();
        file(&custom.path().join("profiles/S/mods/a.jar"));
        let list = ModrinthApp::at(tmp.path().to_path_buf()).list().unwrap();
        assert_eq!(list[0].available, [ImportKind::Mods]);
    }
}
