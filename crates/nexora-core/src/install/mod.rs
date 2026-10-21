//! ゲームのインストール。

pub mod forge;
pub mod plan;

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::db::Db;
use crate::error::CoreError;
use crate::instance::{self, Instance, InstanceState};
use crate::java::verify::JavaProbe;
use crate::meta::loader::merge::merge_profile;
use crate::meta::loader::{FABRIC_META_URL, LoaderKind, LoaderMeta, QUILT_META_URL};
use crate::meta::mojang::manifest::{MANIFEST_URL, MojangMeta};
use crate::meta::mojang::rules::RuleEnv;
use crate::meta::mojang::version::VersionJson;
use crate::net::{Depth, DownloadItem, FetchPhase, Http, fetch_all_with};
use crate::paths::AppPaths;
use crate::settings;
use crate::task::{TaskHandle, TaskPhase};

#[derive(Debug, Clone)]
pub struct Urls {
    pub manifest: String,
    pub runtime_index: String,
    pub resources: String,
    pub fabric_meta: String,
    pub quilt_meta: String,
    pub forge_maven: String,
    pub forge_promotions: String,
    pub neoforge_maven: String,
    pub modrinth_api: String,
    /// Modrinth のファイルを取ってよいオリジン。
    pub modrinth_cdn: String,
}

impl Default for Urls {
    fn default() -> Self {
        Self {
            manifest: MANIFEST_URL.to_string(),
            runtime_index: crate::java::RUNTIME_INDEX_URL.to_string(),
            resources: plan::RESOURCES_URL.to_string(),
            fabric_meta: FABRIC_META_URL.to_string(),
            quilt_meta: QUILT_META_URL.to_string(),
            forge_maven: crate::meta::loader::forge::FORGE_MAVEN_URL.to_string(),
            forge_promotions: crate::meta::loader::forge::FORGE_PROMOTIONS_URL.to_string(),
            neoforge_maven: crate::meta::loader::forge::NEOFORGE_MAVEN_URL.to_string(),
            modrinth_api: crate::content::modrinth::MODRINTH_API_URL.to_string(),
            modrinth_cdn: crate::content::apply::MODRINTH_CDN.to_string(),
        }
    }
}

#[derive(Clone)]
pub struct InstallEnv {
    pub paths: AppPaths,
    pub db: Db,
    pub http: Http,
    pub meta: MojangMeta,
    pub loader: LoaderMeta,
    pub urls: Urls,
    pub probe: Arc<dyn JavaProbe>,
}

