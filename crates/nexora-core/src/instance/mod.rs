//! インスタンスの作成、取得、設定の更新。

pub mod copy;
pub mod dir_name;
pub mod folder;
pub mod lock;

use std::path::PathBuf;

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
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

/// メモリ割り当ての方式。DB には `auto`、`manual` で保存する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MemoryMode {
    Auto,
    Manual,
}

impl MemoryMode {
    fn as_str(self) -> &'static str {
        match self {
            MemoryMode::Auto => "auto",
            MemoryMode::Manual => "manual",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "manual" => MemoryMode::Manual,
            _ => MemoryMode::Auto,
        }
    }
}

/// Java の選び方。DB には `auto`、`manual` で保存する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum JavaMode {
    Auto,
    Manual,
}

impl JavaMode {
    fn as_str(self) -> &'static str {
        match self {
            JavaMode::Auto => "auto",
            JavaMode::Manual => "manual",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "manual" => JavaMode::Manual,
            _ => JavaMode::Auto,
        }
    }
}

/// 手動で指定できるメモリの下限と上限（MB）。
pub const MEMORY_MIN_MB: u32 = 512;
pub const MEMORY_MAX_MB: u32 = 65536;

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
    pub memory_mode: MemoryMode,
    pub memory_mb: Option<u32>,
    pub java_mode: JavaMode,
    pub java_path: Option<String>,
    pub jvm_args: String,
}

/// 設定画面から更新できる項目。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstanceSettings {
    pub name: String,
    pub memory_mode: MemoryMode,
    pub memory_mb: Option<u32>,
    pub java_mode: JavaMode,
    pub java_path: Option<String>,
    pub jvm_args: String,
}

const COLUMNS: &str = "id, name, dir_name, mc_version, state, created_at, last_played_at,      play_time_ms, memory_mode, memory_mb, java_mode, java_path, jvm_args";

