//! Prism Launcher（MultiMC 系を含む）のインスタンスを読む。元のファイルは読むだけで、書き換えない。
//! 保存形式は `docs/research/06-prism-launcher-format.md` にある。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::copy::is_link;
use super::{ImportKind, ImportSource, SourceInstance};
use crate::error::CoreError;
use crate::meta::loader::LoaderKind;
use crate::mrpack::safe_path::check_rel_path;

const APP_FOLDER: &str = "PrismLauncher";
/// 設定ファイルの名前。新しい順（Prism は最初に見つかったものを読む）。
const CONFIG_FILES: [&str; 3] = ["prismlauncher.cfg", "polymc.cfg", "multimc.cfg"];
const DEFAULT_INSTANCE_DIR: &str = "instances";
const INSTANCE_FILE: &str = "instance.cfg";
const PACK_FILE: &str = "mmc-pack.json";

/// Prism Launcher のデータフォルダ（`prismlauncher.cfg` があるところ）。
pub struct PrismLauncher {
    root: PathBuf,
}

fn unavailable(reason: impl Into<String>) -> CoreError {
    CoreError::ImportUnavailable {
        reason: reason.into(),
    }
}

/// `instance.cfg` と `prismlauncher.cfg` の値のエスケープ（`\\`、`\n`、`\t`、`\#`、`\xHHHH`）を戻す。
fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('x') => {
                let mut hex = String::new();
                while hex.len() < 4 && chars.peek().is_some_and(char::is_ascii_hexdigit) {
                    hex.extend(chars.next());
                }
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(decoded) => out.push(decoded),
                    None => out.push('x'),
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// INI を読み、`[General]`（または見出しの前）のキーと値を返す。
/// Prism の新旧どちらの書き方（`QSettings` 形式と、見出しの無い旧形式）も同じように読める。
fn parse_ini(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut in_general = true;
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            in_general = section.eq_ignore_ascii_case("General");
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !in_general {
            continue;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(value);
        map.insert(key.trim().to_string(), unescape(value));
    }
    map
}

/// INI を読む。ファイルが無ければ `Ok(None)`。
fn read_ini(path: &Path) -> std::io::Result<Option<HashMap<String, String>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(parse_ini(&String::from_utf8_lossy(&bytes)))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

impl PrismLauncher {
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// 既定の場所にデータがあれば、そのフォルダ。
    pub fn locate() -> Option<PathBuf> {
        Self::locate_in(dirs::data_dir().as_deref())
    }

    /// `data_dir` の下の `PrismLauncher` に、設定かインスタンスのフォルダがあれば、そのフォルダ。
    fn locate_in(data_dir: Option<&Path>) -> Option<PathBuf> {
        let root = data_dir?.join(APP_FOLDER);
        let found = CONFIG_FILES.iter().any(|f| root.join(f).is_file())
            || root.join(DEFAULT_INSTANCE_DIR).is_dir();
        found.then_some(root)
    }

    /// インスタンスのフォルダが並ぶフォルダ。設定の `InstanceDir`（相対ならデータフォルダから）、無ければ `instances`。
    fn instances_dir(&self) -> Result<PathBuf, CoreError> {
        let mut configured = None;
        for name in CONFIG_FILES {
            let path = self.root.join(name);
            match read_ini(&path) {
                Ok(Some(map)) => {
                    configured = map.get("InstanceDir").cloned();
                    break;
                }
                Ok(None) => {}
                Err(e) => return Err(unavailable(format!("{}: {e}", path.display()))),
            }
        }
        let dir = configured
            .filter(|d| !d.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_INSTANCE_DIR.to_string());
        let dir = self.root.join(dir);
        if dir.is_dir() {
            Ok(dir)
        } else {
            Err(unavailable(format!("{} was not found", dir.display())))
        }
    }
}

/// ゲームのフォルダ。Prism と同じく、`.minecraft` があって `minecraft` が無いときだけ `.minecraft`。
fn game_dir(instance: &Path) -> PathBuf {
    let plain = instance.join("minecraft");
    let dotted = instance.join(".minecraft");
    if dotted.exists() && !plain.exists() {
        dotted
    } else {
        plain
    }
}

/// `mmc-pack.json` から読んだ、インスタンスの構成。
struct Pack {
    mc_version: String,
    loader: LoaderKind,
    loader_version: Option<String>,
}

fn loader_of(uid: &str) -> Option<LoaderKind> {
    match uid {
        "net.fabricmc.fabric-loader" => Some(LoaderKind::Fabric),
        "org.quiltmc.quilt-loader" => Some(LoaderKind::Quilt),
        "net.minecraftforge" => Some(LoaderKind::Forge),
        "net.neoforged" => Some(LoaderKind::Neoforge),
        _ => None,
    }
}

/// 取り込めないときは、その理由（英語）を返す。
fn read_pack(instance: &Path) -> Result<Pack, String> {
    let path = instance.join(PACK_FILE);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("{PACK_FILE} was not found"));
        }
        Err(e) => return Err(format!("{PACK_FILE} could not be read: {e}")),
    };
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| format!("{PACK_FILE} is not valid JSON"))?;
    let components = json
        .get("components")
        .and_then(|c| c.as_array())
        .ok_or_else(|| format!("{PACK_FILE} has no components"))?;
    let mut mc_version = String::new();
    let mut loaders = Vec::new();
    for component in components {
        let uid = component.get("uid").and_then(|u| u.as_str()).unwrap_or("");
        let version = component
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if uid == "net.minecraft" {
            mc_version = version.to_string();
        } else if uid == "com.mumfrey.liteloader" {
            return Err("the loader LiteLoader is not supported".to_string());
        } else if let Some(loader) = loader_of(uid) {
            loaders.push((loader, version.to_string()));
        }
    }
    if mc_version.is_empty() {
        return Err("the Minecraft version is missing".to_string());
    }
    if loaders.len() > 1 {
        return Err("more than one loader is installed".to_string());
    }
    let (loader, loader_version) = loaders
        .pop()
        .map_or((LoaderKind::Vanilla, None), |(loader, version)| {
            (loader, Some(version).filter(|v| !v.is_empty()))
        });
    Ok(Pack {
        mc_version,
        loader,
        loader_version,
    })
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

