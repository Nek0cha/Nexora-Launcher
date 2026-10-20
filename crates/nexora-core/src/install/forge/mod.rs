//! Forge と NeoForge のインストーラを読み、導入に使う。

pub mod data;
pub mod installer;
pub mod profile;
pub mod run;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::CoreError;
use crate::install::plan::{is_safe_relative, maven_path};
use crate::install::{InstallEnv, kib, now_ms};
use crate::instance::Instance;
use crate::meta::loader::forge::{check_target, installer_url};
use crate::meta::loader::{LoaderKind, LoaderProfile, check_version, profile_path};
use crate::meta::mojang::manifest::client_jar_path;
use crate::meta::mojang::version::{Library, VersionJson};
use crate::meta::{parse_json, write_atomic};
use crate::net::{Depth, DownloadItem, fetch_all, io_err, lock_path};
use crate::paths::AppPaths;
use crate::settings;
use crate::task::TaskHandle;
use data::{DataContext, client_resources, plan_processors};
use installer::{Installer, extract_maven, extract_resource, fetch_installer, read_entry};
use profile::{InstallProfile, parse_install_profile};
use run::run_processors;

/// 完了記録のファイル名（`shared/versions/<id>/` の中）。
const RECORD_FILE: &str = "nexora-loader.json";
const INSTALL_PROFILE: &str = "install_profile.json";

fn invalid(message: String) -> CoreError {
    CoreError::InvalidMetadata { message }
}

/// `forge-<loader_version>` / `neoforge-<loader_version>`。`is_safe_version` を満たさなければ `InvalidMetadata`。
pub fn profile_dir_id(kind: LoaderKind, loader_version: &str) -> Result<String, CoreError> {
    let prefix = match kind {
        LoaderKind::Forge | LoaderKind::Neoforge => kind.as_str(),
        _ => return Err(invalid(format!("{} has no installer", kind.as_str()))),
    };
    check_version("loader version", loader_version)?;
    Ok(format!("{prefix}-{loader_version}"))
}

fn loader_version(inst: &Instance) -> Result<&str, CoreError> {
    inst.loader_version
        .as_deref()
        .ok_or_else(|| CoreError::InvalidInstanceSetting {
            field: "loaderVersion".to_string(),
        })
}

/// インスタンスのローダーのインストーラを取得する（取得済みで sha1 が合えばネットワークに出ない）。
async fn instance_installer(
    env: &InstallEnv,
    inst: &Instance,
    token: &CancellationToken,
) -> Result<Installer, CoreError> {
    let base = match inst.loader {
        LoaderKind::Neoforge => &env.urls.neoforge_maven,
        _ => &env.urls.forge_maven,
    };
    let url = installer_url(inst.loader, base, loader_version(inst)?)?;
    fetch_installer(&env.http, &env.paths, &url, token).await
}

/// インストーラの `install_profile.json` を読み、Minecraft のバージョンが合うことを確かめる。
fn read_install_profile(installer: &Path, mc_version: &str) -> Result<InstallProfile, CoreError> {
    let profile = parse_install_profile(&read_entry(installer, INSTALL_PROFILE)?)?;
    if profile.minecraft != mc_version {
        return Err(invalid(format!(
            "installer is for Minecraft {}, not {mc_version}",
            profile.minecraft
        )));
    }
    Ok(profile)
}

