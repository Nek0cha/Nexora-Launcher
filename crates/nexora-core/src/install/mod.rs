//! ゲームのインストール。

pub mod plan;

use std::sync::Arc;

use crate::db::Db;
use crate::error::CoreError;
use crate::instance::{self, InstanceState};
use crate::java::verify::JavaProbe;
use crate::meta::loader::{FABRIC_META_URL, LoaderMeta, QUILT_META_URL};
use crate::meta::mojang::manifest::{MANIFEST_URL, MojangMeta};
use crate::meta::mojang::rules::RuleEnv;
use crate::net::{Depth, Http, fetch_all};
use crate::paths::AppPaths;
use crate::settings;
use crate::task::TaskHandle;

#[derive(Debug, Clone)]
pub struct Urls {
    pub manifest: String,
    pub runtime_index: String,
    pub resources: String,
    pub fabric_meta: String,
    pub quilt_meta: String,
}

impl Default for Urls {
    fn default() -> Self {
        Self {
            manifest: MANIFEST_URL.to_string(),
            runtime_index: crate::java::RUNTIME_INDEX_URL.to_string(),
            resources: plan::RESOURCES_URL.to_string(),
            fabric_meta: FABRIC_META_URL.to_string(),
            quilt_meta: QUILT_META_URL.to_string(),
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
    let version = env.meta.pin_version(&inst.mc_version, token).await?;
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
    let runtime = crate::java::prepare_runtime(
        &env.http,
        &env.paths,
        &env.urls.runtime_index,
        &component,
        major,
    )
    .await?;

    let mut items = game.downloads;
    items.extend(runtime.downloads.iter().cloned());
    let concurrency = usize::from(settings::load(&env.db)?.max_concurrent_downloads);
    fetch_all(
        &env.http,
        items,
        Depth::Full,
        concurrency,
        token,
        |done, total| {
            handle.set_progress(kib(done), kib(total));
        },
    )
    .await?;
    crate::java::finish_runtime(&env.db, &runtime, env.probe.as_ref(), now_ms()).await?;
    Ok(())
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
