//! 設定の同期（共通設定）。保存層、登録表、options.txt の読み書きなど。

pub mod apply;
pub mod options;
pub mod proposals;
pub mod registry;
pub mod store;
pub mod version;
pub mod wire;

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use ts_rs::TS;

use crate::db::Db;
use crate::error::CoreError;
use crate::instance;
use crate::paths::AppPaths;

use self::options::OptionsFile;
use self::registry::{Category, KeyCodec, field_for_key, key_codec};
use self::store::{Membership, SyncProfile};

/// 同期の処理が使う入口。DB とアプリのパスを借りる。
pub struct SyncCtx<'a> {
    pub db: &'a Db,
    pub paths: &'a AppPaths,
}

/// 参加のときに、インスタンスの値をどちらに合わせるか。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum JoinMode {
    /// 共通設定に合わせる。値は書き換えず、次回の起動前に適用する。
    KeepProfile,
    /// このインスタンスの値で共通設定を更新する。
    AdoptInstance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinReport {
    /// インスタンスの現在値と共通設定で食い違う項目の数。
    pub differing: usize,
}

pub(crate) const ALL_CATEGORIES: [Category; 6] = [
    Category::Keys,
    Category::Mouse,
    Category::Sound,
    Category::Graphics,
    Category::Chat,
    Category::Accessibility,
];

fn options_path(ctx: &SyncCtx, instance_id: &str) -> Result<std::path::PathBuf, CoreError> {
    let inst = instance::get(ctx.db, instance_id)?;
    Ok(instance::game_dir(ctx.paths, &inst).join("options.txt"))
}

/// options.txt を UTF-8 として厳密に読む。無ければ `None`。UTF-8 でなければ読めない扱い。
fn read_options(path: &Path) -> Result<Option<OptionsFile>, CoreError> {
    let io = |source: std::io::Error| CoreError::Io {
        path: path.display().to_string(),
        source,
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io(e)),
    };
    let text = String::from_utf8(bytes)
        .map_err(|e| io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
    Ok(Some(OptionsFile::parse(&text)))
}

/// 登録表にある項目の値を、読んだままの文字列で取り出す。
/// キー割り当てのうち、`key.keyboard.*` / `key.mouse.*` の書式でないもの（旧式の整数コードを含む）は入れない。
fn registered_values(file: &OptionsFile, categories: &[Category]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (key, value) in file.entries() {
        let Some(field) = field_for_key(key) else {
            continue;
        };
        if !categories.contains(&field.category) {
            continue;
        }
        if field.category == Category::Keys && key_codec(value) != KeyCodec::Keyboard {
            continue;
        }
        out.insert(field.id.to_string(), value.to_string());
    }
    out
}

pub(crate) fn find_profile(ctx: &SyncCtx, profile_id: &str) -> Result<SyncProfile, CoreError> {
    store::list_profiles(ctx.db)?
        .into_iter()
        .find(|p| p.id == profile_id)
        .ok_or(CoreError::SyncProfileNotFound)
}

/// 共通設定の名前の上限（文字数）。
const MAX_NAME_CHARS: usize = 60;

/// 前後の空白を除き、空や長すぎる名前を `SyncInvalidName` にする。
fn validate_name(name: &str) -> Result<String, CoreError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(CoreError::SyncInvalidName);
    }
    Ok(name.to_string())
}

pub fn rename_profile(ctx: &SyncCtx, profile_id: &str, name: &str) -> Result<(), CoreError> {
    let name = validate_name(name)?;
    find_profile(ctx, profile_id)?;
    store::rename_profile(ctx.db, profile_id, &name)
}

/// 共通設定の有効なカテゴリの値のうち、インスタンスの値と食い違う件数。
/// インスタンスに行が無い項目は書き換えの対象外なので数えない。
fn count_differing(
    profile: &SyncProfile,
    profile_values: &BTreeMap<String, String>,
    file: &OptionsFile,
) -> usize {
    let current = registered_values(file, &profile.categories);
    profile_values
        .iter()
        .filter(|(id, value)| current.get(*id).is_some_and(|mine| mine != *value))
        .count()
}

/// インスタンスの options.txt から共通設定を作り、そのインスタンスを参加させる。
/// options.txt が無ければ何も作らず `SyncNoOptions`。
pub fn create_profile_from_instance(
    ctx: &SyncCtx,
    instance_id: &str,
    name: &str,
) -> Result<SyncProfile, CoreError> {
    let name = validate_name(name)?;
    let path = options_path(ctx, instance_id)?;
    let file = read_options(&path)?.ok_or(CoreError::SyncNoOptions)?;
    let values: Vec<(String, String)> = registered_values(&file, &ALL_CATEGORIES)
        .into_iter()
        .collect();
    // 作成・値の保存・参加は 1 つのトランザクション。失敗しても空の共通設定は残らない
    store::create_profile_joined(ctx.db, &name, &ALL_CATEGORIES, &values, instance_id)
}

