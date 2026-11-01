//! インスタンスの Minecraft バージョンを上げる。ワールド、設定、MOD のファイルには触れない。

use super::*;
use crate::content::Provider;
use crate::content::modrinth::loader_slugs;
use crate::content::store::Row;
use crate::content::updates::{self, UpdateCheck};
use crate::instance::Instance;
use crate::meta::loader::latest_stable;

fn unavailable(reason: impl Into<String>) -> CoreError {
    CoreError::UpgradeUnavailable {
        reason: reason.into(),
    }
}

/// 上げる前の確認。MOD の三つの配列は `rel_path`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpgradePreview {
    /// 上げ先で使うローダーの版。Vanilla は `None`。
    pub loader_version: Option<String>,
    /// 上げ先に合う版がある MOD。
    pub compatible: Vec<String>,
    /// Modrinth の MOD だが、上げ先に合う版が見つからないもの。
    pub incompatible: Vec<String>,
    /// 対応を調べられない MOD（Modrinth で特定できない、版を固定している）。
    pub unknown: Vec<String>,
}

/// 確認の結果を三つに分ける。`check` は、版を固定した行とハッシュの無い行を対象にしない。
fn classify(rows: &[Row], check: &UpdateCheck) -> (Vec<String>, Vec<String>, Vec<String>) {
    let (mut compatible, mut incompatible, mut unknown) = (vec![], vec![], vec![]);
    for row in rows {
        let item = &row.item;
        let rel = item.rel_path.clone();
        let checked = !item.pinned && !item.sha1.is_empty();
        if check.no_compatible.contains(&rel) {
            incompatible.push(rel);
        } else if check.updates.iter().any(|u| u.rel_path == rel)
            || (checked && item.provider == Provider::Modrinth)
        {
            compatible.push(rel);
        } else {
            unknown.push(rel);
        }
    }
    (compatible, incompatible, unknown)
}

impl Launcher {
    /// 上げる前の確認。インスタンスは変更せず、確認の結果は使い回さない。
    /// 上げ先が選べない版なら `UpgradeUnavailable`。
    pub async fn preview_upgrade(
        &self,
        instance_id: &str,
        target_mc_version: &str,
    ) -> Result<UpgradePreview, CoreError> {
        let inst = instance::get(&self.env.db, instance_id)?;
        let loader_version = self
            .resolve_upgrade_target(&inst, target_mc_version)
            .await?;
        let rows = self.mod_rows(instance_id).await?;
        let loaders = loader_slugs(Some(inst.loader.as_str()));
        let check = updates::check(
            &self.env.http,
            &self.env.urls.modrinth_api,
            &rows,
            target_mc_version,
            &loaders,
        )
        .await?;
        let (compatible, incompatible, unknown) = classify(&rows, &check);
        Ok(UpgradePreview {
            loader_version,
            compatible,
            incompatible,
            unknown,
        })
    }

    /// 上げ先を確かめ、使うローダーの版を返す（Vanilla は `None`）。
    /// 上げ先は、いま入っているバージョンより新しい正式版で、ローダーが対応するものだけ。
    pub(super) async fn resolve_upgrade_target(
        &self,
        inst: &Instance,
        target: &str,
    ) -> Result<Option<String>, CoreError> {
        let releases = self.list_versions().await?;
        let position = |id: &str| releases.iter().position(|v| v.id == id);
        let (Some(current), Some(wanted)) = (position(&inst.mc_version), position(target)) else {
            return Err(unavailable(format!(
                "{} -> {target} is not a known release",
                inst.mc_version
            )));
        };
        // 新しい順に並んでいるので、新しいほど位置が小さい
        if wanted >= current {
            return Err(unavailable(format!(
                "{target} is not newer than {}",
                inst.mc_version
            )));
        }
        if inst.loader == LoaderKind::Vanilla {
            return Ok(None);
        }
        let supported = self.list_loader_game_versions(inst.loader).await?;
        if !supported.iter().any(|v| v == target) {
            return Err(unavailable(format!(
                "{} does not support Minecraft {target}",
                inst.loader.as_str()
            )));
        }
        let entries = self.list_loader_versions(inst.loader, target).await?;
        latest_stable(&entries)
            .or_else(|| entries.first())
            .map(|entry| Some(entry.version.clone()))
            .ok_or_else(|| {
                unavailable(format!(
                    "{} has no version for Minecraft {target}",
                    inst.loader.as_str()
                ))
            })
    }
}
