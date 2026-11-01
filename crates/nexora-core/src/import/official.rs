//! 公式ランチャーのプロファイルを読む。保存形式は `docs/research/07-official-launcher-format.md` にある。

use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::copy::is_link;
use super::{ImportKind, ImportSource, SourceInstance, Unsupported, UnsupportedCode};
use crate::error::CoreError;
use crate::meta::loader::LoaderKind;
use crate::mrpack::safe_path::check_rel_path;

/// プロファイルの一覧。Microsoft Store 版のランチャーは接尾辞つきの名前を使うことがあるため、後者も探す。
const PROFILE_FILES: [&str; 2] = [
    "launcher_profiles.json",
    "launcher_profiles_microsoft_store.json",
];
/// `launcher_profiles.json` の大きさの上限。アイコンが埋め込まれて大きくなるが、無制限には読まない。
const MAX_PROFILES_BYTES: u64 = 64 * 1024 * 1024;
/// `versions/<id>/<id>.json` の大きさの上限。
const MAX_VERSION_BYTES: u64 = 16 * 1024 * 1024;

/// 公式ランチャーのデータフォルダ（`launcher_profiles.json` があるところ）。
/// プロファイルごとにフォルダは分かれない。`gameDir` を持たないプロファイルは、このフォルダそのものを使う。
pub struct OfficialLauncher {
    root: PathBuf,
}

fn unavailable(reason: impl Into<String>) -> CoreError {
    CoreError::ImportUnavailable {
        reason: reason.into(),
    }
}

/// 版の ID から読み取れた、Minecraft の版とローダー。
#[derive(Debug, PartialEq, Eq)]
struct Resolved {
    mc_version: String,
    loader: LoaderKind,
    loader_version: Option<String>,
}

impl OfficialLauncher {
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// 既定の場所に `launcher_profiles.json` があれば、そのフォルダ。
    pub fn locate() -> Option<PathBuf> {
        let mut candidates = Vec::new();
        if cfg!(target_os = "windows") {
            candidates.push(dirs::data_dir()?.join(".minecraft"));
        } else if cfg!(target_os = "macos") {
            candidates.push(dirs::data_dir()?.join("minecraft"));
        } else {
            let home = dirs::home_dir()?;
            candidates.push(home.join(".minecraft"));
            candidates.push(home.join(".var/app/com.mojang.Minecraft/.minecraft"));
        }
        candidates
            .into_iter()
            .find(|dir| PROFILE_FILES.iter().any(|f| dir.join(f).is_file()))
    }

    /// `profiles` の中身。ファイルが無い、読めない、形が違うときは `ImportUnavailable`。
    fn read_profiles(&self) -> Result<serde_json::Map<String, Value>, CoreError> {
        let Some(path) = PROFILE_FILES
            .iter()
            .map(|f| self.root.join(f))
            .find(|p| p.is_file())
        else {
            return Err(unavailable(format!(
                "launcher_profiles.json was not found in {}",
                self.root.display()
            )));
        };
        let text = read_limited(&path, MAX_PROFILES_BYTES)
            .map_err(|e| unavailable(format!("{}: {e}", path.display())))?;
        let value: Value = serde_json::from_str(&text)
            .map_err(|e| unavailable(format!("{} could not be read: {e}", path.display())))?;
        match value.get("profiles") {
            Some(Value::Object(map)) => Ok(map.clone()),
            _ => Err(unavailable(format!("{} has no profiles", path.display()))),
        }
    }