/// 参加させたときの差の件数。書き込まない。options.txt が無ければ 0。
pub fn preview_join(
    ctx: &SyncCtx,
    instance_id: &str,
    profile_id: &str,
) -> Result<usize, CoreError> {
    let path = options_path(ctx, instance_id)?;
    let profile = find_profile(ctx, profile_id)?;
    let Some(file) = read_options(&path)? else {
        return Ok(0);
    };
    let values = store::values(ctx.db, profile_id)?;
    Ok(count_differing(&profile, &values, &file))
}

pub fn join_instance(
    ctx: &SyncCtx,
    instance_id: &str,
    profile_id: &str,
    mode: JoinMode,
) -> Result<JoinReport, CoreError> {
    let path = options_path(ctx, instance_id)?;
    let profile = find_profile(ctx, profile_id)?;
    let file = read_options(&path)?;
    let differing = match &file {
        Some(file) => count_differing(&profile, &store::values(ctx.db, profile_id)?, file),
        None => 0,
    };
    match mode {
        JoinMode::KeepProfile => store::join(ctx.db, instance_id, profile_id)?,
        JoinMode::AdoptInstance => {
            let file = file.ok_or(CoreError::SyncNoOptions)?;
            // 同じ共通設定に参加済みなら、その除外項目は取り込まない（移るときは除外が空に戻る）
            let excluded = store::membership(ctx.db, instance_id)?
                .filter(|m| m.profile_id == profile_id)
                .map(|m| m.excluded)
                .unwrap_or_default();
            let current = store::values(ctx.db, profile_id)?;
            // 実際に値が変わる項目だけ。差が無ければ revision を上げない
            let values: Vec<(String, String)> = registered_values(&file, &profile.categories)
                .into_iter()
                .filter(|(id, value)| !excluded.contains(id) && current.get(id) != Some(value))
                .collect();
            store::join(ctx.db, instance_id, profile_id)?;
            if !values.is_empty() {
                store::set_values(ctx.db, profile_id, &values)?;
            }
        }
    }
    Ok(JoinReport { differing })
}

pub fn leave_instance(ctx: &SyncCtx, instance_id: &str) -> Result<(), CoreError> {
    store::leave(ctx.db, instance_id)
}

pub fn set_categories(
    ctx: &SyncCtx,
    profile_id: &str,
    categories: &[Category],
) -> Result<(), CoreError> {
    store::set_categories(ctx.db, profile_id, categories)
}

pub fn set_excluded(
    ctx: &SyncCtx,
    instance_id: &str,
    excluded: &[String],
) -> Result<(), CoreError> {
    store::set_excluded(ctx.db, instance_id, excluded)
}

pub fn set_override(
    ctx: &SyncCtx,
    instance_id: &str,
    field_id: &str,
    value: Option<&str>,
) -> Result<(), CoreError> {
    store::set_override(ctx.db, instance_id, field_id, value)
}

