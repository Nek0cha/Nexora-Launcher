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
    /// 取得せずに、インストーラの `maven/` の展開か processor が置くファイル（`downloads.artifact.url` が空のライブラリ）。
    pub generated: Vec<PathBuf>,
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

/// パスの 1 区切りとして安全な値か。空、`.`、`..`、区切り文字（`/`、`\`、`:`）を含むものを弾く。
fn is_safe_segment(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains(['/', '\\', ':'])
}

/// メタデータの相対パスが、基準のディレクトリの外に出ないか。
/// 通常の区切りだけからなるものを認め、`..`、`.`、ルート、ドライブ、`\`、`:` を含むものは弾く。
/// `\` は Windows で区切りになり、`:` はドライブや代替データストリームになるため、どの OS でも弾く。
/// `components` は途中の `.` と空の区切りを読み飛ばすため、`/` で分けた各部分も確かめる。
/// インストーラの jar のエントリ名にも使う。
pub(crate) fn is_safe_relative(rel: &str) -> bool {
    !rel.is_empty()
        && !rel.contains(['\\', ':'])
        && rel
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
        && std::path::Path::new(rel)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// 安全でない相対パスを `InvalidMetadata` にする。
fn checked_relative(rel: String, library: &str) -> Result<String, CoreError> {
    if is_safe_relative(&rel) {
        Ok(rel)
    } else {
        Err(CoreError::InvalidMetadata {
            message: format!("library {library} has an unsafe path {rel:?}"),
        })
    }
}

/// `group:artifact:version[:classifier][@ext]` を Maven のパスにする。
/// どの部分も、空、`.`、`..`、区切り文字を含むものなら `None`（group の `.` は区切りとして扱う）。
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
    let safe = group.split('.').all(is_safe_segment)
        && [artifact, version, ext]
            .into_iter()
            .chain(classifier)
            .all(is_safe_segment);
    if !safe {
        return None;
    }
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
    let mut generated = Vec::new();

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
            let path = libraries_dir.join(checked_relative(rel, &lib.name)?);
            if artifact.url.is_empty() {
                // URL の無いもの（Forge のクライアントの jar など）は、インストーラが置く
                generated.push(path.clone());
            } else {
                downloads.push(DownloadItem {
                    url: artifact.url.clone(),
                    path: path.clone(),
                    size: Some(artifact.size),
                    sha1: Some(artifact.sha1.clone()),
                    executable: false,
                });
            }
            if on_classpath.insert(path.clone()) {
                classpath.push(path);
            }
        } else if let Some(base) = lib.url.as_deref() {
            // ローダーのライブラリ：その項目の Maven リポジトリから取る。http と https だけを認める
            let scheme_ok =
                reqwest::Url::parse(base).is_ok_and(|u| matches!(u.scheme(), "http" | "https"));
            if !scheme_ok {
                return Err(CoreError::InvalidMetadata {
                    message: format!("library {} has an unsupported url {base:?}", lib.name),
                });
            }
            let rel = maven_path(&lib.name).ok_or_else(|| CoreError::InvalidMetadata {
                message: format!("library {} has no path", lib.name),
            })?;
            let rel = checked_relative(rel, &lib.name)?;
            let base = base.strip_suffix('/').unwrap_or(base);
            let path = libraries_dir.join(&rel);
            downloads.push(DownloadItem {
                url: format!("{base}/{rel}"),
                path: path.clone(),
                size: lib.size,
                sha1: lib.sha1.clone(),
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
            let path = libraries_dir.join(checked_relative(rel, &lib.name)?);
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
    let client_path = client_jar_path(paths, version.jar.as_deref().unwrap_or(&version.id));
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
        generated,
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
    fn plan_uses_the_jar_id_for_the_client_jar() {
        let mut v = fixture("1.21.5");
        v.id = "fabric-loader-0.16.10-1.20.1".into();
        v.jar = Some("1.20.1".into());
        let plan = plan_game(
            &v,
            &empty_index(),
            &env(OsName::Windows, "x86_64"),
            &root(),
            RESOURCES_URL,
        )
        .unwrap();
        assert_eq!(
            rel(plan.classpath.last().unwrap()),
            "shared/versions/1.20.1/1.20.1.jar"
        );
        let client = plan
            .downloads
            .iter()
            .find(|d| d.url == v.downloads.client.url)
            .unwrap();
        assert_eq!(client.path, *plan.classpath.last().unwrap());
    }

    #[test]
    fn plan_downloads_url_only_libraries() {
        let mut v = fixture("1.21.5");
        v.libraries.push(
            serde_json::from_value(serde_json::json!({
                "name": "org.ow2.asm:asm:9.7.1",
                "url": "https://maven.fabricmc.net/",
                "sha1": "f0ed132a49244b042cd0e15702ab9f2ce3cc8436",
                "size": 126093
            }))
            .unwrap(),
        );
        v.libraries.push(
            serde_json::from_value(serde_json::json!({
                "name": "net.fabricmc:intermediary:1.20.1",
                "url": "https://maven.fabricmc.net/"
            }))
            .unwrap(),
        );
        let plan = plan_game(
            &v,
            &empty_index(),
            &env(OsName::Windows, "x86_64"),
            &root(),
            RESOURCES_URL,
        )
        .unwrap();

        let asm = plan
            .downloads
            .iter()
            .find(|d| d.url == "https://maven.fabricmc.net/org/ow2/asm/asm/9.7.1/asm-9.7.1.jar")
            .unwrap();
        assert_eq!(
            rel(&asm.path),
            "shared/libraries/org/ow2/asm/asm/9.7.1/asm-9.7.1.jar"
        );
        assert_eq!(
            asm.sha1.as_deref(),
            Some("f0ed132a49244b042cd0e15702ab9f2ce3cc8436")
        );
        assert_eq!(asm.size, Some(126093));
        assert!(plan.classpath.contains(&asm.path));

        let intermediary = plan
            .downloads
            .iter()
            .find(|d| {
                d.url
                    == "https://maven.fabricmc.net/net/fabricmc/intermediary/1.20.1/intermediary-1.20.1.jar"
            })
            .unwrap();
        assert_eq!(intermediary.sha1, None);
        assert_eq!(intermediary.size, None);
        assert!(plan.classpath.contains(&intermediary.path));
    }

    #[test]
    fn empty_artifact_url_is_generated_not_downloaded() {
        // Forge 1.16.5 の `net.minecraftforge:forge:...` のように、URL が空のライブラリは
        // インストーラの `maven/` か processor が置くもので、取得しない
        let mut v = with_library(serde_json::json!({
            "name": "net.minecraftforge:forge:1.16.5-36.2.39",
            "downloads": { "artifact": {
                "path": "net/minecraftforge/forge/1.16.5-36.2.39/forge-1.16.5-36.2.39.jar",
                "url": "",
                "sha1": "0094f0546dba2d5bcee90e6a0a4f98d3adb40baf",
                "size": 212574
            }}
        }));
        v.libraries.push(
            serde_json::from_value(serde_json::json!({
                "name": "com.example:lib:1.0",
                "downloads": { "artifact": {
                    "path": "com/example/lib/1.0/lib-1.0.jar",
                    "url": "https://example.invalid/lib.jar",
                    "sha1": "0000000000000000000000000000000000000000",
                    "size": 1
                }}
            }))
            .unwrap(),
        );
        let plan = plan_of(&v).unwrap();
        let forge =
            "shared/libraries/net/minecraftforge/forge/1.16.5-36.2.39/forge-1.16.5-36.2.39.jar";
        assert!(plan.downloads.iter().all(|d| rel(&d.path) != forge));
        assert!(plan.downloads.iter().all(|d| !d.url.is_empty()));
        assert!(plan.classpath.iter().any(|p| rel(p) == forge));
        assert_eq!(
            plan.generated.iter().map(|p| rel(p)).collect::<Vec<_>>(),
            vec![forge.to_string()]
        );
        // URL のあるものは従来どおり取得し、generated には入らない
        let lib = plan
            .downloads
            .iter()
            .find(|d| d.url == "https://example.invalid/lib.jar")
            .unwrap();
        assert_eq!(
            rel(&lib.path),
            "shared/libraries/com/example/lib/1.0/lib-1.0.jar"
        );
        assert!(plan.classpath.contains(&lib.path));
        // Vanilla の計画には generated が無い
        assert!(plan_of(&fixture("1.21.5")).unwrap().generated.is_empty());
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

    #[test]
    fn maven_path_rejects_unsafe_segments() {
        for name in [
            // 空の部分
            ":c:1",
            "a..b:c:1",
            ".a:c:1",
            "a.:c:1",
            "a::1",
            "a:c:",
            "a:c:1:",
            "a:c:1@",
            // . と ..
            "..:c:1",
            "a:..:1",
            "a:.:1",
            "a:c:..",
            "a:c:1:..",
            "a:c:1@..",
            // 区切り文字
            "a/b:c:1",
            "a:c/d:1",
            "a:c:1/../../x",
            "a:c:1:x/y",
            "a:c:1@x/y",
            r"a\b:c:1",
            r"a:c:..\..\x",
            "a:c:1@x:y",
        ] {
            assert_eq!(maven_path(name), None, "{name}");
        }
    }

    fn with_library(lib: serde_json::Value) -> VersionJson {
        let mut v = fixture("1.21.5");
        v.libraries.push(serde_json::from_value(lib).unwrap());
        v
    }

    fn plan_of(v: &VersionJson) -> Result<GamePlan, CoreError> {
        plan_game(
            v,
            &empty_index(),
            &env(OsName::Windows, "x86_64"),
            &root(),
            RESOURCES_URL,
        )
    }

    #[test]
    fn artifact_paths_must_stay_inside_the_libraries_dir() {
        for path in [
            "../../evil.jar",
            "a/../../evil.jar",
            "/abs/evil.jar",
            "./a/evil.jar",
            "a/./evil.jar",
            "a//evil.jar",
            "a/evil.jar/",
            r"a\..\..\evil.jar",
            r"C:\evil.jar",
            "C:evil.jar",
            "",
        ] {
            let v = with_library(serde_json::json!({
                "name": "com.example:lib:1.0",
                "downloads": { "artifact": {
                    "path": path,
                    "url": "https://example.invalid/lib.jar",
                    "sha1": "0000000000000000000000000000000000000000",
                    "size": 1
                }}
            }));
            let err = plan_of(&v).unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "{path}");
        }
    }

    #[test]
    fn url_only_libraries_reject_unsafe_names() {
        for name in ["com.example:..:1.0", "com.example:lib:../../x", r"a\..:b:1"] {
            let v = with_library(serde_json::json!({
                "name": name,
                "url": "https://maven.example/"
            }));
            let err = plan_of(&v).unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "{name}");
        }
    }

    #[test]
    fn url_only_libraries_need_an_http_url() {
        for url in ["file:///etc/", "ftp://maven.example/", "maven.example/", ""] {
            let v = with_library(serde_json::json!({
                "name": "com.example:lib:1.0",
                "url": url
            }));
            let err = plan_of(&v).unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "{url}");
        }
        let v = with_library(serde_json::json!({
            "name": "com.example:lib:1.0",
            "url": "http://maven.example/"
        }));
        assert!(plan_of(&v).is_ok());
    }

    #[test]
    fn url_only_base_without_trailing_slash_gets_one() {
        let v = with_library(serde_json::json!({
            "name": "com.example:lib:1.0",
            "url": "https://maven.example/repo"
        }));
        let plan = plan_of(&v).unwrap();
        let lib = plan
            .downloads
            .iter()
            .find(|d| rel(&d.path) == "shared/libraries/com/example/lib/1.0/lib-1.0.jar")
            .unwrap();
        assert_eq!(
            lib.url,
            "https://maven.example/repo/com/example/lib/1.0/lib-1.0.jar"
        );
    }
}