impl InstallEnv {
    pub fn new(paths: AppPaths, db: Db, http: Http, urls: Urls, probe: Arc<dyn JavaProbe>) -> Self {
        let meta = MojangMeta::new(http.clone(), paths.clone(), urls.manifest.clone());
        let loader = LoaderMeta::new(
            http.clone(),
            paths.clone(),
            urls.fabric_meta.clone(),
            urls.quilt_meta.clone(),
        )
        .with_forge_urls(
            urls.forge_maven.clone(),
            urls.forge_promotions.clone(),
            urls.neoforge_maven.clone(),
        );
        Self {
            paths,
            db,
            http,
            meta,
            loader,
            urls,
            probe,
        }
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 進捗はバイトを KiB に切り上げて渡す（TaskProgress は u32 のため）。
pub fn kib(bytes: u64) -> u32 {
    bytes.div_ceil(1024).min(u64::from(u32::MAX)) as u32
}

fn clamp_u32(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// 足りないファイルを取得し、確認（ファイルの数）と取得（KiB）の段階と進捗をタスクに伝える。
pub(crate) async fn fetch_with_phases(
    http: &Http,
    items: Vec<DownloadItem>,
    depth: Depth,
    concurrency: usize,
    handle: &TaskHandle,
) -> Result<(), CoreError> {
    fetch_all_with(
        http,
        items,
        depth,
        concurrency,
        handle.token(),
        |phase| match phase {
            FetchPhase::Verifying { checked, total } => {
                handle.set_phase(TaskPhase::Verifying);
                handle.set_progress(clamp_u32(checked), clamp_u32(total));
            }
            FetchPhase::Downloading => handle.set_phase(TaskPhase::Downloading),
        },
        |done, total| handle.set_progress(kib(done), kib(total)),
    )
    .await
}

/// 取り消されたら、終わるのを待たずに `Cancelled` にする（取り消しを見ない通信や子プロセスの待ちに使う）。
pub(crate) async fn cancellable<T>(
    token: &CancellationToken,
    fut: impl std::future::Future<Output = Result<T, CoreError>>,
) -> Result<T, CoreError> {
    tokio::select! {
        _ = token.cancelled() => Err(CoreError::Cancelled),
        r = fut => r,
    }
}

/// インスタンスをインストールする。結果に応じてインスタンスの state を更新する。
pub async fn install(
    env: &InstallEnv,
    instance_id: &str,
    handle: &TaskHandle,
) -> Result<(), CoreError> {
    let result = install_inner(env, instance_id, handle).await;
    let state = match &result {
        Ok(()) => InstanceState::Ready,
        Err(CoreError::Cancelled) => InstanceState::Installing,
        Err(_) => InstanceState::Broken,
    };
    if let Err(err) = instance::set_state(&env.db, instance_id, state) {
        tracing::warn!(
            instance_id,
            code = err.code(),
            "failed to record install state"
        );
    }
    result
}

async fn install_inner(
    env: &InstallEnv,
    instance_id: &str,
    handle: &TaskHandle,
) -> Result<(), CoreError> {
    let inst = instance::get(&env.db, instance_id)?;
    instance::set_state(&env.db, instance_id, InstanceState::Installing)?;
    let token = handle.token();
    handle.set_phase(TaskPhase::Resolving);
    let vanilla = env.meta.pin_version(&inst.mc_version, token).await?;
    let version = overlay_loader(env, &inst, vanilla, false, token).await?;
    plan::check_profile(&version)?;
    let index = env.meta.pin_asset_index(&version, token).await?;
    let game = plan::plan_game(
        &version,
        &index,
        &RuleEnv::current(),
        &env.paths,
        &env.urls.resources,
    )?;
    let (component, major) = crate::java::requirement(&version);
    let runtime = cancellable(
        token,
        crate::java::prepare_runtime(
            &env.http,
            &env.paths,
            &env.urls.runtime_index,
            &component,
            major,
        ),
    )
    .await?;

    let mut items = game.downloads;
    items.extend(runtime.downloads.iter().cloned());
    let concurrency = usize::from(settings::load(&env.db)?.max_concurrent_downloads);
    fetch_with_phases(&env.http, items, Depth::Full, concurrency, handle).await?;
    // Forge と NeoForge は、この後にローダーの導入（情報の用意から）が続くため、Java の確認もその準備として見せる
    handle.set_phase(if is_forge_like(inst.loader) {
        TaskPhase::Resolving
    } else {
        TaskPhase::Finishing
    });
    // processor の Java は、ゲームと同じ runtime（手動の Java の指定は導入では使わない）
    let java = cancellable(
        token,
        crate::java::finish_runtime(&env.db, &runtime, env.probe.as_ref(), now_ms()),
    )
    .await?;
    if is_forge_like(inst.loader) {
        forge::finish_install(env, &inst, &version, &game.generated, &java, handle).await?;
    }
    Ok(())
}

/// インストーラで導入するローダー（Forge と NeoForge）か。
pub(crate) fn is_forge_like(kind: LoaderKind) -> bool {
    matches!(kind, LoaderKind::Forge | LoaderKind::Neoforge)
}

/// Vanilla の VersionJson に、インスタンスのローダーの差分を重ねた解決済みの VersionJson を返す。
/// 固定済みのプロファイルがあればネットワークを使わず、無ければ取得して固定する。
/// `pinned_only`（起動で使う）が真なら、Forge と NeoForge はインストーラを取得せず、
/// 固定済みのプロファイルが使えなければ `LoaderFilesMissing` にする。Fabric と Quilt は変わらない。
pub(crate) async fn overlay_loader(
    env: &InstallEnv,
    inst: &Instance,
    vanilla: VersionJson,
    pinned_only: bool,
    token: &CancellationToken,
) -> Result<VersionJson, CoreError> {
    if inst.loader == LoaderKind::Vanilla {
        return Ok(vanilla);
    }
    if is_forge_like(inst.loader) {
        let profile = if pinned_only {
            forge::load_pinned(env, inst)?
        } else {
            forge::pin_profile(env, inst, token).await?
        };
        return merge_profile(&vanilla, &profile);
    }
    let loader_version =
        inst.loader_version
            .as_deref()
            .ok_or_else(|| CoreError::InvalidInstanceSetting {
                field: "loaderVersion".to_string(),
            })?;
    let profile = tokio::select! {
        _ = token.cancelled() => return Err(CoreError::Cancelled),
        profile = env.loader.pin_profile(inst.loader, &inst.mc_version, loader_version) => profile?,
    };
    merge_profile(&vanilla, &profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kib_rounds_up_and_saturates() {
        assert_eq!(kib(0), 0);
        assert_eq!(kib(1), 1);
        assert_eq!(kib(1024), 1);
        assert_eq!(kib(1025), 2);
        assert_eq!(kib(u64::MAX), u32::MAX);
    }
}