impl ImportSource for PrismLauncher {
    fn list(&self) -> Result<Vec<SourceInstance>, CoreError> {
        let dir = self.instances_dir()?;
        let entries =
            fs::read_dir(&dir).map_err(|e| unavailable(format!("{}: {e}", dir.display())))?;
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            // `instance.cfg` が無いフォルダ（`_LAUNCHER_TEMP`、`.tmp` など）とファイルは、Prism も一覧に出さない。
            if !path.is_dir() || !path.join(INSTANCE_FILE).is_file() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            let settings = read_ini(&path.join(INSTANCE_FILE));
            // 種類が `OneSix` 以外のインスタンスは、Prism も読み込まない。
            if let Ok(Some(map)) = &settings
                && map
                    .get("InstanceType")
                    .is_some_and(|t| !t.is_empty() && t != "OneSix")
            {
                continue;
            }
            let name = settings
                .as_ref()
                .ok()
                .and_then(|s| s.as_ref())
                .and_then(|map| map.get("name"))
                .map(|n| n.trim())
                .filter(|n| !n.is_empty())
                .unwrap_or(&id)
                .to_string();

            let pack = if is_link(&path) {
                Err("the instance folder is a link".to_string())
            } else if entry.file_name().to_str().is_none() || check_rel_path(&id).is_err() {
                Err(format!("{id} is not a safe folder name"))
            } else if settings.is_err() {
                Err(format!("{INSTANCE_FILE} could not be read"))
            } else {
                read_pack(&path)
            };
            out.push(match pack {
                Ok(pack) => {
                    let game = game_dir(&path);
                    let available = if is_link(&game) {
                        Vec::new()
                    } else {
                        ImportKind::ALL
                            .into_iter()
                            .filter(|k| k.parts().iter().any(|part| has_part(&game, part)))
                            .collect()
                    };
                    SourceInstance {
                        id,
                        name,
                        mc_version: pack.mc_version,
                        loader: pack.loader,
                        loader_version: pack.loader_version,
                        available,
                        unsupported: None,
                    }
                }
                Err(reason) => SourceInstance {
                    id,
                    name,
                    mc_version: String::new(),
                    loader: LoaderKind::Vanilla,
                    loader_version: None,
                    available: Vec::new(),
                    unsupported: Some(reason),
                },
            });
        }
        out.sort_by(|a, b| (a.name.to_lowercase(), &a.id).cmp(&(b.name.to_lowercase(), &b.id)));
        Ok(out)
    }

    fn source_path(&self, id: &str, part: &str) -> Result<Option<PathBuf>, CoreError> {
        let rel = check_rel_path(id)?;
        // ID はインスタンスのフォルダ名 1 つ。階層は認めない。
        if rel.components().count() != 1 {
            return Err(CoreError::MrpackUnsafePath {
                path: id.to_string(),
            });
        }
        let instance = self.instances_dir()?.join(rel);
        if !instance.is_dir() || is_link(&instance) {
            return Ok(None);
        }
        let game = game_dir(&instance);
        if is_link(&game) {
            return Ok(None);
        }
        let path = game.join(part);
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

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn file(path: &Path) {
        write(path, "x");
    }

    /// `mmc-pack.json` の中身。`(uid, version)` の並び。
    fn pack(components: &[(&str, &str)]) -> String {
        let list: Vec<String> = components
            .iter()
            .map(|(uid, version)| format!(r#"{{"uid":"{uid}","version":"{version}"}}"#))
            .collect();
        format!(r#"{{"components":[{}],"formatVersion":1}}"#, list.join(","))
    }

    /// `instances/<folder>` に `instance.cfg` と `mmc-pack.json` を作る。
    fn instance(root: &Path, folder: &str, name: &str, components: &[(&str, &str)]) -> PathBuf {
        let dir = root.join("instances").join(folder);
        write(
            &dir.join("instance.cfg"),
            &format!("[General]\nInstanceType=OneSix\nname={name}\n"),
        );
        write(&dir.join("mmc-pack.json"), &pack(components));
        dir
    }

    fn list(root: &Path) -> Vec<SourceInstance> {
        PrismLauncher::at(root.to_path_buf()).list().unwrap()
    }

    #[test]
    fn lists_instances_with_the_content_that_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instance(
            tmp.path(),
            "Survival",
            "Survival 2024",
            &[
                ("net.minecraft", "1.21.1"),
                ("net.fabricmc.fabric-loader", "0.16.5"),
            ],
        );
        file(&dir.join(".minecraft/mods/a.jar"));
        file(&dir.join(".minecraft/saves/w/level.dat"));
        fs::create_dir_all(dir.join(".minecraft/screenshots")).unwrap();
        let list = list(tmp.path());
        assert_eq!(list.len(), 1);
        let i = &list[0];
        assert_eq!(i.id, "Survival");
        assert_eq!(i.name, "Survival 2024");
        assert_eq!(i.mc_version, "1.21.1");
        assert_eq!(i.loader, LoaderKind::Fabric);
        assert_eq!(i.loader_version.as_deref(), Some("0.16.5"));
        assert_eq!(i.available, [ImportKind::Mods, ImportKind::Saves]);
        assert_eq!(i.unsupported, None);
    }

    #[test]
    fn every_supported_loader_is_mapped() {
        let tmp = tempfile::tempdir().unwrap();
        instance(tmp.path(), "v", "v", &[("net.minecraft", "1.21")]);
        instance(
            tmp.path(),
            "f",
            "f",
            &[
                ("net.minecraft", "1.21"),
                ("net.fabricmc.intermediary", "1.21"),
                ("net.fabricmc.fabric-loader", "0.16.5"),
            ],
        );
        instance(
            tmp.path(),
            "q",
            "q",
            &[
                ("net.minecraft", "1.21"),
                ("org.quiltmc.hashed", "1.21"),
                ("org.quiltmc.quilt-loader", "0.26.0"),
            ],
        );
        instance(
            tmp.path(),
            "fo",
            "fo",
            &[
                ("net.minecraft", "1.20.1"),
                ("net.minecraftforge", "47.2.0"),
            ],
        );
        instance(
            tmp.path(),
            "n",
            "n",
            &[("net.minecraft", "1.21.1"), ("net.neoforged", "21.1.77")],
        );
        let list = list(tmp.path());
        let by = |id: &str| list.iter().find(|i| i.id == id).unwrap();
        assert_eq!(by("v").loader, LoaderKind::Vanilla);
        assert_eq!(by("v").loader_version, None);
        assert_eq!(by("f").loader, LoaderKind::Fabric);
        assert_eq!(by("q").loader, LoaderKind::Quilt);
        assert_eq!(by("q").loader_version.as_deref(), Some("0.26.0"));
        assert_eq!(by("fo").loader, LoaderKind::Forge);
        assert_eq!(by("fo").loader_version.as_deref(), Some("47.2.0"));
        assert_eq!(by("n").loader, LoaderKind::Neoforge);
        assert!(list.iter().all(|i| i.unsupported.is_none()), "{list:?}");
    }

    #[test]
    fn unsupported_loaders_and_broken_packs_say_why() {
        let tmp = tempfile::tempdir().unwrap();
        instance(
            tmp.path(),
            "lite",
            "lite",
            &[
                ("net.minecraft", "1.12.2"),
                ("com.mumfrey.liteloader", "1.12.2"),
            ],
        );
        instance(
            tmp.path(),
            "two",
            "two",
            &[
                ("net.minecraft", "1.20.1"),
                ("net.minecraftforge", "47.2.0"),
                ("net.fabricmc.fabric-loader", "0.15.0"),
            ],
        );
        instance(
            tmp.path(),
            "nomc",
            "nomc",
            &[("net.fabricmc.fabric-loader", "1")],
        );
        instance(tmp.path(), "emptymc", "emptymc", &[("net.minecraft", " ")]);
        let nopack = instance(tmp.path(), "nopack", "nopack", &[]);
        fs::remove_file(nopack.join("mmc-pack.json")).unwrap();
        let broken = instance(tmp.path(), "broken", "broken", &[]);
        write(&broken.join("mmc-pack.json"), "{ not json");
        let list = list(tmp.path());
        assert_eq!(list.len(), 6);
        for i in &list {
            assert!(i.unsupported.is_some(), "{i:?}");
            assert!(i.available.is_empty(), "{i:?}");
        }
    }

    #[test]
    fn the_game_folder_is_dot_minecraft_or_minecraft() {
        let tmp = tempfile::tempdir().unwrap();
        let a = instance(tmp.path(), "dot", "dot", &[("net.minecraft", "1.21")]);
        file(&a.join(".minecraft/mods/a.jar"));
        let b = instance(tmp.path(), "plain", "plain", &[("net.minecraft", "1.21")]);
        file(&b.join("minecraft/saves/w/level.dat"));
        // 両方あるときは Prism と同じく `minecraft` を使う。
        let c = instance(tmp.path(), "both", "both", &[("net.minecraft", "1.21")]);
        file(&c.join(".minecraft/mods/a.jar"));
        file(&c.join("minecraft/screenshots/s.png"));
        let launcher = PrismLauncher::at(tmp.path().to_path_buf());
        let list = launcher.list().unwrap();
        let by = |id: &str| list.iter().find(|i| i.id == id).unwrap();
        assert_eq!(by("dot").available, [ImportKind::Mods]);
        assert_eq!(by("plain").available, [ImportKind::Saves]);
        assert_eq!(by("both").available, [ImportKind::Screenshots]);
        assert_eq!(
            launcher.source_path("dot", "mods").unwrap(),
            Some(a.join(".minecraft").join("mods"))
        );
        assert_eq!(launcher.source_path("both", "mods").unwrap(), None);
    }

    #[test]
    fn settings_are_available_when_any_of_their_parts_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instance(tmp.path(), "S", "S", &[("net.minecraft", "1.21")]);
        file(&dir.join(".minecraft/options.txt"));
        fs::create_dir_all(dir.join(".minecraft/config")).unwrap();
        fs::create_dir_all(dir.join(".minecraft/saves")).unwrap();
        let launcher = PrismLauncher::at(tmp.path().to_path_buf());
        assert_eq!(
            launcher.list().unwrap()[0].available,
            [ImportKind::Settings]
        );
        assert!(launcher.source_path("S", "options.txt").unwrap().is_some());
    }

    #[test]
    fn names_fall_back_to_the_folder_and_values_are_unquoted() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("instances");
        let mc = pack(&[("net.minecraft", "1.21")]);
        write(
            &dir.join("NoName/instance.cfg"),
            "[General]\nInstanceType=OneSix\n",
        );
        write(&dir.join("NoName/mmc-pack.json"), &mc);
        write(
            &dir.join("Fancy/instance.cfg"),
            "; comment\n[General]\r\nname=\"A, B = C\"\r\nother=1\n[Other]\nname=wrong\n",
        );
        write(&dir.join("Fancy/mmc-pack.json"), &mc);
        write(
            &dir.join("Old/instance.cfg"),
            "InstanceType=OneSix\nname=Old\\#style\n",
        );
        write(&dir.join("Old/mmc-pack.json"), &mc);
        let list = list(tmp.path());
        let by = |id: &str| list.iter().find(|i| i.id == id).unwrap();
        assert_eq!(by("NoName").name, "NoName");
        assert_eq!(by("Fancy").name, "A, B = C");
        assert_eq!(by("Old").name, "Old#style");
    }

    #[test]
    fn entries_that_are_not_instances_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        instance(tmp.path(), "Real", "Real", &[("net.minecraft", "1.21")]);
        let dir = tmp.path().join("instances");
        file(&dir.join("_LAUNCHER_TEMP/x.zip"));
        file(&dir.join(".tmp/y"));
        file(&dir.join("instgroups.json"));
        file(&dir.join("loose-file.txt"));
        // `instance.cfg` が無いフォルダは Prism も一覧に出さない。
        fs::create_dir_all(dir.join("Empty")).unwrap();
        // OneSix 以外の種類は Prism も読み込まない。
        write(
            &dir.join("Other/instance.cfg"),
            "InstanceType=Legacy\nname=Other\n",
        );
        let list = list(tmp.path());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "Real");
    }

    #[test]
    fn source_path_refuses_ids_that_leave_the_instances_directory() {
        let tmp = tempfile::tempdir().unwrap();
        instance(tmp.path(), "S", "S", &[("net.minecraft", "1.21")]);
        let launcher = PrismLauncher::at(tmp.path().to_path_buf());
        for id in [
            "..",
            "../outside",
            "S/..",
            "S/sub",
            "S\\sub",
            "/etc",
            "C:\\Windows",
            "",
        ] {
            let err = launcher.source_path(id, "mods").unwrap_err();
            assert_eq!(err.code(), "mrpack.unsafe_path", "{id}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn instance_folders_with_unsafe_names_are_listed_as_unsupported() {
        let tmp = tempfile::tempdir().unwrap();
        instance(tmp.path(), "bad:name", "Bad", &[("net.minecraft", "1.21")]);
        let list = list(tmp.path());
        assert_eq!(list.len(), 1);
        assert!(list[0].unsupported.is_some());
        assert!(list[0].available.is_empty());
    }

    #[test]
    fn source_path_returns_existing_paths_only() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instance(tmp.path(), "S", "S", &[("net.minecraft", "1.21")]);
        file(&dir.join(".minecraft/mods/a.jar"));
        let launcher = PrismLauncher::at(tmp.path().to_path_buf());
        assert_eq!(
            launcher.source_path("S", "mods").unwrap(),
            Some(dir.join(".minecraft").join("mods"))
        );
        assert_eq!(launcher.source_path("S", "saves").unwrap(), None);
        assert_eq!(launcher.source_path("Missing", "mods").unwrap(), None);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_never_followed() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        file(&outside.path().join("mods/a.jar"));
        file(&outside.path().join("saves/w/level.dat"));
        let dir = instance(tmp.path(), "S", "S", &[("net.minecraft", "1.21")]);
        fs::create_dir_all(dir.join(".minecraft")).unwrap();
        symlink(outside.path().join("mods"), dir.join(".minecraft/mods")).unwrap();
        symlink(outside.path().join("saves"), dir.join(".minecraft/saves")).unwrap();
        // インスタンスのフォルダそのものがリンクの場合。
        symlink(&dir, tmp.path().join("instances/Linked")).unwrap();
        let launcher = PrismLauncher::at(tmp.path().to_path_buf());
        assert_eq!(launcher.source_path("S", "mods").unwrap(), None);
        assert_eq!(launcher.source_path("Linked", "mods").unwrap(), None);
        let list = launcher.list().unwrap();
        assert!(
            list.iter()
                .find(|i| i.id == "S")
                .unwrap()
                .available
                .is_empty()
        );
        let linked = list.iter().find(|i| i.id == "Linked").unwrap();
        assert!(linked.unsupported.is_some());
    }

    #[test]
    fn instance_dir_in_the_settings_is_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let custom = tempfile::tempdir().unwrap();
        let settings = |dir_value: &str| {
            write(
                &tmp.path().join("prismlauncher.cfg"),
                &format!("[General]\nConfigVersion=1.3\nInstanceDir={dir_value}\n"),
            );
        };
        let mc = pack(&[("net.minecraft", "1.21")]);
        // 相対パスはデータフォルダからの相対。
        settings("my games");
        let rel = tmp.path().join("my games/R");
        write(&rel.join("instance.cfg"), "name=R\n");
        write(&rel.join("mmc-pack.json"), &mc);
        file(&rel.join("minecraft/mods/a.jar"));
        let launcher = PrismLauncher::at(tmp.path().to_path_buf());
        let l = launcher.list().unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].available, [ImportKind::Mods]);
        assert_eq!(
            launcher.source_path("R", "mods").unwrap(),
            Some(rel.join("minecraft").join("mods"))
        );

        // 絶対パス（Windows の `\` は設定ファイルの中で `\\` になっている）。
        let abs = custom.path().to_str().unwrap().replace('\\', "\\\\");
        settings(&abs);
        let a = custom.path().join("A");
        write(&a.join("instance.cfg"), "name=A\n");
        write(&a.join("mmc-pack.json"), &mc);
        let l = PrismLauncher::at(tmp.path().to_path_buf()).list().unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].name, "A");
    }

    #[test]
    fn a_missing_settings_file_means_the_default_folder() {
        let tmp = tempfile::tempdir().unwrap();
        instance(tmp.path(), "S", "S", &[("net.minecraft", "1.21")]);
        assert_eq!(list(tmp.path()).len(), 1);
    }

    #[test]
    fn a_missing_instances_folder_is_unavailable() {
        let tmp = tempfile::tempdir().unwrap();
        let err = PrismLauncher::at(tmp.path().to_path_buf())
            .list()
            .unwrap_err();
        assert_eq!(err.code(), "import.unavailable");
        let err = PrismLauncher::at(tmp.path().join("nowhere"))
            .list()
            .unwrap_err();
        assert_eq!(err.code(), "import.unavailable");
        let err = PrismLauncher::at(tmp.path().to_path_buf())
            .source_path("S", "mods")
            .unwrap_err();
        assert_eq!(err.code(), "import.unavailable");
    }

    #[test]
    fn an_empty_instances_folder_lists_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("instances")).unwrap();
        assert!(list(tmp.path()).is_empty());
    }

    #[test]
    fn locate_needs_the_settings_or_the_instances_folder() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(PrismLauncher::locate_in(Some(tmp.path())), None);
        assert_eq!(PrismLauncher::locate_in(None), None);
        fs::create_dir_all(tmp.path().join("PrismLauncher")).unwrap();
        assert_eq!(PrismLauncher::locate_in(Some(tmp.path())), None);
        file(&tmp.path().join("PrismLauncher/prismlauncher.cfg"));
        assert_eq!(
            PrismLauncher::locate_in(Some(tmp.path())),
            Some(tmp.path().join("PrismLauncher"))
        );
        let other = tempfile::tempdir().unwrap();
        fs::create_dir_all(other.path().join("PrismLauncher/instances")).unwrap();
        assert!(PrismLauncher::locate_in(Some(other.path())).is_some());
    }

    #[test]
    fn reading_leaves_the_original_files_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = instance(tmp.path(), "S", "S", &[("net.minecraft", "1.21")]);
        file(&dir.join(".minecraft/mods/a.jar"));
        write(
            &tmp.path().join("prismlauncher.cfg"),
            "InstanceDir=instances\n",
        );
        let snapshot = || {
            let mut out = Vec::new();
            let mut stack = vec![tmp.path().to_path_buf()];
            while let Some(d) = stack.pop() {
                for e in fs::read_dir(&d).unwrap() {
                    let p = e.unwrap().path();
                    if p.is_dir() {
                        stack.push(p);
                    } else {
                        out.push((p.clone(), fs::read(&p).unwrap()));
                    }
                }
            }
            out.sort();
            out
        };
        let before = snapshot();
        let launcher = PrismLauncher::at(tmp.path().to_path_buf());
        launcher.list().unwrap();
        launcher.source_path("S", "mods").unwrap();
        assert_eq!(snapshot(), before);
    }
}
