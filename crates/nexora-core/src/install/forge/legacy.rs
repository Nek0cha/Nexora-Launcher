//! 入れ子の旧形式（1.8.9 など）の Forge の `install_profile.json` を読み、`LoaderProfile` の JSON に正規化する。
//! 副作用を持たない。

use serde_json::{Value, json};

use crate::error::CoreError;
use crate::install::plan::{is_safe_relative, maven_path};
use crate::meta::parse_json;

/// URL を持たないライブラリの取得元（公式インストーラの既定と同じ）。
const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";

/// 読み終えた旧形式のプロファイル。
#[derive(Debug, Clone)]
pub struct LegacyProfile {
    pub minecraft: String,
    /// Forge 本体のライブラリの座標（`install.path`）。
    pub forge_path: String,
    /// インストーラ直下の universal jar のエントリ名（`install.filePath`）。
    pub universal_entry: String,
    /// 正規化済みの `LoaderProfile` の JSON。
    pub profile_json: Vec<u8>,
}

fn invalid(message: String) -> CoreError {
    CoreError::InvalidMetadata { message }
}

fn is_nested(object: &serde_json::Map<String, Value>) -> bool {
    !object.contains_key("spec")
        && object.contains_key("install")
        && object.contains_key("versionInfo")
}

/// `spec` が無く、`install` と `versionInfo` を持つ形か。読めない JSON は偽。
pub fn is_legacy_profile(bytes: &[u8]) -> bool {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|v| v.as_object().map(is_nested))
        .unwrap_or(false)
}

fn text(value: &Value, key: &str) -> Result<String, CoreError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| invalid(format!("install_profile.json: install.{key} is missing")))
}

