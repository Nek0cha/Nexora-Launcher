//! Mojang の Java runtime の索引と manifest。

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::error::CoreError;
use crate::net::DownloadItem;
use crate::paths::AppPaths;

/// all.json：プラットフォーム → component → 候補の一覧。
pub type RuntimeIndex = BTreeMap<String, BTreeMap<String, Vec<Candidate>>>;

#[derive(Debug, Clone, Deserialize)]
pub struct Candidate {
    pub availability: Availability,
    pub manifest: ManifestRef,
    pub version: CandidateVersion,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Availability {
    pub group: u32,
    pub progress: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestRef {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CandidateVersion {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeManifest {
    pub files: BTreeMap<String, RuntimeFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RuntimeFile {
    File {
        #[serde(default)]
        executable: bool,
        downloads: FileDownloads,
    },
    Directory,
    Link {
        target: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileDownloads {
    pub raw: RawDownload,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawDownload {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

/// 完全に公開済み（progress 100）の最初の候補を選ぶ。
pub fn select_candidate<'a>(
    index: &'a RuntimeIndex,
    platform: &str,
    component: &str,
) -> Result<&'a Candidate, CoreError> {
    index
        .get(platform)
        .and_then(|c| c.get(component))
        .and_then(|list| list.iter().find(|c| c.availability.progress >= 100))
        .ok_or_else(|| CoreError::NoCompatibleRuntime {
            component: component.to_string(),
            platform: platform.to_string(),
        })
}

pub fn java_relative_path(os: &str) -> &'static str {
    match os {
        "windows" => "bin/java.exe",
        "macos" => "jre.bundle/Contents/Home/bin/java",
        _ => "bin/java",
    }
}

pub fn runtime_dir(
    paths: &AppPaths,
    platform: &str,
    component: &str,
    manifest_sha1: &str,
) -> PathBuf {
    paths
        .runtimes_dir()
        .join(platform)
        .join(component)
        .join(manifest_sha1)
}

/// manifest の相対パスを dir の下に解決する。dir の外を指すものは拒否する。
fn resolve(dir: &Path, rel: &str) -> Result<PathBuf, CoreError> {
    let rel_path = Path::new(rel);
    let safe = rel_path
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    if !safe || rel.is_empty() {
        return Err(CoreError::InvalidMetadata {
            message: format!("runtime path escapes its directory: {rel}"),
        });
    }
    Ok(rel
        .split('/')
        .fold(dir.to_path_buf(), |p, part| p.join(part)))
}

pub type PlannedManifest = (Vec<DownloadItem>, Vec<PathBuf>, Vec<(PathBuf, String)>);

/// ファイル、ディレクトリ、リンクに分ける。ファイルは非圧縮の raw だけを使う。
pub fn plan_manifest(manifest: &RuntimeManifest, dir: &Path) -> Result<PlannedManifest, CoreError> {
    let mut downloads = Vec::new();
    let mut dirs = Vec::new();
    let mut links = Vec::new();
    for (rel, entry) in &manifest.files {
        let path = resolve(dir, rel)?;
        match entry {
            RuntimeFile::File {
                executable,
                downloads: d,
            } => downloads.push(DownloadItem {
                url: d.raw.url.clone(),
                path,
                size: Some(d.raw.size),
                sha1: Some(d.raw.sha1.clone()),
                executable: *executable,
            }),
            RuntimeFile::Directory => dirs.push(path),
            RuntimeFile::Link { target } => links.push((path, target.clone())),
        }
    }
    Ok((downloads, dirs, links))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> RuntimeIndex {
        serde_json::from_str(r#"{
          "windows-x64": {
            "jre-legacy": [
              {"availability":{"group":1,"progress":50},"manifest":{"sha1":"a","size":1,"url":"u1"},"version":{"name":"8u51","released":"r"}},
              {"availability":{"group":2,"progress":100},"manifest":{"sha1":"b","size":1,"url":"u2"},"version":{"name":"8u51","released":"r"}}
            ],
            "java-runtime-delta": []
          }
        }"#).unwrap()
    }

    #[test]
    fn selects_fully_available_candidate() {
        let idx = index();
        let c = select_candidate(&idx, "windows-x64", "jre-legacy").unwrap();
        assert_eq!(c.manifest.sha1, "b");
    }

    #[test]
    fn missing_or_empty_component_is_no_compatible_runtime() {
        let idx = index();
        for (platform, component) in [
            ("windows-x64", "java-runtime-delta"),
            ("windows-x64", "nope"),
            ("mac-os-arm64", "jre-legacy"),
        ] {
            let err = select_candidate(&idx, platform, component).unwrap_err();
            assert_eq!(err.code(), "java.no_compatible_runtime");
        }
    }

    #[test]
    fn manifest_entries_are_planned() {
        let manifest: RuntimeManifest = serde_json::from_str(r#"{"files":{
            "bin": {"type":"directory"},
            "bin/java": {"type":"file","executable":true,"downloads":{"raw":{"sha1":"s","size":3,"url":"http://r/java"},"lzma":{"sha1":"l","size":2,"url":"http://r/java.lzma"}}},
            "lib/libjli.dylib": {"type":"link","target":"../jli/libjli.dylib"}
        }}"#).unwrap();
        let dir = std::path::PathBuf::from("/rt");
        let (downloads, dirs, links) = plan_manifest(&manifest, &dir).unwrap();
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].url, "http://r/java");
        assert!(downloads[0].executable);
        assert_eq!(downloads[0].path, dir.join("bin").join("java"));
        assert_eq!(dirs, vec![dir.join("bin")]);
        assert_eq!(
            links,
            vec![(
                dir.join("lib").join("libjli.dylib"),
                "../jli/libjli.dylib".to_string()
            )]
        );
    }

    #[test]
    fn escaping_paths_are_rejected() {
        for bad in [
            r#"{"files":{"../evil":{"type":"directory"}}}"#,
            r#"{"files":{"/abs":{"type":"directory"}}}"#,
        ] {
            let manifest: RuntimeManifest = serde_json::from_str(bad).unwrap();
            let err = plan_manifest(&manifest, std::path::Path::new("/rt")).unwrap_err();
            assert_eq!(err.code(), "meta.invalid");
        }
    }

    #[test]
    fn java_paths_per_os() {
        assert_eq!(java_relative_path("windows"), "bin/java.exe");
        assert_eq!(
            java_relative_path("macos"),
            "jre.bundle/Contents/Home/bin/java"
        );
        assert_eq!(java_relative_path("linux"), "bin/java");
    }
}