fn from_row(row: &Row<'_>) -> rusqlite::Result<Instance> {
    let state: String = row.get(4)?;
    let memory_mode: String = row.get(8)?;
    let java_mode: String = row.get(10)?;
    let jvm_args: Option<String> = row.get(12)?;
    Ok(Instance {
        id: row.get(0)?,
        name: row.get(1)?,
        dir_name: row.get(2)?,
        mc_version: row.get(3)?,
        state: InstanceState::parse(&state),
        created_at: row.get(5)?,
        last_played_at: row.get(6)?,
        play_time_ms: row.get(7)?,
        memory_mode: MemoryMode::parse(&memory_mode),
        memory_mb: row.get(9)?,
        java_mode: JavaMode::parse(&java_mode),
        java_path: row.get(11)?,
        jvm_args: jvm_args.unwrap_or_default(),
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

/// 行にも、ディスクの上にも使われていないフォルダ名（大文字と小文字は区別しない）。
fn unused_dir_name(
    conn: &rusqlite::Connection,
    instances_dir: &std::path::Path,
    base: &str,
) -> Result<String, CoreError> {
    let mut stmt = conn.prepare("SELECT dir_name FROM instances")?;
    let used: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(dir_name::unique(base, |candidate| {
        used.iter().any(|u| u.eq_ignore_ascii_case(candidate))
            || instances_dir.join(candidate).exists()
    }))
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
        let instance = Instance {
            id: uuid::Uuid::now_v7().to_string(),
            name,
            dir_name: unused_dir_name(conn, &instances_dir, &base)?,
            mc_version: mc_version.to_string(),
            state: InstanceState::Installing,
            created_at: now_ms,
            last_played_at: None,
            play_time_ms: 0,
            memory_mode: MemoryMode::Auto,
            memory_mb: None,
            java_mode: JavaMode::Auto,
            java_path: None,
            jvm_args: String::new(),
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

/// 後始末の対象を消す。失敗しても削除自体は続けるため、警告を残すだけにする。
fn remove_leftover(path: &std::path::Path, is_dir: bool) {
    let result = if is_dir {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    match result {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => tracing::warn!(path = %path.display(), %err, "could not remove leftover"),
    }
}

/// ゲームのディレクトリを消してから行を消す。
/// ディレクトリを消せなければ行は残し、エラーを返す（行だけが消えて残骸が残る事態を避ける）。
/// ディレクトリがなければ行だけを消す。共有のファイル（ライブラリ、アセット）には触れない。
/// 起動中かどうかの確認は呼び出し側で行う。
pub fn delete(db: &Db, paths: &AppPaths, id: &str) -> Result<(), CoreError> {
    let instance = get(db, id)?;
    let dir = game_dir(paths, &instance);
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(CoreError::Io {
                path: dir.display().to_string(),
                source,
            });
        }
    }
    db.with_conn(|conn| {
        conn.execute("DELETE FROM instances WHERE id = ?1", [id])?;
        Ok(())
    })?;
    // 後始末。行はすでにないため、失敗しても警告にとどめる
    remove_leftover(&paths.natives_dir().join(id), true);
    remove_leftover(&paths.argfile(id), false);
    remove_leftover(&paths.game_logs_dir().join(id), true);
    Ok(())
}

/// 複製の行を追加する。ディレクトリは作らない（コピーは呼び出し側で行う）。
/// 新しい `id` と `dir_name` を割り当て、最後に遊んだ日時とプレイ時間は空にする。
/// 名前、設定、`mc_version` は元から引き継ぐ。名前の重複は許し、`dir_name` で区別する。
/// `state` は `installing` で置く。コピーの途中でアプリが落ちても、使える状態に見せないためである。
/// 元の `state` に戻すのは、コピーを終えた呼び出し側の役目である。
pub fn duplicate_row(
    db: &Db,
    paths: &AppPaths,
    source_id: &str,
    now_ms: i64,
) -> Result<Instance, CoreError> {
    let source = get(db, source_id)?;
    let base = dir_name::sanitize(&source.name);
    let instances_dir = paths.instances_dir();
    db.with_conn(|conn| {
        let instance = Instance {
            id: uuid::Uuid::now_v7().to_string(),
            dir_name: unused_dir_name(conn, &instances_dir, &base)?,
            created_at: now_ms,
            last_played_at: None,
            play_time_ms: 0,
            state: InstanceState::Installing,
            ..source
        };
        conn.execute(
            "INSERT INTO instances (id, name, dir_name, mc_version, state, created_at,
                 last_played_at, play_time_ms, memory_mode, memory_mb, java_mode, java_path,
                 jvm_args)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 0, ?7, ?8, ?9, ?10, ?11)",
            params![
                instance.id,
                instance.name,
                instance.dir_name,
                instance.mc_version,
                instance.state.as_str(),
                instance.created_at,
                instance.memory_mode.as_str(),
                instance.memory_mb,
                instance.java_mode.as_str(),
                instance.java_path,
                instance.jvm_args
            ],
        )?;
        Ok(instance)
    })
}

/// 行を消す。複製の後始末に使う（ディレクトリには触れない）。
pub fn remove_row(db: &Db, id: &str) -> Result<(), CoreError> {
    db.with_conn(|conn| {
        conn.execute("DELETE FROM instances WHERE id = ?1", [id])?;
        Ok(())
    })
}

fn invalid(field: &str) -> CoreError {
    CoreError::InvalidInstanceSetting {
        field: field.to_string(),
    }
}

/// 設定を検証して保存する。名前を変えてもフォルダ名は変えない。
/// 自動のときは手動用の値（メモリ量、Java のパス）を捨てる。
pub fn update_settings(
    db: &Db,
    id: &str,
    settings: &InstanceSettings,
) -> Result<Instance, CoreError> {
    let name = settings.name.trim();
    if name.is_empty() {
        return Err(invalid("name"));
    }
    let memory_mb = match settings.memory_mode {
        MemoryMode::Auto => None,
        MemoryMode::Manual => match settings.memory_mb {
            Some(mb) if (MEMORY_MIN_MB..=MEMORY_MAX_MB).contains(&mb) => Some(mb),
            _ => return Err(invalid("memoryMb")),
        },
    };
    let java_path = match settings.java_mode {
        JavaMode::Auto => None,
        JavaMode::Manual => match settings.java_path.as_deref().map(str::trim) {
            Some(path) if !path.is_empty() => Some(path.to_string()),
            _ => return Err(invalid("javaPath")),
        },
    };
    let changed = db.with_conn(|conn| {
        Ok(conn.execute(
            "UPDATE instances SET name = ?1, memory_mode = ?2, memory_mb = ?3,
                 java_mode = ?4, java_path = ?5, jvm_args = ?6
             WHERE id = ?7",
            params![
                name,
                settings.memory_mode.as_str(),
                memory_mb,
                settings.java_mode.as_str(),
                java_path,
                settings.jvm_args,
                id
            ],
        )?)
    })?;
    if changed == 0 {
        return Err(not_found(id));
    }
    get(db, id)
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

    fn settings(name: &str) -> InstanceSettings {
        InstanceSettings {
            name: name.to_string(),
            memory_mode: MemoryMode::Auto,
            memory_mb: None,
            java_mode: JavaMode::Auto,
            java_path: None,
            jvm_args: String::new(),
        }
    }

    fn invalid_field(err: CoreError) -> String {
        match err {
            CoreError::InvalidInstanceSetting { field } => field,
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn defaults_after_create() {
        let (_t, paths, db) = setup();
        let inst = create(&db, &paths, "x", "1.21.5", 1).unwrap();
        assert_eq!(inst.memory_mode, MemoryMode::Auto);
        assert_eq!(inst.memory_mb, None);
        assert_eq!(inst.java_mode, JavaMode::Auto);
        assert_eq!(inst.java_path, None);
        assert_eq!(inst.jvm_args, "");
        assert_eq!(get(&db, &inst.id).unwrap(), inst);
    }

    #[test]
    fn update_settings_changes_fields_and_keeps_dir_name() {
        let (_t, paths, db) = setup();
        let inst = create(&db, &paths, "old", "1.21.5", 1).unwrap();
        let updated = update_settings(
            &db,
            &inst.id,
            &InstanceSettings {
                name: "  new name  ".into(),
                memory_mode: MemoryMode::Manual,
                memory_mb: Some(4096),
                java_mode: JavaMode::Manual,
                java_path: Some(r"C:\java\bin\javaw.exe".into()),
                jvm_args: "-Dfoo=bar".into(),
            },
        )
        .unwrap();
        assert_eq!(updated.name, "new name");
        assert_eq!(updated.dir_name, inst.dir_name);
        assert_eq!(updated.memory_mode, MemoryMode::Manual);
        assert_eq!(updated.memory_mb, Some(4096));
        assert_eq!(updated.java_mode, JavaMode::Manual);
        assert_eq!(updated.java_path.as_deref(), Some(r"C:\java\bin\javaw.exe"));
        assert_eq!(updated.jvm_args, "-Dfoo=bar");
        assert_eq!(get(&db, &inst.id).unwrap(), updated);
    }

    #[test]
    fn update_settings_auto_clears_manual_values() {
        let (_t, paths, db) = setup();
        let inst = create(&db, &paths, "x", "1.21.5", 1).unwrap();
        let mut s = settings("x");
        s.memory_mb = Some(2048);
        s.java_path = Some("java".into());
        let updated = update_settings(&db, &inst.id, &s).unwrap();
        assert_eq!(updated.memory_mb, None);
        assert_eq!(updated.java_path, None);
    }

    #[test]
    fn update_settings_rejects_blank_name() {
        let (_t, paths, db) = setup();
        let inst = create(&db, &paths, "x", "1.21.5", 1).unwrap();
        let err = update_settings(&db, &inst.id, &settings("   ")).unwrap_err();
        assert_eq!(err.code(), "instance.invalid_setting");
        assert_eq!(invalid_field(err), "name");
    }

    #[test]
    fn update_settings_rejects_manual_memory_out_of_range() {
        let (_t, paths, db) = setup();
        let inst = create(&db, &paths, "x", "1.21.5", 1).unwrap();
        for mb in [None, Some(511), Some(65537)] {
            let mut s = settings("x");
            s.memory_mode = MemoryMode::Manual;
            s.memory_mb = mb;
            let err = update_settings(&db, &inst.id, &s).unwrap_err();
            assert_eq!(invalid_field(err), "memoryMb");
        }
        for mb in [512, 65536] {
            let mut s = settings("x");
            s.memory_mode = MemoryMode::Manual;
            s.memory_mb = Some(mb);
            assert!(update_settings(&db, &inst.id, &s).is_ok());
        }
    }

    #[test]
    fn update_settings_rejects_manual_java_without_path() {
        let (_t, paths, db) = setup();
        let inst = create(&db, &paths, "x", "1.21.5", 1).unwrap();
        for path in [None, Some("   ".to_string())] {
            let mut s = settings("x");
            s.java_mode = JavaMode::Manual;
            s.java_path = path;
            let err = update_settings(&db, &inst.id, &s).unwrap_err();
            assert_eq!(invalid_field(err), "javaPath");
        }
    }

    #[test]
    fn delete_removes_dir_and_row() {
        let (_t, paths, db) = setup();
        let inst = create(&db, &paths, "x", "1.21.5", 1).unwrap();
        let dir = game_dir(&paths, &inst);
        std::fs::write(dir.join("options.txt"), "a").unwrap();
        // 共有のファイルと、後始末の対象を用意する
        let shared = paths.shared_dir().join("libraries");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("keep.jar"), "a").unwrap();
        let natives = paths.natives_dir().join(&inst.id);
        std::fs::create_dir_all(&natives).unwrap();
        std::fs::create_dir_all(paths.launch_cache_dir()).unwrap();
        std::fs::write(paths.argfile(&inst.id), "a").unwrap();
        let logs = paths.game_logs_dir().join(&inst.id);
        std::fs::create_dir_all(&logs).unwrap();
        delete(&db, &paths, &inst.id).unwrap();
        assert!(!dir.exists());
        assert!(!natives.exists());
        assert!(!paths.argfile(&inst.id).exists());
        assert!(!logs.exists());
        assert!(shared.join("keep.jar").is_file());
        assert_eq!(get(&db, &inst.id).unwrap_err().code(), "instance.not_found");
    }

    #[test]
    fn delete_without_dir_removes_row() {
        let (_t, paths, db) = setup();
        let inst = create(&db, &paths, "x", "1.21.5", 1).unwrap();
        std::fs::remove_dir_all(game_dir(&paths, &inst)).unwrap();
        delete(&db, &paths, &inst.id).unwrap();
        assert_eq!(get(&db, &inst.id).unwrap_err().code(), "instance.not_found");
    }

    #[test]
    fn delete_missing_is_not_found() {
        let (_t, paths, db) = setup();
        let err = delete(&db, &paths, "nope").unwrap_err();
        assert_eq!(err.code(), "instance.not_found");
    }

    /// 親を読み取り専用にして、ディレクトリを消せない状況を作る。
    /// Windows では読み取り専用の属性が親の中の削除を妨げないため、決まった結果にならない。
    #[cfg(unix)]
    #[test]
    fn delete_keeps_row_when_dir_removal_fails() {
        use std::os::unix::fs::PermissionsExt;
        let (_t, paths, db) = setup();
        let inst = create(&db, &paths, "x", "1.21.5", 1).unwrap();
        let dir = game_dir(&paths, &inst);
        let parent = paths.instances_dir();
        let writable = std::fs::metadata(&parent).unwrap().permissions();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = delete(&db, &paths, &inst.id);
        std::fs::set_permissions(&parent, writable).unwrap();
        // root では権限が効かず削除できてしまうため、その場合は確かめられない
        if !dir.exists() {
            return;
        }
        let err = result.unwrap_err();
        assert_eq!(err.code(), "io.failed");
        assert_eq!(get(&db, &inst.id).unwrap(), inst);
    }

    #[test]
    fn duplicate_row_copies_settings_and_resets_play_stats() {
        let (_t, paths, db) = setup();
        let src = create(&db, &paths, "World", "1.21.5", 1).unwrap();
        update_settings(
            &db,
            &src.id,
            &InstanceSettings {
                name: "World".into(),
                memory_mode: MemoryMode::Manual,
                memory_mb: Some(4096),
                java_mode: JavaMode::Manual,
                java_path: Some("/opt/java/bin/java".into()),
                jvm_args: "-Dfoo=bar".into(),
            },
        )
        .unwrap();
        set_state(&db, &src.id, InstanceState::Ready).unwrap();
        record_launch(&db, &src.id, 50).unwrap();
        add_play_time(&db, &src.id, 1000).unwrap();
        let before = get(&db, &src.id).unwrap();

        let copy = duplicate_row(&db, &paths, &src.id, 200).unwrap();
        assert_ne!(copy.id, src.id);
        assert_ne!(copy.dir_name, src.dir_name);
        assert_eq!(copy.dir_name, "World (2)");
        assert_eq!(copy.name, "World");
        assert_eq!(copy.mc_version, "1.21.5");
        // コピーが終わるまではインストール中として置く
        assert_eq!(copy.state, InstanceState::Installing);
        assert_eq!(copy.created_at, 200);
        assert_eq!(copy.last_played_at, None);
        assert_eq!(copy.play_time_ms, 0);
        assert_eq!(copy.memory_mode, MemoryMode::Manual);
        assert_eq!(copy.memory_mb, Some(4096));
        assert_eq!(copy.java_mode, JavaMode::Manual);
        assert_eq!(copy.java_path.as_deref(), Some("/opt/java/bin/java"));
        assert_eq!(copy.jvm_args, "-Dfoo=bar");
        assert_eq!(get(&db, &copy.id).unwrap(), copy);
        // ディレクトリは作らず、元は変わらない
        assert!(!game_dir(&paths, &copy).exists());
        assert_eq!(get(&db, &src.id).unwrap(), before);
    }

    #[test]
    fn duplicate_row_is_installing_whatever_the_source_state() {
        let (_t, paths, db) = setup();
        let src = create(&db, &paths, "x", "1.21.5", 1).unwrap();
        for state in [
            InstanceState::Installing,
            InstanceState::Ready,
            InstanceState::Broken,
        ] {
            set_state(&db, &src.id, state).unwrap();
            let copy = duplicate_row(&db, &paths, &src.id, 2).unwrap();
            assert_eq!(copy.state, InstanceState::Installing);
            assert_eq!(get(&db, &copy.id).unwrap().state, InstanceState::Installing);
            assert_eq!(get(&db, &src.id).unwrap().state, state);
        }
    }

    #[test]
    fn duplicate_row_of_missing_is_not_found() {
        let (_t, paths, db) = setup();
        let err = duplicate_row(&db, &paths, "nope", 1).unwrap_err();
        assert_eq!(err.code(), "instance.not_found");
    }

    #[test]
    fn update_settings_for_missing_instance_is_not_found() {
        let (_t, _paths, db) = setup();
        let err = update_settings(&db, "nope", &settings("x")).unwrap_err();
        assert_eq!(err.code(), "instance.not_found");
    }
}