/// 入れ子の旧形式を読み、`versionInfo` を `LoaderProfile` として読める JSON に正規化する。
/// 別の形は `UnsupportedProfile`、読めない JSON や食い違いは `InvalidMetadata`。
/// 正規化：`clientreq` が `false` のライブラリを捨て、`serverreq`、`clientreq`、`checksums` を消し、
/// Forge 本体は URL を空にして（インストーラが置く）、`url` の無いものは Mojang の取得元にする。
pub fn parse_legacy_profile(bytes: &[u8]) -> Result<LegacyProfile, CoreError> {
    let mut value: Value = parse_json(bytes, "install_profile.json")?;
    let nested = value.as_object().is_some_and(is_nested);
    if !nested {
        return Err(CoreError::UnsupportedProfile {
            reason: "not a nested legacy install profile".into(),
        });
    }
    let install = &value["install"];
    let minecraft = text(install, "minecraft")?;
    let forge_path = text(install, "path")?;
    let universal_entry = text(install, "filePath")?;
    let forge_rel = maven_path(&forge_path)
        .filter(|rel| is_safe_relative(rel))
        .ok_or_else(|| invalid(format!("install.path {forge_path:?} is not a safe library")))?;

    let info = value
        .get_mut("versionInfo")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid("install_profile.json: versionInfo is not an object".into()))?;
    if info.get("inheritsFrom").and_then(Value::as_str) != Some(minecraft.as_str()) {
        return Err(invalid(format!(
            "versionInfo does not inherit from Minecraft {minecraft}"
        )));
    }
    let libraries = info
        .get("libraries")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("install_profile.json: versionInfo.libraries is missing".into()))?;
    let mut normalized = Vec::new();
    let mut has_forge = false;
    for lib in libraries {
        let mut lib = lib.clone();
        let Some(object) = lib.as_object_mut() else {
            return Err(invalid(
                "install_profile.json: a library is not an object".into(),
            ));
        };
        if object.get("clientreq") == Some(&Value::Bool(false)) {
            continue;
        }
        for key in ["serverreq", "clientreq", "checksums"] {
            object.remove(key);
        }
        if object.get("name").and_then(Value::as_str) == Some(forge_path.as_str()) {
            has_forge = true;
            object.remove("url");
            object.insert(
                "downloads".into(),
                json!({ "artifact": { "path": forge_rel, "url": "", "sha1": "", "size": 0 } }),
            );
        } else if !object.contains_key("url") {
            object.insert("url".into(), MOJANG_LIBRARIES.into());
        }
        normalized.push(lib);
    }
    if !has_forge {
        return Err(invalid(format!(
            "versionInfo has no library {forge_path}, so the universal jar would not be used"
        )));
    }
    info.insert("libraries".into(), Value::Array(normalized));
    let profile_json = serde_json::to_vec_pretty(&value["versionInfo"])
        .map_err(|e| invalid(format!("install_profile.json: {e}")))?;
    Ok(LegacyProfile {
        minecraft,
        forge_path,
        universal_entry,
        profile_json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::loader::LoaderProfile;
    use crate::meta::parse_json;

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/tests/fixtures/forge/{name}-install_profile.json",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(path).unwrap()
    }

    fn real() -> LegacyProfile {
        parse_legacy_profile(&fixture("forge-1.8.9")).unwrap()
    }

    fn normalized() -> serde_json::Value {
        serde_json::from_slice(&real().profile_json).unwrap()
    }

    fn library<'a>(profile: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
        profile["libraries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["name"] == name)
    }

    #[test]
    fn real_legacy_profile_parses() {
        let legacy = real();
        assert_eq!(legacy.minecraft, "1.8.9");
        assert_eq!(
            legacy.forge_path,
            "net.minecraftforge:forge:1.8.9-11.15.1.2318-1.8.9"
        );
        assert_eq!(
            legacy.universal_entry,
            "forge-1.8.9-11.15.1.2318-1.8.9-universal.jar"
        );
    }

    #[test]
    fn normalized_profile_is_a_loader_profile() {
        let profile: LoaderProfile = parse_json(&real().profile_json, "loader profile").unwrap();
        assert_eq!(profile.inherits_from, "1.8.9");
        assert_eq!(profile.main_class, "net.minecraft.launchwrapper.Launch");
        let args = profile.minecraft_arguments.unwrap();
        assert!(
            args.contains("--tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker"),
            "{args}"
        );
    }

    #[test]
    fn forge_library_is_generated_not_downloaded() {
        let profile = normalized();
        let forge = library(
            &profile,
            "net.minecraftforge:forge:1.8.9-11.15.1.2318-1.8.9",
        )
        .unwrap();
        let artifact = &forge["downloads"]["artifact"];
        assert_eq!(artifact["url"], "");
        assert_eq!(
            artifact["path"],
            "net/minecraftforge/forge/1.8.9-11.15.1.2318-1.8.9/forge-1.8.9-11.15.1.2318-1.8.9.jar"
        );
        assert!(forge.get("url").is_none());
    }

    #[test]
    fn libraries_without_url_use_the_mojang_repository() {
        let profile = normalized();
        let launchwrapper = library(&profile, "net.minecraft:launchwrapper:1.12").unwrap();
        assert_eq!(launchwrapper["url"], "https://libraries.minecraft.net/");
        // 元から URL を持つものは変えない
        let akka = library(&profile, "com.typesafe.akka:akka-actor_2.11:2.3.3").unwrap();
        assert_eq!(akka["url"], "https://maven.minecraftforge.net/");
    }

    #[test]
    fn client_unneeded_libraries_are_dropped() {
        let profile = normalized();
        assert!(library(&profile, "jline:jline:2.13").is_none());
        assert!(library(&profile, "net.sf.trove4j:trove4j:3.0.3").is_some());
    }

    #[test]
    fn server_only_keys_are_removed() {
        let profile = normalized();
        for lib in profile["libraries"].as_array().unwrap() {
            for key in ["serverreq", "clientreq", "checksums"] {
                assert!(lib.get(key).is_none(), "{key} in {lib}");
            }
        }
    }

    #[test]
    fn nested_1_12_2_build_2847_parses_too() {
        // 1.12.2 の古いビルドも入れ子の形（新しい 2859 以降は spec 0）
        let legacy = parse_legacy_profile(&fixture("forge-legacy-2847")).unwrap();
        assert_eq!(legacy.minecraft, "1.12.2");
        assert_eq!(
            legacy.universal_entry,
            "forge-1.12.2-14.23.5.2847-universal.jar"
        );
        let profile: LoaderProfile = parse_json(&legacy.profile_json, "loader profile").unwrap();
        assert_eq!(profile.inherits_from, "1.12.2");
        assert!(profile.minecraft_arguments.is_some());
    }

    #[test]
    fn is_legacy_profile_tells_the_shapes_apart() {
        assert!(is_legacy_profile(&fixture("forge-1.8.9")));
        assert!(!is_legacy_profile(&fixture("forge-1.12.2")));
        assert!(!is_legacy_profile(&fixture("forge-1.16.5")));
        assert!(!is_legacy_profile(b"not json"));
    }

    #[test]
    fn other_shapes_are_unsupported_and_broken_json_is_invalid() {
        let err = parse_legacy_profile(&fixture("forge-1.12.2")).unwrap_err();
        assert_eq!(err.code(), "install.unsupported_profile");
        let err = parse_legacy_profile(b"{").unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
    }

    #[test]
    fn mismatched_minecraft_is_invalid() {
        let mut value: serde_json::Value = serde_json::from_slice(&fixture("forge-1.8.9")).unwrap();
        value["install"]["minecraft"] = "1.8".into();
        let err = parse_legacy_profile(value.to_string().as_bytes()).unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
    }

    #[test]
    fn a_profile_without_the_forge_library_is_invalid() {
        let mut value: serde_json::Value = serde_json::from_slice(&fixture("forge-1.8.9")).unwrap();
        value["install"]["path"] = "net.minecraftforge:forge:9.9.9".into();
        let err = parse_legacy_profile(value.to_string().as_bytes()).unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
    }

    #[test]
    fn an_unsafe_forge_path_is_invalid() {
        let mut value: serde_json::Value = serde_json::from_slice(&fixture("forge-1.8.9")).unwrap();
        value["install"]["path"] = "..:..:1".into();
        let err = parse_legacy_profile(value.to_string().as_bytes()).unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
    }
}
