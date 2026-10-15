//! version_manifest_v2.json の取得と、version JSON と asset index の固定。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use super::version::{AssetIndex, VersionJson};
use crate::error::CoreError;
use crate::meta::{fetch_cached, parse_json};
use crate::net::{Depth, DownloadItem, Http, fetch_all, io_err};
use crate::paths::AppPaths;

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
/// 対応する最も古いバージョン。これより前に公開された正式版は一覧に出さない。
pub const OLDEST_SUPPORTED: &str = "1.8.9";

#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    pub versions: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub release_time: String,
    pub sha1: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VersionEntry {
    pub id: String,
    pub release_time: String,
}

/// 1.8.9 以降の正式版を新しい順に返す。releaseTime はすべて UTC の ISO 8601 のため、文字列で比べる。
pub fn release_versions(m: &VersionManifest) -> Result<Vec<VersionEntry>, CoreError> {
    let floor = m
        .versions
        .iter()
        .find(|v| v.id == OLDEST_SUPPORTED)
        .ok_or_else(|| CoreError::InvalidMetadata {
            message: format!("{OLDEST_SUPPORTED} is missing from the version manifest"),
        })?
        .release_time
        .clone();
    let mut out: Vec<VersionEntry> = m
        .versions
        .iter()
        .filter(|v| v.kind == "release" && v.release_time >= floor)
        .map(|v| VersionEntry {
            id: v.id.clone(),
            release_time: v.release_time.clone(),
        })
        .collect();
    out.sort_by(|a, b| b.release_time.cmp(&a.release_time));
    Ok(out)
}

pub fn version_json_path(paths: &AppPaths, id: &str) -> PathBuf {
    paths.versions_dir().join(id).join(format!("{id}.json"))
}

pub fn client_jar_path(paths: &AppPaths, id: &str) -> PathBuf {
    paths.versions_dir().join(id).join(format!("{id}.jar"))
}

pub fn asset_index_path(paths: &AppPaths, id: &str) -> PathBuf {
    paths
        .assets_dir()
        .join("indexes")
        .join(format!("{id}.json"))
}

fn load_optional<T: serde::de::DeserializeOwned>(
    path: PathBuf,
    what: &str,
) -> Result<Option<T>, CoreError> {
    match std::fs::read(&path) {
        Ok(bytes) => parse_json(&bytes, what).map(Some),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_err(&path, source)),
    }
}

/// 固定済みの version JSON を読む。ネットワークは使わない。
pub fn load_pinned_version(paths: &AppPaths, id: &str) -> Result<Option<VersionJson>, CoreError> {
    load_optional(version_json_path(paths, id), "version JSON")
}

pub fn load_pinned_asset_index(
    paths: &AppPaths,
    id: &str,
) -> Result<Option<AssetIndex>, CoreError> {
    load_optional(asset_index_path(paths, id), "asset index")
}

#[derive(Clone)]
pub struct MojangMeta {
    http: Http,
    paths: AppPaths,
    manifest_url: String,
}

impl MojangMeta {
    pub fn new(http: Http, paths: AppPaths, manifest_url: impl Into<String>) -> Self {
        Self {
            http,
            paths,
            manifest_url: manifest_url.into(),
        }
    }

    pub async fn manifest(&self) -> Result<VersionManifest, CoreError> {
        let body = fetch_cached(
            &self.http,
            &self.paths.meta_cache_dir(),
            "version_manifest_v2.json",
            &self.manifest_url,
        )
        .await?;
        parse_json(&body, "version manifest")
    }

    /// manifest の sha1 で検証した version JSON を shared/versions に保存して返す。
    pub async fn pin_version(
        &self,
        id: &str,
        token: &CancellationToken,
    ) -> Result<VersionJson, CoreError> {
        let manifest = self.manifest().await?;
        let entry = manifest
            .versions
            .iter()
            .find(|v| v.id == id)
            .ok_or_else(|| CoreError::VersionNotFound { id: id.to_string() })?;
        let path = version_json_path(&self.paths, id);
        let item = DownloadItem {
            url: entry.url.clone(),
            path: path.clone(),
            size: None,
            sha1: Some(entry.sha1.clone()),
            executable: false,
        };
        fetch_all(&self.http, vec![item], Depth::Full, 1, token, |_, _| {}).await?;
        load_pinned_version(&self.paths, id)?.ok_or_else(|| io_err(&path, missing()))
    }

    pub async fn pin_asset_index(
        &self,
        version: &VersionJson,
        token: &CancellationToken,
    ) -> Result<AssetIndex, CoreError> {
        let r = &version.asset_index;
        let path = asset_index_path(&self.paths, &r.id);
        let item = DownloadItem {
            url: r.url.clone(),
            path: path.clone(),
            size: Some(r.size),
            sha1: Some(r.sha1.clone()),
            executable: false,
        };
        fetch_all(&self.http, vec![item], Depth::Full, 1, token, |_, _| {}).await?;
        load_pinned_asset_index(&self.paths, &r.id)?.ok_or_else(|| io_err(&path, missing()))
    }
}

fn missing() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, "file vanished after download")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, kind: &str, time: &str) -> ManifestEntry {
        ManifestEntry {
            id: id.into(),
            kind: kind.into(),
            url: format!("https://example.invalid/{id}.json"),
            release_time: time.into(),
            sha1: "0".repeat(40),
        }
    }

    #[test]
    fn release_versions_are_releases_since_1_8_9_newest_first() {
        let m = VersionManifest {
            versions: vec![
                entry("25w01a", "snapshot", "2025-01-01T00:00:00+00:00"),
                entry("1.21.5", "release", "2025-03-25T00:00:00+00:00"),
                entry("1.8.9", "release", "2015-12-03T09:24:39+00:00"),
                entry("1.8.8", "release", "2015-07-27T00:00:00+00:00"),
                entry("b1.7.3", "old_beta", "2011-07-08T00:00:00+00:00"),
                entry("1.12.2", "release", "2017-09-18T08:39:46+00:00"),
            ],
        };
        let ids: Vec<_> = release_versions(&m)
            .unwrap()
            .into_iter()
            .map(|v| v.id)
            .collect();
        assert_eq!(ids, vec!["1.21.5", "1.12.2", "1.8.9"]);
    }

    #[test]
    fn missing_floor_version_is_invalid_metadata() {
        let m = VersionManifest {
            versions: vec![entry("1.21.5", "release", "2025-03-25T00:00:00+00:00")],
        };
        assert_eq!(release_versions(&m).unwrap_err().code(), "meta.invalid");
    }
}
