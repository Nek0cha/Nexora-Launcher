//! Forge と NeoForge のインストーラが持つ `install_profile.json` を解釈する。副作用を持たない。

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::CoreError;
use crate::meta::mojang::version::Library;
use crate::meta::parse_json;

/// `data` の 1 項目。client と server で値が分かれる。
#[derive(Debug, Clone, Deserialize)]
pub struct SidedValue {
    pub client: String,
    #[serde(default)]
    pub server: String,
}

/// インストール時に実行する processor の 1 つ。
#[derive(Debug, Clone, Deserialize)]
pub struct Processor {
    /// 実行する側（`client`、`server`）。無ければ両方で実行する。
    pub sides: Option<Vec<String>>,
    pub jar: String,
    #[serde(default)]
    pub classpath: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// 出力のパスと、期待する sha1（どちらも `data` の参照を含みうる）。
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InstallProfile {
    pub spec: u32,
    pub minecraft: String,
    /// version.json のエントリ名（通常 `/version.json`）。
    pub json: String,
    #[serde(default)]
    pub data: BTreeMap<String, SidedValue>,
    #[serde(default)]
    pub processors: Vec<Processor>,
    #[serde(default)]
    pub libraries: Vec<Library>,
}

fn unsupported(reason: String) -> CoreError {
    CoreError::UnsupportedProfile { reason }
}

/// `spec` が 0 と 1 以外、または入れ子の旧形式（`install` と `versionInfo` を持ち `spec` が無い）は
/// `UnsupportedProfile`（理由の文字列に `legacy` または `spec` を含める）。読めない JSON は `InvalidMetadata`。
pub fn parse_install_profile(bytes: &[u8]) -> Result<InstallProfile, CoreError> {
    // 形を確かめてから型に読む。旧形式は型が合わず、読めない JSON と区別できないため
    let value: serde_json::Value = parse_json(bytes, "install_profile.json")?;
    let Some(object) = value.as_object() else {
        return Err(CoreError::InvalidMetadata {
            message: "install_profile.json is not an object".into(),
        });
    };
    if !object.contains_key("spec")
        && object.contains_key("install")
        && object.contains_key("versionInfo")
    {
        return Err(unsupported("legacy nested install profile".into()));
    }
    let profile: InstallProfile =
        serde_json::from_value(value).map_err(|e| CoreError::InvalidMetadata {
            message: format!("install_profile.json: {e}"),
        })?;
    if profile.spec > 1 {
        return Err(unsupported(format!(
            "install profile spec {}",
            profile.spec
        )));
    }
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/tests/fixtures/forge/{name}-install_profile.json",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(path).unwrap()
    }

    #[test]
    fn real_profiles_parse() {
        // (fixture, spec, minecraft, processors の数)。README の記録に合わせる
        for (name, spec, minecraft, processors) in [
            ("forge-1.16.5", 0, "1.16.5", 4),
            ("forge-1.20.1", 1, "1.20.1", 10),
            ("neoforge-21.1", 1, "1.21.1", 10),
            ("neoforge-1.20.1", 1, "1.20.1", 10),
        ] {
            let profile = parse_install_profile(&fixture(name)).unwrap();
            assert_eq!(profile.spec, spec, "{name}");
            assert_eq!(profile.minecraft, minecraft, "{name}");
            assert_eq!(profile.json, "/version.json", "{name}");
            assert_eq!(profile.processors.len(), processors, "{name}");
            assert!(!profile.libraries.is_empty(), "{name}");
            assert_eq!(
                profile.data["BINPATCH"].client, "/data/client.lzma",
                "{name}"
            );
        }
    }

    #[test]
    fn real_1_12_2_profile_has_no_processors() {
        // 1.12.2 の 2859 以降は spec 0 で、processor も data も無い（導入は `maven/` を置くだけ）
        let profile = parse_install_profile(&fixture("forge-1.12.2")).unwrap();
        assert_eq!(profile.spec, 0);
        assert_eq!(profile.minecraft, "1.12.2");
        assert_eq!(profile.json, "/version.json");
        assert!(profile.processors.is_empty());
        assert!(profile.data.is_empty());
        assert!(crate::install::forge::data::client_resources(&profile).is_empty());
    }

    #[test]
    fn sides_and_outputs_are_read() {
        let profile = parse_install_profile(&fixture("forge-1.20.1")).unwrap();
        let sided: Vec<_> = profile
            .processors
            .iter()
            .filter_map(|p| p.sides.as_ref())
            .collect();
        assert_eq!(sided.len(), 5);
        assert_eq!(
            sided
                .iter()
                .filter(|s| s.iter().any(|x| x == "client"))
                .count(),
            1
        );
        assert_eq!(
            profile
                .processors
                .iter()
                .filter(|p| !p.outputs.is_empty())
                .count(),
            2
        );
        let old = parse_install_profile(&fixture("forge-1.16.5")).unwrap();
        assert!(old.processors.iter().all(|p| p.sides.is_none()));
    }

    #[test]
    fn libraries_with_an_empty_url_are_read() {
        // 1.16.5 の `:universal` は jar 内の `maven/` から置くもので、URL が空
        let profile = parse_install_profile(&fixture("forge-1.16.5")).unwrap();
        let universal = profile
            .libraries
            .iter()
            .find(|l| l.name == "net.minecraftforge:forge:1.16.5-36.2.39:universal")
            .unwrap();
        let artifact = universal
            .downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .unwrap();
        assert_eq!(artifact.url, "");
        assert_eq!(
            artifact.path.as_deref(),
            Some("net/minecraftforge/forge/1.16.5-36.2.39/forge-1.16.5-36.2.39-universal.jar")
        );
    }

    #[test]
    fn legacy_nested_profile_is_unsupported() {
        let err = parse_install_profile(&fixture("forge-legacy-2847")).unwrap_err();
        assert_eq!(err.code(), "install.unsupported_profile");
        assert!(err.to_string().contains("legacy"), "{err}");
    }

    #[test]
    fn unknown_spec_is_unsupported() {
        let bytes = br#"{"spec": 2, "minecraft": "1.21.1", "json": "/version.json"}"#;
        let err = parse_install_profile(bytes).unwrap_err();
        assert_eq!(err.code(), "install.unsupported_profile");
        assert!(err.to_string().contains("spec"), "{err}");
    }

    #[test]
    fn truncated_json_is_invalid_metadata() {
        let bytes = fixture("forge-1.20.1");
        let err = parse_install_profile(&bytes[..bytes.len() / 2]).unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
        let err = parse_install_profile(b"[]").unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
    }
}