    /// プロファイルのゲームフォルダ。
    /// `gameDir` はユーザーが選んだ絶対パスで、読むだけなので起動元のフォルダの中に収まる必要はない。
    /// ただし相対パス（基準が確かめられない）、`..` を含むもの、リンクは断る。
    fn game_dir(&self, profile: &Value) -> Result<PathBuf, Unsupported> {
        let Some(dir) = profile
            .get("gameDir")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|d| !d.is_empty())
        else {
            return Ok(self.root.clone());
        };
        let path = PathBuf::from(dir);
        if !path.is_absolute() || path.components().any(|c| c == Component::ParentDir) {
            Err(Unsupported::with(UnsupportedCode::UnsafeFolder, dir))
        } else if is_link(&path) {
            Err(Unsupported::with(UnsupportedCode::LinkedFolder, dir))
        } else {
            Ok(path)
        }
    }

    /// `versions/<id>/<id>.json` から、Minecraft の版とローダーを読む。読めなければ `None`。
    fn resolve_from_json(&self, id: &str) -> Option<Resolved> {
        let rel = check_rel_path(id).ok()?;
        let path = self
            .root
            .join("versions")
            .join(&rel)
            .join(format!("{id}.json"));
        if is_link(&path) || !path.is_file() {
            return None;
        }
        let value: Value =
            serde_json::from_str(&read_limited(&path, MAX_VERSION_BYTES).ok()?).ok()?;
        let mc = value.get("inheritsFrom")?.as_str()?;
        if !is_release(mc) {
            return None;
        }
        value.get("libraries")?.as_array()?.iter().find_map(|lib| {
            let mut parts = lib.get("name")?.as_str()?.splitn(3, ':');
            let (group, artifact, version) = (parts.next()?, parts.next()?, parts.next()?);
            let (loader, version) = match (group, artifact) {
                ("net.fabricmc", "fabric-loader") => (LoaderKind::Fabric, version),
                ("org.quiltmc", "quilt-loader") => (LoaderKind::Quilt, version),
                ("net.minecraftforge", "forge") => (
                    LoaderKind::Forge,
                    version.strip_prefix(&format!("{mc}-")).unwrap_or(version),
                ),
                ("net.neoforged", "neoforge") => (LoaderKind::Neoforge, version),
                _ => return None,
            };
            Some(Resolved {
                mc_version: mc.to_string(),
                loader,
                loader_version: Some(version.to_string()),
            })
        })
    }

    fn resolve(&self, profile: &Value) -> Result<Resolved, Unsupported> {
        let id = profile
            .get("lastVersionId")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if id.is_empty() {
            return Err(Unsupported::new(UnsupportedCode::MissingVersion));
        }
        if id.starts_with("latest-") {
            return Err(Unsupported::new(UnsupportedCode::LatestVersion));
        }
        match self.resolve_from_json(id) {
            Some(resolved) => Ok(resolved),
            None => parse_version_id(id),
        }
    }
}

/// 大きさの上限つきで UTF-8 のファイルを読む。
fn read_limited(path: &Path, limit: u64) -> std::io::Result<String> {
    if fs::metadata(path)?.len() > limit {
        return Err(std::io::Error::other("the file is too large"));
    }
    fs::read_to_string(path)
}

