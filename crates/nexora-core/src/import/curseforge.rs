//! CurseForge アプリのインスタンスを読む。保存形式は `docs/research/08-curseforge-app-format.md` にある。
//! 本物のインストールでは確かめていない（公開情報だけが根拠）。

use std::fs;
use std::path::{Path, PathBuf};

use super::copy::is_link;
use super::{ImportKind, ImportSource, SourceInstance};
use crate::error::CoreError;
use crate::meta::loader::LoaderKind;
use crate::mrpack::safe_path::check_rel_path;

const INSTANCES_DIR: &str = "Instances";
const META_FILE: &str = "minecraftinstance.json";

/// CurseForge アプリの Minecraft フォルダ（`Instances` がある所）。
pub struct CurseForgeApp {
    root: PathBuf,
}

impl CurseForgeApp {
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// 既定の場所に `Instances` があれば、その親のフォルダ。
    pub fn locate() -> Option<PathBuf> {
        // 公式の案内では Windows はユーザーフォルダ直下、macOS と Linux は Documents の下。
        let base = if cfg!(windows) {
            dirs::home_dir()?
        } else {
            dirs::document_dir().or_else(|| dirs::home_dir().map(|h| h.join("Documents")))?
        };
        let root = base.join("curseforge").join("minecraft");
        root.join(INSTANCES_DIR).is_dir().then_some(root)
    }

    /// インスタンスのフォルダが並ぶ `Instances`。選んだフォルダが `Instances` そのものでもよい。
    fn instances_dir(&self) -> Result<PathBuf, CoreError> {
        let nested = self.root.join(INSTANCES_DIR);
        if nested.is_dir() {
            return Ok(nested);
        }
        let holds_instances = fs::read_dir(&self.root)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|e| e.path().join(META_FILE).is_file())
            })
            .unwrap_or(false);
        if holds_instances {
            Ok(self.root.clone())
        } else {
            Err(CoreError::ImportUnavailable {
                reason: format!("{} was not found", nested.display()),
            })
        }
    }
}

/// `minecraftinstance.json` から読んだ、取り込みに要る分。
struct Meta {
    name: Option<String>,
    game_version: String,
    loader_name: String,
}

fn read_meta(path: &Path) -> Result<Meta, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("{META_FILE} could not be read: {e}"))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{META_FILE} is not valid JSON: {e}"))?;
    let text_of = |v: Option<&serde_json::Value>| {
        v.and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };
    Ok(Meta {
        name: Some(text_of(json.get("name"))).filter(|n| !n.is_empty()),
        game_version: text_of(json.get("gameVersion")),
        loader_name: text_of(json.get("baseModLoader").and_then(|b| b.get("name"))),
    })
}

/// `forge-47.2.0`、`fabric-0.16.10-1.21.1` のような名前を、ローダーとその版にする。
/// 空ならバニラ。知らない名前や版のない名前は `None`。
fn parse_loader(name: &str) -> Option<(LoaderKind, Option<String>)> {
    if name.is_empty() {
        return Some((LoaderKind::Vanilla, None));
    }
    let (prefix, rest) = name.split_once('-')?;
    if rest.is_empty() {
        return None;
    }
    match prefix.to_ascii_lowercase().as_str() {
        "forge" => Some((LoaderKind::Forge, Some(rest.to_string()))),
        "neoforge" => Some((LoaderKind::Neoforge, Some(rest.to_string()))),
        // 末尾に Minecraft の版が付く
        "fabric" | "quilt" => {
            let version = rest.rsplit_once('-').map_or(rest, |(v, _)| v);
            let kind = if prefix.eq_ignore_ascii_case("fabric") {
                LoaderKind::Fabric
            } else {
                LoaderKind::Quilt
            };
            Some((kind, Some(version.to_string())))
        }
        _ => None,
    }
}

/// 部分（フォルダまたはファイル）に、取り込めるものが実際にあるか。フォルダなら中に何か入っていること。
fn has_part(base: &Path, part: &str) -> bool {
    let path = base.join(part);
    !is_link(&path) && (path.is_file() || has_content(&path))
}

/// フォルダがあり、中に何か入っていて、リンクではないか。
fn has_content(dir: &Path) -> bool {
    !is_link(dir)
        && fs::read_dir(dir)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
}

