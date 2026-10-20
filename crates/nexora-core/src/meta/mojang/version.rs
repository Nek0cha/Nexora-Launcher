//! version JSON と asset index の型。読むのに必要なフィールドだけを持つ。

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub main_class: String,
    pub inherits_from: Option<String>,
    /// クライアントの jar を持つバージョンの id。無ければ `id` 自身の jar を使う。
    #[serde(default)]
    pub jar: Option<String>,
    pub arguments: Option<Arguments>,
    pub minecraft_arguments: Option<String>,
    pub asset_index: AssetIndexRef,
    pub downloads: ClientDownloads,
    #[serde(default)]
    pub libraries: Vec<Library>,
    pub java_version: Option<JavaVersion>,
    pub logging: Option<Logging>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<ArgValue>,
    #[serde(default)]
    pub jvm: Vec<ArgValue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: ArgValues },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgValues {
    One(String),
    Many(Vec<String>),
}

impl ArgValues {
    pub fn as_slice(&self) -> &[String] {
        match self {
            ArgValues::One(s) => std::slice::from_ref(s),
            ArgValues::Many(v) => v,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    pub os: Option<OsRule>,
    pub features: Option<BTreeMap<String, bool>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub version: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub rules: Option<Vec<Rule>>,
    pub natives: Option<BTreeMap<String, String>>,
    pub extract: Option<Extract>,
    /// ローダーのライブラリが持つ Maven リポジトリの基底 URL（Vanilla は持たない）。
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<BTreeMap<String, Artifact>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub path: Option<String>,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientDownloads {
    pub client: Artifact,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub component: String,
    pub major_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Logging {
    pub client: Option<LoggingEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingEntry {
    pub argument: String,
    pub file: LoggingFile,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    #[serde(default)]
    pub objects: BTreeMap<String, AssetObject>,
    #[serde(default, rename = "virtual")]
    pub is_virtual: bool,
    #[serde(default)]
    pub map_to_resources: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn fixture(name: &str) -> VersionJson {
        let path = format!(
            "{}/tests/fixtures/versions/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn all_fixtures_parse() {
        for name in ["1.8.9", "1.12.2", "1.13", "1.21.5", "latest-release"] {
            let v = fixture(name);
            assert!(!v.libraries.is_empty(), "{name} has no libraries");
            assert!(v.inherits_from.is_none(), "{name} inherits");
            assert_eq!(v.kind, "release");
        }
    }

    #[test]
    fn legacy_and_modern_argument_formats() {
        let old = fixture("1.12.2");
        assert!(old.minecraft_arguments.is_some());
        assert!(old.arguments.is_none());
        let new = fixture("1.13");
        assert!(new.minecraft_arguments.is_none());
        let args = new.arguments.unwrap();
        assert!(!args.game.is_empty());
        assert!(!args.jvm.is_empty());
    }

    #[test]
    fn java_requirement_of_1_8_9() {
        // spec 5.5：1.8.9 に javaVersion があるかを実物で確かめる
        let v = fixture("1.8.9");
        let java = v.java_version.expect("1.8.9 declares javaVersion");
        assert_eq!(java.component, "jre-legacy");
        assert_eq!(java.major_version, 8);
    }

    #[test]
    fn asset_index_shape() {
        let json = r#"{"objects":{"icons/icon_16x16.png":{"hash":"bdf48ef6b5d0d23bbb02e17d04865216179f510a","size":3665}}}"#;
        let index: AssetIndex = serde_json::from_str(json).unwrap();
        assert!(!index.is_virtual);
        assert!(!index.map_to_resources);
        assert_eq!(index.objects["icons/icon_16x16.png"].size, 3665);
        let legacy: AssetIndex = serde_json::from_str(r#"{"virtual":true,"objects":{}}"#).unwrap();
        assert!(legacy.is_virtual);
    }
}
