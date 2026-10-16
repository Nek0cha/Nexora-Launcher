//! version JSON から、取得するファイル、classpath、natives を求める。副作用を持たない。

use std::collections::HashSet;
use std::path::PathBuf;

use serde::Serialize;

use crate::error::CoreError;
use crate::meta::mojang::manifest::client_jar_path;
use crate::meta::mojang::rules::{RuleEnv, allowed};
use crate::meta::mojang::version::{AssetIndex, VersionJson};
use crate::net::DownloadItem;
use crate::paths::AppPaths;

pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeJar {
    pub path: PathBuf,
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoggingConfig {
    pub path: PathBuf,
    /// `${path}` を置き換え済みの JVM 引数。
    pub argument: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GamePlan {
    pub downloads: Vec<DownloadItem>,
    pub classpath: Vec<PathBuf>,
    pub natives: Vec<NativeJar>,
    pub logging: Option<LoggingConfig>,
}

fn unsupported(reason: &str) -> CoreError {
    CoreError::UnsupportedProfile {
        reason: reason.to_string(),
    }
}

/// フェーズ 2 で扱えない version JSON を弾く。
pub fn check_profile(v: &VersionJson) -> Result<(), CoreError> {
    if v.inherits_from.is_some() {
        return Err(unsupported("inheritsFrom"));
    }
    Ok(())
}

/// `group:artifact:version[:classifier][@ext]` を Maven のパスにする。
pub fn maven_path(name: &str) -> Option<String> {
    let (coords, ext) = match name.split_once('@') {
        Some((c, e)) => (c, e),
        None => (name, "jar"),
    };
    let parts: Vec<&str> = coords.split(':').collect();
    let (group, artifact, version, classifier) = match parts.as_slice() {
        [g, a, v] => (*g, *a, *v, None),
        [g, a, v, c] => (*g, *a, *v, Some(*c)),
        _ => return None,
    };
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Some(format!(
        "{}/{artifact}/{version}/{file}",
        group.replace('.', "/")
    ))
}

pub fn plan_game(
    version: &VersionJson,
    asset_index: &AssetIndex,
    env: &RuleEnv,
    paths: &AppPaths,
    resources_url: &str,
) -> Result<GamePlan, CoreError> {
    if asset_index.is_virtual || asset_index.map_to_resources {
        return Err(unsupported("legacy assets"));
    }
    let libraries_dir = paths.libraries_dir();
    let mut downloads = Vec::new();
    let mut classpath = Vec::new();
    let mut on_classpath = HashSet::new();
    let mut natives = Vec::new();

    for lib in &version.libraries {
        if !allowed(lib.rules.as_deref(), env) {
            continue;
        }
        let lib_downloads = lib.downloads.as_ref();
        if let Some(artifact) = lib_downloads.and_then(|d| d.artifact.as_ref()) {
            let rel = artifact
                .path
                .clone()
                .or_else(|| maven_path(&lib.name))
                .ok_or_else(|| CoreError::InvalidMetadata {
                    message: format!("library {} has no path", lib.name),
                })?;
            let path = libraries_dir.join(rel);
            downloads.push(DownloadItem {
                url: artifact.url.clone(),
                path: path.clone(),
                size: Some(artifact.size),
                sha1: Some(artifact.sha1.clone()),
                executable: false,
            });
            if on_classpath.insert(path.clone()) {
                classpath.push(path);
            }
        }
        // 旧形式の natives：classifier の jar を展開して使う
        if let Some(classifier) = lib.natives.as_ref().and_then(|n| n.get(env.os.as_str())) {
            let bits = if env.arch == "x86" { "32" } else { "64" };
            let classifier = classifier.replace("${arch}", bits);
            let Some(artifact) = lib_downloads
                .and_then(|d| d.classifiers.as_ref())
                .and_then(|c| c.get(&classifier))
            else {
                tracing::warn!(library = %lib.name, %classifier, "natives classifier is missing");
                continue;
            };
            let rel = artifact
                .path
                .clone()
                .or_else(|| maven_path(&format!("{}:{classifier}", lib.name)))
                .ok_or_else(|| CoreError::InvalidMetadata {
                    message: format!("natives of {} have no path", lib.name),
                })?;
            let path = libraries_dir.join(rel);
            downloads.push(DownloadItem {
                url: artifact.url.clone(),
                path: path.clone(),
                size: Some(artifact.size),
                sha1: Some(artifact.sha1.clone()),
                executable: false,
            });
            natives.push(NativeJar {
                path,
                exclude: lib.extract.clone().unwrap_or_default().exclude,
            });
        }
    }

    let client = &version.downloads.client;
    let client_path = client_jar_path(paths, &version.id);
    downloads.push(DownloadItem {
        url: client.url.clone(),
        path: client_path.clone(),
        size: Some(client.size),
        sha1: Some(client.sha1.clone()),
        executable: false,
    });
    classpath.push(client_path);

    let objects_dir = paths.assets_dir().join("objects");
    let mut seen_hashes = HashSet::new();
    for object in asset_index.objects.values() {
        if !seen_hashes.insert(object.hash.as_str()) || object.hash.len() < 2 {
            continue;
        }
        let prefix = &object.hash[..2];
        downloads.push(DownloadItem {
            url: format!("{resources_url}/{prefix}/{}", object.hash),
            path: objects_dir.join(prefix).join(&object.hash),
            size: Some(object.size),
            sha1: Some(object.hash.clone()),
            executable: false,
        });
    }

    let logging = match version.logging.as_ref().and_then(|l| l.client.as_ref()) {
        Some(entry) => {
            let path = paths.assets_dir().join("log_configs").join(&entry.file.id);
            downloads.push(DownloadItem {
                url: entry.file.url.clone(),
                path: path.clone(),
                size: Some(entry.file.size),
                sha1: Some(entry.file.sha1.clone()),
                executable: false,
            });
            Some(LoggingConfig {
                argument: entry
                    .argument
                    .replace("${path}", &path.display().to_string()),
                path,
            })
        }
        None => None,
    };

    Ok(GamePlan {
        downloads,
        classpath,
        natives,
        logging,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::mojang::rules::OsName;
    use crate::meta::mojang::version::AssetIndex;

    fn fixture(name: &str) -> VersionJson {
        let path = format!(
            "{}/tests/fixtures/versions/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn env(os: OsName, arch: &str) -> RuleEnv {
        RuleEnv {
            os,
            os_version: if os == OsName::Windows {
                "10.0"
            } else {
                "14.5"
            }
            .into(),
            arch: arch.into(),
        }
    }

    fn empty_index() -> AssetIndex {
        serde_json::from_str(r#"{"objects":{}}"#).unwrap()
    }

    fn root() -> AppPaths {
        AppPaths::from_root("/data")
    }

    /// スナップショット用に、ルートからの相対パスを / 区切りで返す。
    fn rel(p: &std::path::Path) -> String {
        p.strip_prefix("/data")
            .unwrap()
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    }

    fn summary(plan: &GamePlan) -> serde_json::Value {
        serde_json::json!({
            "classpath": plan.classpath.iter().map(|p| rel(p)).collect::<Vec<_>>(),
            "natives": plan.natives.iter().map(|n| (rel(&n.path), n.exclude.clone())).collect::<Vec<_>>(),
            "logging": plan.logging.as_ref().map(|l| rel(&l.path)),
            "downloads": plan.downloads.len(),
        })
    }

    #[test]
    fn plan_1_8_9_windows() {
        let plan = plan_game(
            &fixture("1.8.9"),
            &empty_index(),
            &env(OsName::Windows, "x86_64"),
            &root(),
            RESOURCES_URL,
        )
        .unwrap();
        insta::assert_json_snapshot!(summary(&plan));
        assert!(rel(plan.classpath.last().unwrap()).ends_with("versions/1.8.9/1.8.9.jar"));
        assert!(
            plan.natives
                .iter()
                .any(|n| rel(&n.path).contains("natives-windows"))
        );
        assert!(
            plan.natives
                .iter()
                .all(|n| !rel(&n.path).contains("natives-osx"))
        );
    }

    #[test]
    fn plan_1_12_2_linux() {
        let plan = plan_game(
            &fixture("1.12.2"),
            &empty_index(),
            &env(OsName::Linux, "x86_64"),
            &root(),
            RESOURCES_URL,
        )
        .unwrap();
        insta::assert_json_snapshot!(summary(&plan));
        assert!(
            plan.natives
                .iter()
                .all(|n| rel(&n.path).contains("natives-linux"))
        );
    }

    #[test]
    fn plan_1_21_5_macos_arm() {
        let plan = plan_game(
            &fixture("1.21.5"),
            &empty_index(),
            &env(OsName::Osx, "aarch64"),
            &root(),
            RESOURCES_URL,
        )
        .unwrap();
        insta::assert_json_snapshot!(summary(&plan));
        // 新しい形式の natives は classpath に載り、展開はしない
        assert!(plan.natives.is_empty());
        assert!(
            plan.classpath
                .iter()
                .any(|p| rel(p).contains("natives-macos-arm64"))
        );
    }

    #[test]
    fn plan_latest_windows() {
        let plan = plan_game(
            &fixture("latest-release"),
            &empty_index(),
            &env(OsName::Windows, "x86_64"),
            &root(),
            RESOURCES_URL,
        )
        .unwrap();
        assert!(
            plan.classpath
                .iter()
                .any(|p| rel(p).contains("natives-windows"))
        );
        assert!(
            plan.classpath
                .iter()
                .all(|p| !rel(p).contains("natives-linux"))
        );
    }

    #[test]
    fn classpath_has_no_duplicates() {
        for name in ["1.8.9", "1.12.2", "1.13", "1.21.5", "latest-release"] {
            let plan = plan_game(
                &fixture(name),
                &empty_index(),
                &env(OsName::Windows, "x86_64"),
                &root(),
                RESOURCES_URL,
            )
            .unwrap();
            let mut seen = std::collections::HashSet::new();
            assert!(
                plan.classpath.iter().all(|p| seen.insert(p.clone())),
                "{name}"
            );
        }
    }

    #[test]
    fn assets_are_planned_by_hash_once() {
        let index: AssetIndex = serde_json::from_str(
            r#"{"objects":{
            "a.png":{"hash":"bdf48ef6b5d0d23bbb02e17d04865216179f510a","size":3},
            "b.png":{"hash":"bdf48ef6b5d0d23bbb02e17d04865216179f510a","size":3}}}"#,
        )
        .unwrap();
        let plan = plan_game(
            &fixture("1.8.9"),
            &index,
            &env(OsName::Windows, "x86_64"),
            &root(),
            "http://res",
        )
        .unwrap();
        let assets: Vec<_> = plan
            .downloads
            .iter()
            .filter(|d| d.url.starts_with("http://res"))
            .collect();
        assert_eq!(assets.len(), 1);
        assert_eq!(
            assets[0].url,
            "http://res/bd/bdf48ef6b5d0d23bbb02e17d04865216179f510a"
        );
        assert_eq!(
            rel(&assets[0].path),
            "shared/assets/objects/bd/bdf48ef6b5d0d23bbb02e17d04865216179f510a"
        );
    }

    #[test]
    fn legacy_asset_formats_are_unsupported() {
        let index: AssetIndex = serde_json::from_str(r#"{"virtual":true,"objects":{}}"#).unwrap();
        let err = plan_game(
            &fixture("1.8.9"),
            &index,
            &env(OsName::Windows, "x86_64"),
            &root(),
            RESOURCES_URL,
        )
        .unwrap_err();
        assert_eq!(err.code(), "install.unsupported_profile");
    }

    #[test]
    fn inherits_from_is_unsupported() {
        let mut v = fixture("1.8.9");
        v.inherits_from = Some("1.8.8".into());
        assert_eq!(
            check_profile(&v).unwrap_err().code(),
            "install.unsupported_profile"
        );
        assert!(check_profile(&fixture("1.8.9")).is_ok());
    }

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path("com.example:lib:1.0").unwrap(),
            "com/example/lib/1.0/lib-1.0.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );
        assert_eq!(maven_path("a.b:c:1@zip").unwrap(), "a/b/c/1/c-1.zip");
        assert!(maven_path("broken").is_none());
    }
}
