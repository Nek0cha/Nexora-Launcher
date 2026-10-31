//! `.mrpack` のインポートとエクスポートを Launcher の操作として通す。

use std::path::PathBuf;

use super::*;
use crate::mrpack::archive;
use crate::mrpack::manifest::PackTarget;
use crate::mrpack::{Limits, MrpackInfo, loader_version_for};

impl Launcher {
    /// パックを読み、確認ダイアログに出す要約を返す。何も書き込まない。
    /// ローダーの版が導入処理の一覧に無ければ `MrpackUnsupported`。
    pub async fn inspect_mrpack(&self, path: PathBuf) -> Result<MrpackInfo, CoreError> {
        let (manifest, target) = blocking(move || {
            let mut pack = archive::open(&path, &Limits::DEFAULT)?;
            let manifest = pack.manifest()?;
            let target = manifest.target()?;
            Ok((manifest, target))
        })
        .await?;
        let loader_version = self.resolve_pack_loader(&target).await?;
        Ok(MrpackInfo {
            name: manifest.name.clone(),
            version_id: manifest.version_id.clone(),
            mc_version: target.mc_version,
            loader: target.loader,
            loader_version,
            file_count: u32::try_from(manifest.client_files().len()).unwrap_or(u32::MAX),
        })
    }

    /// パックのローダーを、導入処理が扱う版の表記にする。Vanilla は `None`。
    async fn resolve_pack_loader(&self, target: &PackTarget) -> Result<Option<String>, CoreError> {
        let Some(pack_version) = target.loader_version.as_deref() else {
            return Ok(None);
        };
        let entries = self
            .env
            .loader
            .loader_versions(target.loader, &target.mc_version)
            .await?;
        loader_version_for(target.loader, &target.mc_version, pack_version, &entries)
            .map(Some)
            .ok_or_else(|| CoreError::MrpackUnsupported {
                reason: format!(
                    "{} {} for Minecraft {} is not available",
                    target.loader.as_str(),
                    pack_version,
                    target.mc_version
                ),
            })
    }
}
