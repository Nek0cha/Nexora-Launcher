//! Mojang 配布の Java の導入と登録。

pub mod platform;
pub mod runtime;
pub mod verify;

use std::path::PathBuf;

use rusqlite::{OptionalExtension, params};

use crate::db::Db;
use crate::error::CoreError;
use crate::meta::mojang::version::VersionJson;
use crate::meta::{fetch_cached, parse_json};
use crate::net::hash::sha1_hex;
use crate::net::{DownloadItem, Http, io_err};
use crate::paths::AppPaths;
use runtime::{
    RuntimeIndex, RuntimeManifest, java_relative_path, plan_manifest, runtime_dir, select_candidate,
};
use verify::{JavaProbe, arch_matches};

pub const RUNTIME_INDEX_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

/// version JSON が求める Java。javaVersion がなければ Java 8（jre-legacy）とする。
pub fn requirement(v: &VersionJson) -> (String, u32) {
    match &v.java_version {
        Some(j) => (j.component.clone(), j.major_version),
        None => ("jre-legacy".to_string(), 8),
    }
}

fn current_platform(component: &str) -> Result<&'static str, CoreError> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    platform::platform_key(os, arch).ok_or_else(|| CoreError::NoCompatibleRuntime {
        component: component.to_string(),
        platform: format!("{os}-{arch}"),
    })
}

#[derive(Debug, Clone)]
pub struct PreparedRuntime {
    pub component: String,
    pub major: u32,
    pub platform: String,
    pub dir: PathBuf,
    pub java: PathBuf,
    pub downloads: Vec<DownloadItem>,
    pub dirs: Vec<PathBuf>,
    pub links: Vec<(PathBuf, String)>,
}

/// 索引と manifest を取得し、取得するファイルの一覧を作る。ファイルの取得は呼び出し側が行う。
pub async fn prepare_runtime(
    http: &Http,
    paths: &AppPaths,
    index_url: &str,
    component: &str,
    major: u32,
) -> Result<PreparedRuntime, CoreError> {
    let platform = current_platform(component)?;
    let index_bytes = fetch_cached(
        http,
        &paths.meta_cache_dir(),
        "java-runtime-all.json",
        index_url,
    )
    .await?;
    let index: RuntimeIndex = parse_json(&index_bytes, "java runtime index")?;
    let candidate = select_candidate(&index, platform, component)?;
    let manifest_bytes = http.get_bytes(&candidate.manifest.url).await?;
    if !sha1_hex(&manifest_bytes).eq_ignore_ascii_case(&candidate.manifest.sha1) {
        return Err(CoreError::HashMismatch {
            path: candidate.manifest.url.clone(),
        });
    }
    let manifest: RuntimeManifest = parse_json(&manifest_bytes, "java runtime manifest")?;
    let dir = runtime_dir(paths, platform, component, &candidate.manifest.sha1);
    let (downloads, dirs, links) = plan_manifest(&manifest, &dir)?;
    let java = java_relative_path(std::env::consts::OS)
        .split('/')
        .fold(dir.clone(), |p, part| p.join(part));
    Ok(PreparedRuntime {
        component: component.to_string(),
        major,
        platform: platform.to_string(),
        dir,
        java,
        downloads,
        dirs,
        links,
    })
}

#[cfg(unix)]
fn make_link(path: &std::path::Path, target: &str) -> Result<(), CoreError> {
    if std::fs::symlink_metadata(path).is_ok() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|s| io_err(parent, s))?;
    }
    std::os::unix::fs::symlink(target, path).map_err(|s| io_err(path, s))
}

#[cfg(not(unix))]
fn make_link(_path: &std::path::Path, _target: &str) -> Result<(), CoreError> {
    // Windows 用の runtime にはリンクがない
    Ok(())
}

/// ファイルの取得後に、ディレクトリとリンクを作り、Java を実行して確かめ、DB に登録する。
pub async fn finish_runtime(
    db: &Db,
    prepared: &PreparedRuntime,
    probe: &dyn JavaProbe,
    now_ms: i64,
) -> Result<PathBuf, CoreError> {
    for dir in &prepared.dirs {
        std::fs::create_dir_all(dir).map_err(|s| io_err(dir, s))?;
    }
    for (path, target) in &prepared.links {
        make_link(path, target)?;
    }
    let info = probe.probe(&prepared.java).await?;
    let arch = std::env::consts::ARCH;
    if info.major != prepared.major || !arch_matches(&info.arch, arch) {
        return Err(CoreError::JavaVerifyFailed {
            expected_major: prepared.major,
            found: format!("{} {}", info.major, info.arch),
        });
    }
    let java = prepared.java.display().to_string();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO java_runtimes (path, major_version, component, os, arch, source, last_verified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'mojang', ?6)
             ON CONFLICT(path) DO UPDATE SET major_version = excluded.major_version,
                 last_verified_at = excluded.last_verified_at",
            params![java, info.major, prepared.component, std::env::consts::OS, arch, now_ms],
        )?;
        Ok(())
    })?;
    Ok(prepared.java.clone())
}

/// 今の OS とアーキテクチャで登録済みの、component の Java のうち、ディスクにある最新のもの。
pub fn find_registered(db: &Db, component: &str) -> Result<Option<PathBuf>, CoreError> {
    let path: Option<String> = db.with_conn(|conn| {
        Ok(conn
            .query_row(
                "SELECT path FROM java_runtimes
                 WHERE component = ?1 AND os = ?2 AND arch = ?3 AND source = 'mojang'
                 ORDER BY last_verified_at DESC LIMIT 1",
                params![component, std::env::consts::OS, std::env::consts::ARCH],
                |r| r.get(0),
            )
            .optional()?)
    })?;
    Ok(path.map(PathBuf::from).filter(|p| p.is_file()))
}