/// `1.21`、`1.21.1` の形の正式版か。スナップショットや旧版（`b1.7.3` など）は含めない。
fn is_release(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    (2..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn release(version: &str, id: &str) -> Result<String, Unsupported> {
    if is_release(version) {
        Ok(version.to_string())
    } else {
        Err(Unsupported::with(UnsupportedCode::UnsupportedVersion, id))
    }
}

/// 版の ID の名前の形から、Minecraft の版とローダーを読む。
fn parse_version_id(id: &str) -> Result<Resolved, Unsupported> {
    let resolved = |mc: String, loader, version: &str| Resolved {
        mc_version: mc,
        loader,
        loader_version: Some(version.to_string()),
    };
    for (prefix, loader) in [
        ("fabric-loader-", LoaderKind::Fabric),
        ("quilt-loader-", LoaderKind::Quilt),
    ] {
        if let Some(rest) = id.strip_prefix(prefix) {
            let (loader_version, mc) = rest
                .split_once('-')
                .filter(|(l, m)| !l.is_empty() && !m.is_empty())
                .ok_or_else(|| Unsupported::new(UnsupportedCode::MissingVersion))?;
            return Ok(resolved(release(mc, id)?, loader, loader_version));
        }
    }
    if let Some(version) = id.strip_prefix("neoforge-") {
        // NeoForge の版は `<Minecraft の 2 番目>.<3 番目>.<ビルド>`。26.1 以降は表記が変わった。
        let mut nums = version.split('.');
        let major: u32 = nums.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        let minor: u32 = nums.next().and_then(|n| n.parse().ok()).unwrap_or(u32::MAX);
        if !(20..26).contains(&major) || minor == u32::MAX || nums.next().is_none() {
            return Err(Unsupported::with(
                UnsupportedCode::UnsupportedVersion,
                version,
            ));
        }
        let mc = if minor == 0 {
            format!("1.{major}")
        } else {
            format!("1.{major}.{minor}")
        };
        return Ok(resolved(mc, LoaderKind::Neoforge, version));
    }
    if let Some((mc, rest)) = id.split_once("-forge") {
        let mc = release(mc, id)?;
        let rest = rest.strip_prefix('-').unwrap_or(rest);
        let build = rest.strip_prefix(&format!("{mc}-")).unwrap_or(rest);
        if build.is_empty() {
            return Err(Unsupported::with(UnsupportedCode::MissingLoaderVersion, id));
        }
        return Ok(resolved(mc, LoaderKind::Forge, build));
    }
    if is_release(id) {
        return Ok(Resolved {
            mc_version: id.to_string(),
            loader: LoaderKind::Vanilla,
            loader_version: None,
        });
    }
    Err(Unsupported::with(UnsupportedCode::UnsupportedVersion, id))
}

/// 部分（フォルダまたはファイル）に、取り込めるものが実際にあるか。フォルダなら中に何か入っていること。
fn has_part(base: &Path, part: &str) -> bool {
    let path = base.join(part);
    !is_link(&path)
        && (path.is_file()
            || fs::read_dir(&path)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(false))
}

impl ImportSource for OfficialLauncher {
    fn list(&self) -> Result<Vec<SourceInstance>, CoreError> {
        let mut rows: Vec<SourceInstance> = self
            .read_profiles()?
            .iter()
            .map(|(key, profile)| {
                let name = profile
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .unwrap_or(key)
                    .to_string();
                let checked = if check_rel_path(key).is_err() {
                    Err(Unsupported::with(UnsupportedCode::UnsafeName, key))
                } else if !profile.is_object() {
                    Err(Unsupported::with(UnsupportedCode::UnreadableMetadata, key))
                } else {
                    self.game_dir(profile)
                        .and_then(|dir| self.resolve(profile).map(|r| (dir, r)))
                };
                match checked {
                    Ok((dir, resolved)) => SourceInstance {
                        id: key.clone(),
                        name,
                        mc_version: resolved.mc_version,
                        loader: resolved.loader,
                        loader_version: resolved.loader_version,
                        available: ImportKind::ALL
                            .into_iter()
                            .filter(|k| k.parts().iter().any(|part| has_part(&dir, part)))
                            .collect(),
                        unsupported: None,
                    },
                    Err(reason) => SourceInstance {
                        id: key.clone(),
                        name,
                        mc_version: String::new(),
                        loader: LoaderKind::Vanilla,
                        loader_version: None,
                        available: Vec::new(),
                        unsupported: Some(reason),
                    },
                }
            })
            .collect();
        rows.sort_by_cached_key(|r| (r.name.to_lowercase(), r.id.clone()));
        Ok(rows)
    }

    fn source_path(&self, id: &str, part: &str) -> Result<Option<PathBuf>, CoreError> {
        check_rel_path(id)?;
        let profiles = self.read_profiles()?;
        let Some(profile) = profiles.get(id) else {
            return Ok(None);
        };
        let Ok(dir) = self.game_dir(profile) else {
            return Ok(None);
        };
        let path = dir.join(part);
        Ok((path.exists() && !is_link(&path)).then_some(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::ImportKind;
    use crate::meta::loader::LoaderKind;
    use std::fs;
    use std::path::Path;

    fn profiles(root: &Path, json: &str) {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("launcher_profiles.json"), json).unwrap();
    }

    fn file(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    fn one(version: &str) -> String {
        format!(
            r#"{{"profiles":{{"k":{{"name":"P","type":"custom","lastVersionId":"{version}"}}}}}}"#
        )
    }

    fn resolved(version: &str) -> SourceInstance {
        let tmp = tempfile::tempdir().unwrap();
        profiles(tmp.path(), &one(version));
        OfficialLauncher::at(tmp.path().to_path_buf())
            .list()
            .unwrap()
            .remove(0)
    }

    fn first(root: &Path) -> SourceInstance {
        OfficialLauncher::at(root.to_path_buf())
            .list()
            .unwrap()
            .remove(0)
    }

    #[test]
    fn lists_one_row_per_profile_with_the_content_of_its_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tempfile::tempdir().unwrap();
        profiles(
            tmp.path(),
            &format!(
                r#"{{"profiles":{{
                    "abc123":{{"name":"Survival","type":"custom","lastVersionId":"fabric-loader-0.16.10-1.21.1",
                        "gameDir":{}}},
                    "def456":{{"name":"Alpha","type":"custom","lastVersionId":"1.20.4"}}}},
                    "authenticationDatabase":{{"secret":"x"}}}}"#,
                serde_json::to_string(game.path().to_str().unwrap()).unwrap()
            ),
        );
        file(&game.path().join("mods/a.jar"));
        file(&game.path().join("saves/w/level.dat"));
        fs::create_dir_all(game.path().join("screenshots")).unwrap();
        let list = OfficialLauncher::at(tmp.path().to_path_buf())
            .list()
            .unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Alpha");
        assert_eq!(list[0].id, "def456");
        assert_eq!(list[0].loader, LoaderKind::Vanilla);
        let s = &list[1];
        assert_eq!((s.id.as_str(), s.name.as_str()), ("abc123", "Survival"));
        assert_eq!(s.mc_version, "1.21.1");
        assert_eq!(s.loader, LoaderKind::Fabric);
        assert_eq!(s.loader_version.as_deref(), Some("0.16.10"));
        assert_eq!(s.available, [ImportKind::Mods, ImportKind::Saves]);
        assert_eq!(s.unsupported, None);
    }

    #[test]
    fn every_loader_id_pattern_is_resolved() {
        let v = resolved("1.21.1");
        assert_eq!(
            (v.loader, v.mc_version.as_str()),
            (LoaderKind::Vanilla, "1.21.1")
        );
        assert_eq!(v.loader_version, None);
        let q = resolved("quilt-loader-0.26.4-1.21");
        assert_eq!(
            (q.loader, q.mc_version.as_str()),
            (LoaderKind::Quilt, "1.21")
        );
        assert_eq!(q.loader_version.as_deref(), Some("0.26.4"));
        let f = resolved("1.20.1-forge-47.2.0");
        assert_eq!(
            (f.loader, f.mc_version.as_str()),
            (LoaderKind::Forge, "1.20.1")
        );
        assert_eq!(f.loader_version.as_deref(), Some("47.2.0"));
        let old = resolved("1.12.2-forge1.12.2-14.23.5.2860");
        assert_eq!(
            (old.loader, old.mc_version.as_str()),
            (LoaderKind::Forge, "1.12.2")
        );
        assert_eq!(old.loader_version.as_deref(), Some("14.23.5.2860"));
        let n = resolved("neoforge-21.1.50");
        assert_eq!(
            (n.loader, n.mc_version.as_str()),
            (LoaderKind::Neoforge, "1.21.1")
        );
        assert_eq!(n.loader_version.as_deref(), Some("21.1.50"));
        let n0 = resolved("neoforge-21.0.167");
        assert_eq!(n0.mc_version, "1.21");
        let n4 = resolved("neoforge-20.4.237");
        assert_eq!(n4.mc_version, "1.20.4");
        for i in [&v, &q, &f, &old, &n, &n0, &n4] {
            assert_eq!(i.unsupported, None, "{i:?}");
        }
    }

    #[test]
    fn the_reason_is_a_code_with_the_value_to_show() {
        let code_and_detail = |id: &str| {
            let u = resolved(id).unsupported.unwrap();
            (u.code, u.detail)
        };
        assert_eq!(
            code_and_detail("latest-release"),
            (UnsupportedCode::LatestVersion, None)
        );
        assert_eq!(code_and_detail(""), (UnsupportedCode::MissingVersion, None));
        assert_eq!(
            code_and_detail("24w14a"),
            (
                UnsupportedCode::UnsupportedVersion,
                Some("24w14a".to_string())
            )
        );
        assert_eq!(
            code_and_detail("1.20.1-forge"),
            (
                UnsupportedCode::MissingLoaderVersion,
                Some("1.20.1-forge".to_string())
            )
        );
    }

    #[test]
    fn versions_that_cannot_be_resolved_say_why() {
        for id in [
            "latest-release",
            "latest-snapshot",
            "24w14a",
            "1.21-pre1",
            "b1.7.3",
            "my-custom-thing",
            "neoforge-26.1.0.5-beta",
            "fabric-loader-0.16.10",
            "",
        ] {
            let i = resolved(id);
            assert!(i.unsupported.is_some(), "{id}: {i:?}");
            assert!(i.available.is_empty());
        }
        let tmp = tempfile::tempdir().unwrap();
        profiles(
            tmp.path(),
            r#"{"profiles":{"a":{"type":"latest-release"},"b":"junk","c":null}}"#,
        );
        let list = OfficialLauncher::at(tmp.path().to_path_buf())
            .list()
            .unwrap();
        assert_eq!(list.len(), 3);
        assert!(list.iter().all(|i| i.unsupported.is_some()));
        assert!(list.iter().any(|i| i.name == "a"));
    }

    #[test]
    fn the_version_json_overrides_the_name_pattern() {
        let tmp = tempfile::tempdir().unwrap();
        profiles(tmp.path(), &one("My Pack"));
        let json = tmp.path().join("versions/My Pack/My Pack.json");
        file(&json);
        fs::write(
            &json,
            r#"{"id":"My Pack","inheritsFrom":"1.20.1","libraries":[
                {"name":"org.ow2.asm:asm:9.5"},
                {"name":"net.minecraftforge:forge:1.20.1-47.2.0"}]}"#,
        )
        .unwrap();
        let i = first(tmp.path());
        assert_eq!(
            (i.loader, i.mc_version.as_str()),
            (LoaderKind::Forge, "1.20.1")
        );
        assert_eq!(i.loader_version.as_deref(), Some("47.2.0"));
        assert_eq!(i.unsupported, None);

        fs::write(
            &json,
            r#"{"inheritsFrom":"1.21.1","libraries":[{"name":"net.fabricmc:fabric-loader:0.16.10"}]}"#,
        )
        .unwrap();
        let i = first(tmp.path());
        assert_eq!(
            (i.loader, i.mc_version.as_str()),
            (LoaderKind::Fabric, "1.21.1")
        );

        // 壊れた JSON は無視して名前の形に戻る。
        fs::write(&json, b"{{{").unwrap();
        assert!(first(tmp.path()).unsupported.is_some());
    }

    #[test]
    fn profiles_without_a_game_dir_share_the_launcher_root() {
        let tmp = tempfile::tempdir().unwrap();
        profiles(
            tmp.path(),
            r#"{"profiles":{"a":{"name":"A","lastVersionId":"1.21"},"b":{"name":"B","lastVersionId":"1.20.1"}}}"#,
        );
        file(&tmp.path().join("mods/a.jar"));
        let app = OfficialLauncher::at(tmp.path().to_path_buf());
        let list = app.list().unwrap();
        assert!(list.iter().all(|i| i.available == [ImportKind::Mods]));
        assert_eq!(
            app.source_path("a", "mods").unwrap(),
            Some(tmp.path().join("mods"))
        );
        assert_eq!(
            app.source_path("b", "mods").unwrap(),
            app.source_path("a", "mods").unwrap()
        );
        assert_eq!(app.source_path("a", "saves").unwrap(), None);
    }

    #[test]
    fn a_game_dir_must_be_an_absolute_path_without_parent_steps() {
        let tmp = tempfile::tempdir().unwrap();
        let abs = tmp.path().join("x").join("..").join("y");
        profiles(
            tmp.path(),
            &format!(
                r#"{{"profiles":{{
                    "rel":{{"name":"rel","lastVersionId":"1.21","gameDir":"games/one"}},
                    "up":{{"name":"up","lastVersionId":"1.21","gameDir":{}}},
                    "blank":{{"name":"blank","lastVersionId":"1.21","gameDir":"  "}}}}}}"#,
                serde_json::to_string(abs.to_str().unwrap()).unwrap()
            ),
        );
        let app = OfficialLauncher::at(tmp.path().to_path_buf());
        let list = app.list().unwrap();
        let by = |id: &str| list.iter().find(|i| i.id == id).unwrap();
        assert!(by("rel").unsupported.is_some());
        assert!(by("up").unsupported.is_some());
        assert_eq!(by("blank").unsupported, None);
        assert_eq!(app.source_path("rel", "mods").unwrap(), None);
    }

    #[test]
    fn missing_or_broken_profile_files_are_unavailable() {
        let empty = tempfile::tempdir().unwrap();
        let err = OfficialLauncher::at(empty.path().to_path_buf())
            .list()
            .unwrap_err();
        assert_eq!(err.code(), "import.unavailable");
        for text in [
            "not json",
            "[]",
            r#"{"settings":{}}"#,
            r#"{"profiles":[1]}"#,
        ] {
            let tmp = tempfile::tempdir().unwrap();
            profiles(tmp.path(), text);
            let err = OfficialLauncher::at(tmp.path().to_path_buf())
                .list()
                .unwrap_err();
            assert_eq!(err.code(), "import.unavailable", "{text}");
        }
        let tmp = tempfile::tempdir().unwrap();
        let err = OfficialLauncher::at(tmp.path().to_path_buf())
            .source_path("a", "mods")
            .unwrap_err();
        assert_eq!(err.code(), "import.unavailable");
    }

    #[test]
    fn the_microsoft_store_file_is_read_when_the_plain_one_is_absent() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("launcher_profiles_microsoft_store.json"),
            one("1.21"),
        )
        .unwrap();
        let list = OfficialLauncher::at(tmp.path().to_path_buf())
            .list()
            .unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn unsafe_keys_are_not_importable_and_never_resolve_to_a_path() {
        let tmp = tempfile::tempdir().unwrap();
        profiles(
            tmp.path(),
            r#"{"profiles":{"../escape":{"name":"E","lastVersionId":"1.21"},"ok":{"name":"O","lastVersionId":"1.21"}}}"#,
        );
        file(&tmp.path().join("mods/a.jar"));
        let app = OfficialLauncher::at(tmp.path().to_path_buf());
        let list = app.list().unwrap();
        let bad = list.iter().find(|i| i.id == "../escape").unwrap();
        assert!(bad.unsupported.is_some() && bad.available.is_empty());
        assert_eq!(
            app.source_path("../escape", "mods").unwrap_err().code(),
            "mrpack.unsafe_path"
        );
        assert!(app.source_path("ok", "mods").unwrap().is_some());
        assert_eq!(app.source_path("missing", "mods").unwrap(), None);
    }

    #[test]
    fn links_are_never_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        file(&outside.path().join("mods/a.jar"));
        profiles(tmp.path(), &one("1.21"));
        let link = tmp.path().join("mods");
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(outside.path().join("mods"), &link).is_ok();
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(outside.path().join("mods"), &link).is_ok();
        if !made {
            return; // 権限が無い環境では作れない。
        }
        let app = OfficialLauncher::at(tmp.path().to_path_buf());
        assert!(app.list().unwrap()[0].available.is_empty());
        assert_eq!(app.source_path("k", "mods").unwrap(), None);
    }

    #[test]
    fn reading_leaves_the_original_files_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        profiles(tmp.path(), &one("1.21"));
        file(&tmp.path().join("mods/a.jar"));
        let before = fs::read(tmp.path().join("launcher_profiles.json")).unwrap();
        let app = OfficialLauncher::at(tmp.path().to_path_buf());
        app.list().unwrap();
        app.source_path("k", "mods").unwrap();
        assert_eq!(
            fs::read(tmp.path().join("launcher_profiles.json")).unwrap(),
            before
        );
        let mut names: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(names, ["launcher_profiles.json", "mods"]);
    }
}