impl ImportSource for CurseForgeApp {
    fn list(&self) -> Result<Vec<SourceInstance>, CoreError> {
        let dir = self.instances_dir()?;
        let entries = fs::read_dir(&dir).map_err(|e| CoreError::ImportUnavailable {
            reason: format!("{}: {e}", dir.display()),
        })?;
        let mut list = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            // リンクはたどらない。メタデータのないフォルダやファイルはインスタンスではない。
            if is_link(&path) || !path.is_dir() || !path.join(META_FILE).is_file() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            let (meta, mut unsupported) = match read_meta(&path.join(META_FILE)) {
                Ok(meta) => (Some(meta), None),
                Err(reason) => (None, Some(reason)),
            };
            if check_rel_path(&id).is_err() {
                unsupported = Some(format!("{id} is not a safe folder name"));
            }
            let mut loader = None;
            if let Some(meta) = &meta
                && unsupported.is_none()
            {
                loader = parse_loader(&meta.loader_name);
                if loader.is_none() {
                    unsupported = Some(format!("the loader {} is not supported", meta.loader_name));
                } else if meta.game_version.is_empty() {
                    unsupported = Some("the Minecraft version is missing".to_string());
                }
            }
            let available = if unsupported.is_none() {
                ImportKind::ALL
                    .into_iter()
                    .filter(|k| k.parts().iter().any(|part| has_part(&path, part)))
                    .collect()
            } else {
                Vec::new()
            };
            let (loader, loader_version) = loader.unwrap_or((LoaderKind::Vanilla, None));
            list.push(SourceInstance {
                name: meta
                    .as_ref()
                    .and_then(|m| m.name.clone())
                    .unwrap_or_else(|| id.clone()),
                mc_version: meta.map(|m| m.game_version).unwrap_or_default(),
                id,
                loader,
                loader_version,
                available,
                unsupported,
            });
        }
        list.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(list)
    }

    fn source_path(&self, id: &str, part: &str) -> Result<Option<PathBuf>, CoreError> {
        let rel = check_rel_path(id)?;
        let base = self.instances_dir()?.join(rel);
        if is_link(&base) {
            return Ok(None);
        }
        let path = base.join(part);
        Ok((path.exists() && !is_link(&path)).then_some(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn instance(root: &Path, dir: &str, json: &str) -> PathBuf {
        let p = root.join("Instances").join(dir);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join(META_FILE), json).unwrap();
        p
    }

    fn file(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    fn meta(game: &str, loader: Option<&str>) -> String {
        let base = match loader {
            Some(l) => format!(r#"{{"name":"{l}","minecraftVersion":"{game}"}}"#),
            None => "null".to_string(),
        };
        format!(r#"{{"name":"Pack","gameVersion":"{game}","baseModLoader":{base}}}"#)
    }

    fn one(json: &str) -> SourceInstance {
        let tmp = tempfile::tempdir().unwrap();
        instance(tmp.path(), "P", json);
        let mut list = CurseForgeApp::at(tmp.path().to_path_buf()).list().unwrap();
        assert_eq!(list.len(), 1);
        list.remove(0)
    }

    #[test]
    fn lists_instances_with_the_content_that_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let p = instance(
            tmp.path(),
            "All the Mods",
            r#"{"name":"ATM 9","gameVersion":"1.20.1","baseModLoader":{"name":"forge-47.2.0"}}"#,
        );
        file(&p.join("mods/a.jar"));
        file(&p.join("saves/w/level.dat"));
        fs::create_dir_all(p.join("screenshots")).unwrap();
        instance(tmp.path(), "Second", &meta("1.21.1", None));
        let list = CurseForgeApp::at(tmp.path().to_path_buf()).list().unwrap();
        assert_eq!(list.len(), 2);
        let i = list.iter().find(|i| i.id == "All the Mods").unwrap();
        assert_eq!(
            (i.name.as_str(), i.mc_version.as_str()),
            ("ATM 9", "1.20.1")
        );
        assert_eq!(i.loader, LoaderKind::Forge);
        assert_eq!(i.loader_version.as_deref(), Some("47.2.0"));
        assert_eq!(i.available, [ImportKind::Mods, ImportKind::Saves]);
        assert_eq!(i.unsupported, None);
    }

    #[test]
    fn loader_names_map_to_loaders_and_versions() {
        let cases = [
            ("forge-47.2.0", LoaderKind::Forge, "47.2.0"),
            ("fabric-0.16.10-1.21.1", LoaderKind::Fabric, "0.16.10"),
            ("quilt-0.26.0-1.20.1", LoaderKind::Quilt, "0.26.0"),
            ("neoforge-21.1.77", LoaderKind::Neoforge, "21.1.77"),
        ];
        for (name, kind, version) in cases {
            let i = one(&meta("1.21.1", Some(name)));
            assert_eq!(i.loader, kind, "{name}");
            assert_eq!(i.loader_version.as_deref(), Some(version), "{name}");
            assert_eq!(i.unsupported, None, "{name}");
        }
    }

    #[test]
    fn no_base_loader_is_vanilla() {
        for json in [
            meta("1.21.1", None),
            r#"{"name":"V","gameVersion":"1.21.1"}"#.to_string(),
            r#"{"name":"V","gameVersion":"1.21.1","baseModLoader":{"name":""}}"#.to_string(),
        ] {
            let i = one(&json);
            assert_eq!(i.loader, LoaderKind::Vanilla);
            assert_eq!(i.loader_version, None);
            assert_eq!(i.unsupported, None);
        }
    }

    #[test]
    fn unknown_loaders_and_missing_versions_say_why() {
        for json in [
            meta("1.12.2", Some("liteloader-1.0")),
            meta("1.21.1", Some("forge")),
            r#"{"name":"N","baseModLoader":{"name":"forge-47.2.0"}}"#.to_string(),
            r#"{"name":"N","gameVersion":"  "}"#.to_string(),
        ] {
            let i = one(&json);
            assert!(i.unsupported.is_some(), "{json}");
            assert!(i.available.is_empty());
        }
    }

    #[test]
    fn a_broken_metadata_file_makes_only_that_row_unsupported() {
        let tmp = tempfile::tempdir().unwrap();
        instance(tmp.path(), "Broken", "{ not json");
        instance(tmp.path(), "Good", &meta("1.21.1", None));
        let list = CurseForgeApp::at(tmp.path().to_path_buf()).list().unwrap();
        assert_eq!(list.len(), 2);
        let broken = list.iter().find(|i| i.id == "Broken").unwrap();
        assert_eq!(broken.name, "Broken");
        assert!(broken.unsupported.is_some());
        assert_eq!(
            list.iter().find(|i| i.id == "Good").unwrap().unsupported,
            None
        );
    }

    #[test]
    fn name_falls_back_to_the_folder_name() {
        let i = one(r#"{"gameVersion":"1.21.1"}"#);
        assert_eq!(i.name, "P");
    }

    #[test]
    fn entries_that_are_not_instances_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        instance(tmp.path(), "Real", &meta("1.21.1", None));
        fs::create_dir_all(tmp.path().join("Instances/NoMeta/mods")).unwrap();
        file(&tmp.path().join("Instances/stray.txt"));
        let list = CurseForgeApp::at(tmp.path().to_path_buf()).list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "Real");
    }

    #[test]
    fn a_missing_or_foreign_folder_is_unavailable() {
        let empty = tempfile::tempdir().unwrap();
        let err = CurseForgeApp::at(empty.path().to_path_buf())
            .list()
            .unwrap_err();
        assert_eq!(err.code(), "import.unavailable");
        let err = CurseForgeApp::at(empty.path().join("nope"))
            .list()
            .unwrap_err();
        assert_eq!(err.code(), "import.unavailable");
    }

    #[test]
    fn the_instances_folder_itself_can_be_chosen() {
        let tmp = tempfile::tempdir().unwrap();
        instance(tmp.path(), "A", &meta("1.21.1", None));
        let list = CurseForgeApp::at(tmp.path().join("Instances"))
            .list()
            .unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn source_path_returns_existing_paths_only() {
        let tmp = tempfile::tempdir().unwrap();
        let p = instance(tmp.path(), "A", &meta("1.21.1", None));
        file(&p.join("mods/a.jar"));
        file(&p.join("options.txt"));
        let app = CurseForgeApp::at(tmp.path().to_path_buf());
        assert_eq!(app.source_path("A", "mods").unwrap(), Some(p.join("mods")));
        assert_eq!(
            app.source_path("A", "options.txt").unwrap(),
            Some(p.join("options.txt"))
        );
        assert_eq!(app.source_path("A", "saves").unwrap(), None);
        assert_eq!(app.source_path("Missing", "mods").unwrap(), None);
    }

    #[test]
    fn source_path_refuses_unsafe_ids() {
        let tmp = tempfile::tempdir().unwrap();
        instance(tmp.path(), "A", &meta("1.21.1", None));
        let app = CurseForgeApp::at(tmp.path().to_path_buf());
        for id in ["../outside", "/abs", "a/../../b"] {
            let err = app.source_path(id, "mods").unwrap_err();
            assert_eq!(err.code(), "mrpack.unsafe_path", "{id}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn linked_parts_and_linked_instances_are_not_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        file(&outside.path().join("x.jar"));
        let p = instance(tmp.path(), "A", &meta("1.21.1", None));
        std::os::unix::fs::symlink(outside.path(), p.join("mods")).unwrap();
        std::os::unix::fs::symlink(&p, tmp.path().join("Instances/Link")).unwrap();
        let app = CurseForgeApp::at(tmp.path().to_path_buf());
        assert_eq!(app.source_path("A", "mods").unwrap(), None);
        assert_eq!(app.source_path("Link", "mods").unwrap(), None);
        let list = app.list().unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].available.is_empty());
    }

    #[test]
    fn reading_leaves_the_originals_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let p = instance(tmp.path(), "A", &meta("1.21.1", Some("forge-1.0")));
        let before = fs::read(p.join(META_FILE)).unwrap();
        let app = CurseForgeApp::at(tmp.path().to_path_buf());
        app.list().unwrap();
        app.source_path("A", "mods").unwrap();
        assert_eq!(fs::read(p.join(META_FILE)).unwrap(), before);
        assert_eq!(fs::read_dir(&p).unwrap().count(), 1);
    }
}
