//! `modrinth.index.json` の型と解釈、受け入れ条件。

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::content::Side;
use crate::error::CoreError;
use crate::meta::loader::LoaderKind;

/// 索引の 1 ファイル分。`sha1` と `sha512` は小文字にそろえて持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFile {
    pub path: String,
    pub sha1: String,
    pub sha512: String,
    pub env_client: Option<Side>,
    pub env_server: Option<Side>,
    pub downloads: Vec<String>,
    pub file_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub version_id: String,
    pub name: String,
    pub files: Vec<PackFile>,
    pub dependencies: BTreeMap<String, String>,
}

/// パックが求めるゲームの版とローダー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackTarget {
    pub mc_version: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawManifest {
    version_id: String,
    name: String,
    #[serde(default)]
    files: Vec<RawFile>,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFile {
    path: String,
    hashes: RawHashes,
    env: Option<RawEnv>,
    #[serde(default)]
    downloads: Vec<String>,
    file_size: u64,
}

#[derive(Deserialize)]
struct RawHashes {
    sha1: String,
    sha512: String,
}

#[derive(Deserialize)]
struct RawEnv {
    client: Option<String>,
    server: Option<String>,
}

fn invalid(reason: impl Into<String>) -> CoreError {
    CoreError::MrpackInvalid {
        reason: reason.into(),
    }
}

fn unsupported(reason: impl Into<String>) -> CoreError {
    CoreError::MrpackUnsupported {
        reason: reason.into(),
    }
}

fn hex_hash(value: &str, len: usize, what: &str) -> Result<String, CoreError> {
    if value.len() == len && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(value.to_ascii_lowercase())
    } else {
        Err(invalid(format!("{what} is not a {len}-digit hex string")))
    }
}

fn side(value: Option<String>, what: &str) -> Result<Option<Side>, CoreError> {
    match value {
        None => Ok(None),
        Some(s) => Side::parse(&s)
            .map(Some)
            .ok_or_else(|| invalid(format!("{what} has an unknown value: {s}"))),
    }
}

/// `dependencies` のうちローダーのキー。
const LOADER_KEYS: [(&str, LoaderKind); 4] = [
    ("fabric-loader", LoaderKind::Fabric),
    ("quilt-loader", LoaderKind::Quilt),
    ("forge", LoaderKind::Forge),
    ("neoforge", LoaderKind::Neoforge),
];

impl RawFile {
    fn into_file(self) -> Result<PackFile, CoreError> {
        if self.downloads.is_empty() {
            return Err(invalid(format!("{} has no download URL", self.path)));
        }
        let (env_client, env_server) = match self.env {
            Some(env) => (
                side(env.client, "env.client")?,
                side(env.server, "env.server")?,
            ),
            None => (None, None),
        };
        Ok(PackFile {
            sha1: hex_hash(&self.hashes.sha1, 40, "sha1")?,
            sha512: hex_hash(&self.hashes.sha512, 128, "sha512")?,
            path: self.path,
            env_client,
            env_server,
            downloads: self.downloads,
            file_size: self.file_size,
        })
    }
}

/// 索引を読み、受け入れ条件（`formatVersion` 1、`game` が minecraft、依存のキー）を確かめる。
pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, CoreError> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| invalid(format!("not valid JSON: {e}")))?;
    match value.get("formatVersion").and_then(|v| v.as_u64()) {
        Some(1) => {}
        Some(n) => return Err(unsupported(format!("formatVersion {n}"))),
        None => return Err(invalid("formatVersion is missing")),
    }
    match value.get("game").and_then(|v| v.as_str()) {
        Some("minecraft") => {}
        Some(other) => return Err(unsupported(format!("game {other}"))),
        None => return Err(invalid("game is missing")),
    }
    let raw: RawManifest =
        serde_json::from_value(value).map_err(|e| invalid(format!("malformed index: {e}")))?;
    let manifest = Manifest {
        version_id: raw.version_id,
        name: raw.name,
        files: raw
            .files
            .into_iter()
            .map(RawFile::into_file)
            .collect::<Result<_, _>>()?,
        dependencies: raw.dependencies,
    };
    manifest.target()?;
    Ok(manifest)
}