/// 固定済みの version.json を読む。無い、読めない、`inheritsFrom` が mc_version と違うときは `Ok(None)`。
/// NotFound 以外の読み出しの失敗だけを `Err` にする。
fn read_pinned(
    path: &Path,
    id: &str,
    mc_version: &str,
) -> Result<Option<LoaderProfile>, std::io::Error> {
    match std::fs::read(path) {
        Ok(bytes) => match parse_json::<LoaderProfile>(&bytes, "loader profile") {
            Ok(profile) if profile.inherits_from == mc_version => Ok(Some(profile)),
            Ok(_) => {
                tracing::warn!(id, "pinned loader profile is for another version");
                Ok(None)
            }
            Err(err) => {
                tracing::warn!(id, code = err.code(), "pinned loader profile is unreadable");
                Ok(None)
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// 起動用。固定済みの version.json だけを使い、ネットワークにもインストーラにも触れない。
/// 無い、読めない、`inheritsFrom` が `inst.mc_version` と違うときは、そのパスの `LoaderFilesMissing`（導入し直してもらう）。
pub fn load_pinned(env: &InstallEnv, inst: &Instance) -> Result<LoaderProfile, CoreError> {
    let id = profile_dir_id(inst.loader, loader_version(inst)?)?;
    let path = profile_path(&env.paths, &id);
    match read_pinned(&path, &id, &inst.mc_version) {
        Ok(Some(profile)) => return Ok(profile),
        Ok(None) => {}
        Err(err) => {
            // 原因は利用者に見せる文言には入らないため、ログに残す
            tracing::warn!(id, %err, "pinned loader profile could not be read");
        }
    }
    Err(CoreError::LoaderFilesMissing {
        path: path.display().to_string(),
    })
}

/// 対応範囲の外の Minecraft のバージョンや、別のバージョン向けのローダーの版は、何にも触れずに `UnsupportedProfile`。
/// 固定済みの version.json があり、`inheritsFrom` が `inst.mc_version` と一致するなら、ネットワークに出ずにそれを返す。
/// 無ければインストーラを取得し、`install_profile.minecraft` が `inst.mc_version` と一致することを確かめ
/// （違えば `InvalidMetadata`）、`version.json` を `shared/versions/<id>/<id>.json` に固定して返す。
pub async fn pin_profile(
    env: &InstallEnv,
    inst: &Instance,
    token: &CancellationToken,
) -> Result<LoaderProfile, CoreError> {
    let version = loader_version(inst)?;
    check_target(inst.loader, &inst.mc_version, version)?;
    let id = profile_dir_id(inst.loader, version)?;
    let path = profile_path(&env.paths, &id);
    match read_pinned(&path, &id, &inst.mc_version) {
        Ok(Some(profile)) => return Ok(profile),
        Ok(None) => {}
        Err(source) => return Err(io_err(&path, source)),
    }

    let installer = instance_installer(env, inst, token).await?;
    let install = read_install_profile(&installer.path, &inst.mc_version)?;
    let body = read_entry(&installer.path, &install.json)?;
    let profile: LoaderProfile = parse_json(&body, "loader profile")?;
    if profile.inherits_from != inst.mc_version {
        return Err(invalid(format!(
            "loader profile {id} inherits from {}, not {}",
            profile.inherits_from, inst.mc_version
        )));
    }
    write_atomic(&path, &body)?;
    Ok(profile)
}

/// 導入が済んだことの記録。一致の判定は `completed_at` 以外の 3 つ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompletionRecord {
    installer_sha1: String,
    minecraft_jar_sha1: String,
    java_major: u32,
    #[serde(default)]
    completed_at: i64,
}

impl CompletionRecord {
    fn matches(&self, other: &CompletionRecord) -> bool {
        self.installer_sha1
            .eq_ignore_ascii_case(&other.installer_sha1)
            && self
                .minecraft_jar_sha1
                .eq_ignore_ascii_case(&other.minecraft_jar_sha1)
            && self.java_major == other.java_major
    }
}

/// 記録が現在の入力と一致し、`generated` のファイルがすべてあるか。読めない記録は一致しないとみなす。
fn is_complete(record_path: &Path, expected: &CompletionRecord, generated: &[PathBuf]) -> bool {
    let recorded = std::fs::read(record_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<CompletionRecord>(&bytes).ok());
    recorded.is_some_and(|r| r.matches(expected)) && generated.iter().all(|p| p.is_file())
}

fn remove_if_exists(path: &Path) -> Result<(), CoreError> {
    match std::fs::remove_file(path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(io_err(path, err)),
        _ => Ok(()),
    }
}

/// インストール用のライブラリ（processor の jar など）の取得の一覧。
/// `downloads.artifact.url` が空のものは、インストーラの `maven/` から置くため取得しない。
fn install_library_items(
    libraries: &[Library],
    paths: &AppPaths,
) -> Result<Vec<DownloadItem>, CoreError> {
    let libraries_dir = paths.libraries_dir();
    let mut items = Vec::new();
    for lib in libraries {
        let artifact = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());
        let (url, rel, size, sha1) = match artifact {
            Some(a) if a.url.is_empty() => continue,
            Some(a) => {
                let rel = a.path.clone().or_else(|| maven_path(&lib.name));
                (a.url.clone(), rel, Some(a.size), Some(a.sha1.clone()))
            }
            None => {
                let Some(base) = lib.url.as_deref() else {
                    return Err(invalid(format!(
                        "install profile library {} has no download",
                        lib.name
                    )));
                };
                let rel = maven_path(&lib.name);
                let url = rel
                    .as_ref()
                    .map(|rel| format!("{}/{rel}", base.strip_suffix('/').unwrap_or(base)));
                (url.unwrap_or_default(), rel, lib.size, lib.sha1.clone())
            }
        };
        let rel = rel.filter(|rel| is_safe_relative(rel)).ok_or_else(|| {
            invalid(format!(
                "install profile library {} has a bad path",
                lib.name
            ))
        })?;
        let scheme_ok =
            reqwest::Url::parse(&url).is_ok_and(|u| matches!(u.scheme(), "http" | "https"));
        if !scheme_ok {
            return Err(invalid(format!(
                "install profile library {} has an unsupported url {url:?}",
                lib.name
            )));
        }
        items.push(DownloadItem {
            url,
            path: libraries_dir.join(rel),
            size,
            sha1: sha1.filter(|s| !s.is_empty()),
            executable: false,
        });
    }
    Ok(items)
}

/// 導入ごとの一時的な作業場所（`staging/forge-<一意>`）。落とすと中身ごと消す。
struct StagingDir(PathBuf);

impl StagingDir {
    fn create(paths: &AppPaths) -> Result<Self, CoreError> {
        let dir = paths
            .staging_dir()
            .join(format!("forge-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).map_err(|s| io_err(&dir, s))?;
        Ok(Self(dir))
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_dir_all(&self.0) {
            tracing::debug!(path = %self.0.display(), %err, "could not remove the staging directory");
        }
    }
}

/// 同期の処理を別のスレッドで動かす。
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, CoreError> + Send + 'static,
) -> Result<T, CoreError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| CoreError::Unexpected {
            message: e.to_string(),
        })?
}

/// ゲームのファイルと Java の用意が済んだ後に呼ぶ。完了記録が現在の入力と一致し、`generated` のファイルがすべてあれば何もしない。
/// そうでなければ、`profile_dir` の `lock_path` を取り、インストール用のライブラリを取得し、`maven/` と資源を展開し、
/// processor を実行し、`generated` を確かめ、最後に完了記録を書く。
pub async fn finish_install(
    env: &InstallEnv,
    inst: &Instance,
    version: &VersionJson,
    generated: &[PathBuf],
    java: &Path,
    handle: &TaskHandle,
) -> Result<(), CoreError> {
    let token = handle.token();
    let id = profile_dir_id(inst.loader, loader_version(inst)?)?;
    let profile_dir = env.paths.versions_dir().join(&id);
    let record_path = profile_dir.join(RECORD_FILE);
    let installer = instance_installer(env, inst, token).await?;
    let (_, java_major) = crate::java::requirement(version);
    let expected = CompletionRecord {
        installer_sha1: installer.sha1.clone(),
        minecraft_jar_sha1: version.downloads.client.sha1.clone(),
        java_major,
        completed_at: 0,
    };
    if is_complete(&record_path, &expected, generated) {
        return Ok(());
    }

    // 同じ版を導入するほかのタスクが終わるまで待ち、その結果で済んでいれば何もしない
    let _guard = tokio::select! {
        _ = token.cancelled() => return Err(CoreError::Cancelled),
        guard = lock_path(&profile_dir) => guard,
    };
    if is_complete(&record_path, &expected, generated) {
        return Ok(());
    }
    // 古い記録は、すべてが済むまで残さない
    remove_if_exists(&record_path)?;

    let profile = read_install_profile(&installer.path, &inst.mc_version)?;
    let items = install_library_items(&profile.libraries, &env.paths)?;
    let concurrency = usize::from(settings::load(&env.db)?.max_concurrent_downloads);
    fetch_all(
        &env.http,
        items,
        Depth::Full,
        concurrency,
        token,
        |done, total| handle.set_progress(kib(done), kib(total)),
    )
    .await?;
    if token.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    let libraries_dir = env.paths.libraries_dir();
    let installer_path = installer.path.clone();
    let libs = libraries_dir.clone();
    blocking(move || extract_maven(&installer_path, &libs).map(|_| ())).await?;

    // 資源は導入ごとの作業場所に展開し、終わったら（失敗や取り消しでも）消す
    let staging = StagingDir::create(&env.paths)?;
    let names = client_resources(&profile);
    let installer_path = installer.path.clone();
    let staging_path = staging.0.clone();
    let resources: BTreeMap<String, PathBuf> = blocking(move || {
        names
            .into_iter()
            .map(|name| {
                let path = extract_resource(&installer_path, &name, &staging_path)?;
                Ok((name, path))
            })
            .collect()
    })
    .await?;

    let root = env.paths.shared_dir();
    let minecraft_jar = client_jar_path(&env.paths, &inst.mc_version);
    let ctx = DataContext {
        libraries_dir: &libraries_dir,
        root: &root,
        installer: &installer.path,
        minecraft_jar: &minecraft_jar,
        minecraft_version: &inst.mc_version,
        resources: &resources,
    };
    let planned = plan_processors(&profile, &ctx)?;
    run_processors(java, &planned, token, |done, total| {
        handle.set_progress(
            u32::try_from(done).unwrap_or(u32::MAX),
            u32::try_from(total).unwrap_or(u32::MAX),
        );
    })
    .await?;
    drop(staging);

    if let Some(missing) = generated.iter().find(|p| !p.is_file()) {
        return Err(CoreError::LoaderFilesMissing {
            path: missing.display().to_string(),
        });
    }
    let record = CompletionRecord {
        completed_at: now_ms(),
        ..expected
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|e| CoreError::Unexpected {
        message: e.to_string(),
    })?;
    write_atomic(&record_path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_dir_ids() {
        assert_eq!(
            profile_dir_id(LoaderKind::Forge, "1.20.1-47.3.0").unwrap(),
            "forge-1.20.1-47.3.0"
        );
        assert_eq!(
            profile_dir_id(LoaderKind::Neoforge, "21.1.172").unwrap(),
            "neoforge-21.1.172"
        );
        // NeoForge の旧系統と Forge の同じ版は別の場所になる
        assert_eq!(
            profile_dir_id(LoaderKind::Neoforge, "1.20.1-47.1.106").unwrap(),
            "neoforge-1.20.1-47.1.106"
        );
    }

    #[test]
    fn profile_dir_id_rejects_unsafe_values_and_other_loaders() {
        for bad in ["", "..", "../x", "a/b", r"a\b", "a:b"] {
            let err = profile_dir_id(LoaderKind::Forge, bad).unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "{bad:?}");
        }
        let err = profile_dir_id(LoaderKind::Fabric, "0.16.10").unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
    }

    fn record(installer: &str, jar: &str, major: u32, at: i64) -> CompletionRecord {
        CompletionRecord {
            installer_sha1: installer.into(),
            minecraft_jar_sha1: jar.into(),
            java_major: major,
            completed_at: at,
        }
    }

    #[test]
    fn completion_record_format_and_match() {
        let r = record("aa", "bb", 17, 5);
        let json: serde_json::Value = serde_json::to_value(&r).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "installerSha1": "aa",
                "minecraftJarSha1": "bb",
                "javaMajor": 17,
                "completedAt": 5
            })
        );
        // 完了の時刻は比べない
        assert!(r.matches(&record("AA", "bb", 17, 0)));
        assert!(!r.matches(&record("ab", "bb", 17, 5)));
        assert!(!r.matches(&record("aa", "bc", 17, 5)));
        assert!(!r.matches(&record("aa", "bb", 21, 5)));
    }

    #[test]
    fn is_complete_needs_the_record_and_every_generated_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(RECORD_FILE);
        let expected = record("aa", "bb", 17, 0);
        let file = temp.path().join("client.jar");
        assert!(!is_complete(&path, &expected, &[]));
        std::fs::write(&path, b"{broken").unwrap();
        assert!(!is_complete(&path, &expected, &[]));
        std::fs::write(
            &path,
            serde_json::to_vec(&record("aa", "bb", 17, 9)).unwrap(),
        )
        .unwrap();
        assert!(is_complete(&path, &expected, &[]));
        assert!(!is_complete(&path, &expected, std::slice::from_ref(&file)));
        std::fs::write(&file, b"x").unwrap();
        assert!(is_complete(&path, &expected, &[file]));
    }

    fn lib(value: serde_json::Value) -> Library {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn install_libraries_skip_empty_urls() {
        let paths = AppPaths::from_root("/data");
        let items = install_library_items(
            &[
                lib(serde_json::json!({
                    "name": "net.minecraftforge:forge:1.16.5-36.2.39:universal",
                    "downloads": { "artifact": {
                        "path": "net/minecraftforge/forge/1.16.5-36.2.39/forge-1.16.5-36.2.39-universal.jar",
                        "url": "", "sha1": "00", "size": 1
                    }}
                })),
                lib(serde_json::json!({
                    "name": "net.minecraftforge:installertools:1.2.6",
                    "downloads": { "artifact": {
                        "path": "net/minecraftforge/installertools/1.2.6/installertools-1.2.6.jar",
                        "url": "https://maven.example/installertools-1.2.6.jar",
                        "sha1": "0123456789012345678901234567890123456789", "size": 2
                    }}
                })),
            ],
            &paths,
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].url,
            "https://maven.example/installertools-1.2.6.jar"
        );
        assert_eq!(
            items[0].path,
            paths
                .libraries_dir()
                .join("net/minecraftforge/installertools/1.2.6/installertools-1.2.6.jar")
        );
    }

    #[test]
    fn install_libraries_reject_unsafe_paths_and_urls() {
        let paths = AppPaths::from_root("/data");
        for value in [
            serde_json::json!({
                "name": "a:b:1",
                "downloads": { "artifact": {
                    "path": "../../evil.jar", "url": "https://maven.example/x.jar", "sha1": "", "size": 1
                }}
            }),
            serde_json::json!({
                "name": "a:b:1",
                "downloads": { "artifact": {
                    "path": "a/b/1/b-1.jar", "url": "file:///etc/passwd", "sha1": "", "size": 1
                }}
            }),
            serde_json::json!({ "name": "a:b:1" }),
        ] {
            let err = install_library_items(&[lib(value.clone())], &paths).unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "{value}");
        }
    }
}