/// 参加中の共通設定。無ければ `None`。
pub fn membership(ctx: &SyncCtx, instance_id: &str) -> Result<Option<Membership>, CoreError> {
    store::membership(ctx.db, instance_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::NewInstance;

    struct Env {
        _t: tempfile::TempDir,
        paths: AppPaths,
        db: Db,
    }

    impl Env {
        fn new() -> Env {
            let t = tempfile::tempdir().unwrap();
            let paths = AppPaths::from_root(t.path().join("data"));
            paths.ensure_dirs().unwrap();
            Env {
                _t: t,
                paths,
                db: Db::open_in_memory().unwrap(),
            }
        }

        fn ctx(&self) -> SyncCtx<'_> {
            SyncCtx {
                db: &self.db,
                paths: &self.paths,
            }
        }

        fn instance(&self, name: &str, options: Option<&[u8]>) -> String {
            let inst = instance::create(
                &self.db,
                &self.paths,
                &NewInstance::vanilla(name, "1.21.5"),
                1,
            )
            .unwrap();
            if let Some(bytes) = options {
                std::fs::write(self.options(&inst.id), bytes).unwrap();
            }
            inst.id
        }

        fn options(&self, id: &str) -> std::path::PathBuf {
            let inst = instance::get(&self.db, id).unwrap();
            instance::game_dir(&self.paths, &inst).join("options.txt")
        }
    }

    const SAMPLE: &str = "version:4325\nfov:0.5\nmouseSensitivity:0.3\nlastServer:x\nkey_key.jump:key.keyboard.space\nresourcePacks:[\"a\"]\n";

    #[test]
    fn create_from_instance_copies_registered_values_only() {
        let env = Env::new();
        let id = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = create_profile_from_instance(&env.ctx(), &id, "共通").unwrap();
        let values = store::values(&env.db, &p.id).unwrap();
        assert_eq!(
            values,
            BTreeMap::from([
                ("fov".to_string(), "0.5".to_string()),
                ("mouseSensitivity".to_string(), "0.3".to_string()),
                ("key_key.jump".to_string(), "key.keyboard.space".to_string()),
            ])
        );
        assert_eq!(p.revision, 1);
        assert_eq!(store::list_profiles(&env.db).unwrap(), vec![p.clone()]);
        assert_eq!(
            membership(&env.ctx(), &id).unwrap().unwrap().profile_id,
            p.id
        );
    }

    #[test]
    fn create_without_options_file_fails_and_creates_nothing() {
        let env = Env::new();
        let id = env.instance("A", None);
        let err = create_profile_from_instance(&env.ctx(), &id, "共通").unwrap_err();
        assert_eq!(err.code(), "sync.no_options");
        assert!(store::list_profiles(&env.db).unwrap().is_empty());
        assert!(membership(&env.ctx(), &id).unwrap().is_none());
    }

    #[test]
    fn create_skips_unknown_and_lwjgl2_key_codecs() {
        let env = Env::new();
        let id = env.instance(
            "A",
            Some(
                b"key_key.jump:scancode.4
key_key.use:key.mouse.right
key_key.drop:16
",
            ),
        );
        let p = create_profile_from_instance(&env.ctx(), &id, "共通").unwrap();
        let values = store::values(&env.db, &p.id).unwrap();
        // 旧式の整数コード（drop）も判定できない書式（jump）も入れない
        assert_eq!(
            values.keys().collect::<Vec<_>>(),
            vec![&"key_key.use".to_string()]
        );
    }

    #[test]
    fn adopt_skips_lwjgl2_key_codec() {
        let env = Env::new();
        let id = env.instance(
            "A",
            Some(
                b"fov:0.5
key_key.drop:16
key_key.jump:key.keyboard.space
",
            ),
        );
        let p = store::create_profile(&env.db, "P", &ALL_CATEGORIES).unwrap();
        join_instance(&env.ctx(), &id, &p.id, JoinMode::AdoptInstance).unwrap();
        let values = store::values(&env.db, &p.id).unwrap();
        assert!(!values.contains_key("key_key.drop"));
        assert!(values.contains_key("key_key.jump") && values.contains_key("fov"));
    }

    #[test]
    fn preview_ignores_lwjgl2_key_values() {
        let env = Env::new();
        let id = env.instance(
            "A",
            Some(
                b"key_key.drop:16
fov:0.5
",
            ),
        );
        let p = store::create_profile(&env.db, "P", &ALL_CATEGORIES).unwrap();
        store::set_values(
            &env.db,
            &p.id,
            &[
                ("key_key.drop".to_string(), "17".to_string()),
                ("fov".to_string(), "0.9".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(preview_join(&env.ctx(), &id, &p.id).unwrap(), 1);
    }

    #[test]
    fn profile_names_are_trimmed_and_validated() {
        let env = Env::new();
        let id = env.instance("A", Some(SAMPLE.as_bytes()));
        for bad in ["", "   ", &"あ".repeat(61)] {
            let err = create_profile_from_instance(&env.ctx(), &id, bad).unwrap_err();
            assert_eq!(err.code(), "sync.invalid_name");
        }
        assert!(store::list_profiles(&env.db).unwrap().is_empty());
        let p = create_profile_from_instance(&env.ctx(), &id, &format!("  {}  ", "あ".repeat(60)))
            .unwrap();
        assert_eq!(p.name, "あ".repeat(60));
        assert_eq!(
            rename_profile(&env.ctx(), &p.id, " 	").unwrap_err().code(),
            "sync.invalid_name"
        );
        assert_eq!(
            rename_profile(&env.ctx(), &p.id, &"x".repeat(61))
                .unwrap_err()
                .code(),
            "sync.invalid_name"
        );
        rename_profile(&env.ctx(), &p.id, "  新  ").unwrap();
        assert_eq!(store::list_profiles(&env.db).unwrap()[0].name, "新");
        assert_eq!(
            rename_profile(&env.ctx(), "nope", "x").unwrap_err().code(),
            "sync.profile_not_found"
        );
    }

    #[test]
    fn non_utf8_options_is_an_error_not_lossy() {
        let env = Env::new();
        let id = env.instance("A", Some(b"fov:0.5\nlastServer:\xff\n"));
        let err = create_profile_from_instance(&env.ctx(), &id, "共通").unwrap_err();
        assert_eq!(err.code(), "io.failed");
        assert!(store::list_profiles(&env.db).unwrap().is_empty());
    }

    fn profile_with_fov(env: &Env, fov: &str) -> SyncProfile {
        let p = store::create_profile(&env.db, "P", &ALL_CATEGORIES).unwrap();
        store::set_values(&env.db, &p.id, &[("fov".to_string(), fov.to_string())]).unwrap();
        p
    }

    #[test]
    fn join_keep_profile_does_not_touch_options_file() {
        let env = Env::new();
        let id = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = profile_with_fov(&env, "0.9");
        let report = join_instance(&env.ctx(), &id, &p.id, JoinMode::KeepProfile).unwrap();
        assert_eq!(report.differing, 1);
        assert_eq!(std::fs::read_to_string(env.options(&id)).unwrap(), SAMPLE);
        assert_eq!(store::values(&env.db, &p.id).unwrap()["fov"], "0.9");
        assert_eq!(
            membership(&env.ctx(), &id).unwrap().unwrap().profile_id,
            p.id
        );
        // options.txt が無いインスタンスは差 0 で参加できる
        let bare = env.instance("B", None);
        let report = join_instance(&env.ctx(), &bare, &p.id, JoinMode::KeepProfile).unwrap();
        assert_eq!(report.differing, 0);
    }

    #[test]
    fn join_adopt_updates_profile_and_bumps_revision() {
        let env = Env::new();
        let id = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = profile_with_fov(&env, "0.9");
        let before = store::list_profiles(&env.db).unwrap()[0].revision;
        let report = join_instance(&env.ctx(), &id, &p.id, JoinMode::AdoptInstance).unwrap();
        assert_eq!(report.differing, 1);
        assert_eq!(store::values(&env.db, &p.id).unwrap()["fov"], "0.5");
        assert_eq!(
            store::list_profiles(&env.db).unwrap()[0].revision,
            before + 1
        );
        assert_eq!(std::fs::read_to_string(env.options(&id)).unwrap(), SAMPLE);
        let bare = env.instance("B", None);
        let err = join_instance(&env.ctx(), &bare, &p.id, JoinMode::AdoptInstance).unwrap_err();
        assert_eq!(err.code(), "sync.no_options");
        assert!(membership(&env.ctx(), &bare).unwrap().is_none());
    }

    #[test]
    fn adopt_skips_excluded_fields_and_keeps_revision_when_nothing_changes() {
        let env = Env::new();
        let id = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = profile_with_fov(&env, "0.9");
        join_instance(&env.ctx(), &id, &p.id, JoinMode::KeepProfile).unwrap();
        set_excluded(&env.ctx(), &id, &["fov".to_string()]).unwrap();
        join_instance(&env.ctx(), &id, &p.id, JoinMode::AdoptInstance).unwrap();
        let values = store::values(&env.db, &p.id).unwrap();
        // 除外した fov は取り込まない。他の項目は取り込む
        assert_eq!(values["fov"], "0.9");
        assert_eq!(values["mouseSensitivity"], "0.3");
        // 差が無いなら revision は動かない
        let rev = store::list_profiles(&env.db).unwrap()[0].revision;
        join_instance(&env.ctx(), &id, &p.id, JoinMode::AdoptInstance).unwrap();
        assert_eq!(store::list_profiles(&env.db).unwrap()[0].revision, rev);
    }

    #[test]
    fn preview_counts_differing_values() {
        let env = Env::new();
        let id = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = store::create_profile(&env.db, "P", &ALL_CATEGORIES).unwrap();
        store::set_values(
            &env.db,
            &p.id,
            &[
                ("fov".to_string(), "0.9".to_string()),
                ("mouseSensitivity".to_string(), "0.3".to_string()),
                ("gamma".to_string(), "1.0".to_string()),
            ],
        )
        .unwrap();
        // fov だけが食い違う。mouseSensitivity は同じ、gamma はインスタンスに行が無い
        assert_eq!(preview_join(&env.ctx(), &id, &p.id).unwrap(), 1);
        assert!(membership(&env.ctx(), &id).unwrap().is_none());
        set_categories(&env.ctx(), &p.id, &[Category::Mouse]).unwrap();
        assert_eq!(preview_join(&env.ctx(), &id, &p.id).unwrap(), 0);
    }

    #[test]
    fn leaving_removes_membership_and_keeps_profile() {
        let env = Env::new();
        let id = env.instance("A", Some(SAMPLE.as_bytes()));
        let p = create_profile_from_instance(&env.ctx(), &id, "共通").unwrap();
        leave_instance(&env.ctx(), &id).unwrap();
        assert!(membership(&env.ctx(), &id).unwrap().is_none());
        assert_eq!(store::list_profiles(&env.db).unwrap(), vec![p]);
    }
}