impl Manifest {
    /// 求められるゲームの版とローダー。ローダーのキーが無ければ Vanilla。
    pub fn target(&self) -> Result<PackTarget, CoreError> {
        let mc_version = self
            .dependencies
            .get("minecraft")
            .ok_or_else(|| invalid("dependencies has no minecraft"))?
            .clone();
        let mut loaders = Vec::new();
        for (key, version) in &self.dependencies {
            if key == "minecraft" {
                continue;
            }
            match LOADER_KEYS.iter().find(|(k, _)| k == key) {
                Some((_, kind)) => loaders.push((*kind, version.clone())),
                None => return Err(unsupported(format!("unknown dependency {key}"))),
            }
        }
        match loaders.len() {
            0 => Ok(PackTarget {
                mc_version,
                loader: LoaderKind::Vanilla,
                loader_version: None,
            }),
            1 => {
                let (loader, version) = loaders.remove(0);
                Ok(PackTarget {
                    mc_version,
                    loader,
                    loader_version: Some(version),
                })
            }
            _ => Err(invalid("more than one mod loader is listed")),
        }
    }

    /// クライアントで使うファイル（`env.client` が `unsupported` のものを除く）。
    pub fn client_files(&self) -> Vec<&PackFile> {
        self.files
            .iter()
            .filter(|f| f.env_client != Some(Side::Unsupported))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::loader::LoaderKind;
    use serde_json::{Value, json};

    const SHA1: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn sha512() -> String {
        "b".repeat(128)
    }

    fn file(path: &str, env: Option<Value>) -> Value {
        let mut v = json!({
            "path": path,
            "hashes": { "sha1": SHA1, "sha512": sha512() },
            "downloads": ["https://cdn.modrinth.com/data/x/versions/y/a.jar"],
            "fileSize": 10,
        });
        if let Some(env) = env {
            v["env"] = env;
        }
        v
    }

    fn index(deps: Value, files: Vec<Value>) -> Value {
        json!({
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1.0.0",
            "name": "Test pack",
            "files": files,
            "dependencies": deps,
        })
    }

    fn parse(v: &Value) -> Result<Manifest, crate::error::CoreError> {
        parse_manifest(&serde_json::to_vec(v).unwrap())
    }

    #[test]
    fn parses_a_minimal_fabric_pack() {
        let m = parse(&index(
            json!({ "minecraft": "1.21.1", "fabric-loader": "0.16.10" }),
            vec![file("mods/a.jar", None)],
        ))
        .unwrap();
        assert_eq!(m.name, "Test pack");
        assert_eq!(m.version_id, "1.0.0");
        let t = m.target().unwrap();
        assert_eq!(t.mc_version, "1.21.1");
        assert_eq!(t.loader, LoaderKind::Fabric);
        assert_eq!(t.loader_version.as_deref(), Some("0.16.10"));
        assert_eq!(m.files[0].sha1, SHA1);
    }

    #[test]
    fn a_pack_without_a_loader_key_is_vanilla() {
        let m = parse(&index(json!({ "minecraft": "1.20.4" }), vec![])).unwrap();
        let t = m.target().unwrap();
        assert_eq!(t.loader, LoaderKind::Vanilla);
        assert_eq!(t.loader_version, None);
    }

    #[test]
    fn every_loader_key_maps_to_its_kind() {
        for (key, kind) in [
            ("fabric-loader", LoaderKind::Fabric),
            ("quilt-loader", LoaderKind::Quilt),
            ("forge", LoaderKind::Forge),
            ("neoforge", LoaderKind::Neoforge),
        ] {
            let m = parse(&index(
                json!({ "minecraft": "1.21.1", key: "1.2.3" }),
                vec![],
            ))
            .unwrap();
            assert_eq!(m.target().unwrap().loader, kind, "{key}");
        }
    }

    #[test]
    fn hashes_are_normalised_to_lowercase() {
        let mut f = file("mods/a.jar", None);
        f["hashes"]["sha1"] = json!(SHA1.to_uppercase());
        let m = parse(&index(json!({ "minecraft": "1.21.1" }), vec![f])).unwrap();
        assert_eq!(m.files[0].sha1, SHA1);
    }

    #[test]
    fn rejects_with_the_right_error() {
        let ok_deps = || json!({ "minecraft": "1.21.1" });
        let mut format2 = index(ok_deps(), vec![]);
        format2["formatVersion"] = json!(2);
        let mut terraria = index(ok_deps(), vec![]);
        terraria["game"] = json!("terraria");
        let mut no_sha512 = file("mods/a.jar", None);
        no_sha512["hashes"] = json!({ "sha1": SHA1 });
        let mut short_sha1 = file("mods/a.jar", None);
        short_sha1["hashes"]["sha1"] = json!("abc");
        let mut no_downloads = file("mods/a.jar", None);
        no_downloads["downloads"] = json!([]);
        let bad_env = file("mods/a.jar", Some(json!({ "client": "maybe" })));

        let cases: Vec<(&str, Vec<u8>, &str)> = vec![
            (
                "format 2",
                serde_json::to_vec(&format2).unwrap(),
                "mrpack.unsupported",
            ),
            (
                "other game",
                serde_json::to_vec(&terraria).unwrap(),
                "mrpack.unsupported",
            ),
            (
                "no minecraft",
                serde_json::to_vec(&index(json!({ "fabric-loader": "0.1" }), vec![])).unwrap(),
                "mrpack.invalid",
            ),
            (
                "two loaders",
                serde_json::to_vec(&index(
                    json!({ "minecraft": "1.21.1", "fabric-loader": "0.1", "forge": "1" }),
                    vec![],
                ))
                .unwrap(),
                "mrpack.invalid",
            ),
            (
                "unknown key",
                serde_json::to_vec(&index(
                    json!({ "minecraft": "1.21.1", "mystery-loader": "1" }),
                    vec![],
                ))
                .unwrap(),
                "mrpack.unsupported",
            ),
            (
                "no sha512",
                serde_json::to_vec(&index(ok_deps(), vec![no_sha512])).unwrap(),
                "mrpack.invalid",
            ),
            (
                "short sha1",
                serde_json::to_vec(&index(ok_deps(), vec![short_sha1])).unwrap(),
                "mrpack.invalid",
            ),
            (
                "no downloads",
                serde_json::to_vec(&index(ok_deps(), vec![no_downloads])).unwrap(),
                "mrpack.invalid",
            ),
            (
                "unknown env value",
                serde_json::to_vec(&index(ok_deps(), vec![bad_env])).unwrap(),
                "mrpack.invalid",
            ),
            ("not json", b"not json".to_vec(), "mrpack.invalid"),
        ];
        for (label, bytes, code) in cases {
            let err = parse_manifest(&bytes).unwrap_err();
            assert_eq!(err.code(), code, "{label}");
        }
    }

    #[test]
    fn client_files_skips_unsupported_and_keeps_optional_and_missing_env() {
        let m = parse(&index(
            json!({ "minecraft": "1.21.1" }),
            vec![
                file(
                    "mods/required.jar",
                    Some(json!({ "client": "required", "server": "required" })),
                ),
                file("mods/optional.jar", Some(json!({ "client": "optional" }))),
                file(
                    "mods/server-only.jar",
                    Some(json!({ "client": "unsupported", "server": "required" })),
                ),
                file("mods/no-env.jar", None),
            ],
        ))
        .unwrap();
        let paths: Vec<&str> = m.client_files().iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            ["mods/required.jar", "mods/optional.jar", "mods/no-env.jar"]
        );
    }
}
