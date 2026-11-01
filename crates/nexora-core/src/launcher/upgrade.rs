//! インスタンスの Minecraft バージョンを上げる。ワールド、設定、MOD のファイルには触れない。

use super::*;
use crate::content::Provider;
use crate::content::modrinth::loader_slugs;
use crate::content::store::Row;
use crate::content::updates::{self, UpdateCheck};
use crate::import::copy::copy_tree;
use crate::install::kib;
use crate::instance::Instance;
use crate::meta::loader::latest_stable;
use crate::paths::AppPaths;
use tokio_util::sync::CancellationToken;

/// ワールドの退避のフォルダー名の前置き。`backups/<instance_id>/` の直下に置く。
const BACKUP_PREFIX: &str = "upgrade-";

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

/// `saves/` を `backups/<instance_id>/upgrade-<時刻>/saves` にコピーして、その `upgrade-<時刻>` を返す。
/// `saves/` が無ければ何もせず `None`。失敗かキャンセルなら、途中の退避を消す。
fn backup_saves(
    paths: &AppPaths,
    inst: &Instance,
    now_ms: i64,
    token: &CancellationToken,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Option<PathBuf>, CoreError> {
    let saves = instance::game_dir(paths, inst).join("saves");
    if !saves.is_dir() {
        return Ok(None);
    }
    let root = paths.backups_dir().join(&inst.id);
    let dir = root.join(format!("{BACKUP_PREFIX}{now_ms}"));
    std::fs::create_dir_all(&root).map_err(|source| CoreError::Io {
        path: root.display().to_string(),
        source,
    })?;
    match copy_tree(&saves, &root, &dir.join("saves"), token, progress) {
        Ok(_) => Ok(Some(dir)),
        Err(err) => {
            let _ = std::fs::remove_dir_all(&dir);
            Err(err)
        }
    }
}

/// `keep` 以外のアップグレードの退避を消す。MOD の退避など、前置きの違うものには触れない。
fn prune_old_backups(paths: &AppPaths, instance_id: &str, keep: &Path) {
    let root = paths.backups_dir().join(instance_id);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_backup = entry
            .file_name()
            .to_string_lossy()
            .starts_with(BACKUP_PREFIX);
        if is_backup && path != keep && path.is_dir() {
            if let Err(err) = std::fs::remove_dir_all(&path) {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to remove an old upgrade backup"
                );
            }
        }
    }
}

impl Launcher {
    /// インスタンスのバージョンを上げるタスクを始め、そのタスクの ID を返す。
    /// 同じインスタンスで別のタスクが動いているか、ゲームが起動中なら `InstanceBusy`。
    /// 上げ先が選べない版なら、タスクが `UpgradeUnavailable` で失敗する（何も変えない）。
    pub fn upgrade_instance(
        &self,
        instance_id: &str,
        target_mc_version: String,
    ) -> Result<String, CoreError> {
        self.ensure_idle(instance_id)?;
        self.ensure_no_pending_operation(instance_id)?;
        let this = self.clone();
        let id = instance_id.to_string();
        Ok(self
            .tasks
            .spawn_unique(UPGRADE_KIND, Some(id.clone()), move |handle| async move {
                let _guard = this.locks.lock(&id).await;
                // ロックを待つ間にゲームが起動したか、操作の記録が残った場合
                if this.processes.is_running(&id)? {
                    return Err(CoreError::InstanceBusy { instance_id: id });
                }
                this.ensure_no_pending_operation(&id)?;
                this.run_upgrade(&id, &target_mc_version, &handle).await
            }))
    }

    /// ワールドを退避し、行を上げ先にして導入する。失敗かキャンセルなら行を元に戻す。
    async fn run_upgrade(
        &self,
        id: &str,
        target: &str,
        handle: &TaskHandle,
    ) -> Result<(), CoreError> {
        let before = instance::get(&self.env.db, id)?;
        handle.set_phase(TaskPhase::Resolving);
        let loader_version = self.resolve_upgrade_target(&before, target).await?;

        handle.set_phase(TaskPhase::Downloading);
        let backup = {
            let (paths, inst) = (self.env.paths.clone(), before.clone());
            let (token, progress) = (handle.token().clone(), handle.clone());
            blocking(move || {
                backup_saves(&paths, &inst, now_ms(), &token, &mut |done, total| {
                    progress.set_progress(kib(done), kib(total))
                })
            })
            .await?
        };

        instance::set_version(&self.env.db, id, target, loader_version.as_deref())?;
        instance::set_state(&self.env.db, id, InstanceState::Installing)?;
        match install(&self.env, id, handle).await {
            Ok(()) => {
                self.updates.invalidate(id);
                if let Some(keep) = backup {
                    let (paths, owned) = (self.env.paths.clone(), id.to_string());
                    // 古い退避が消せなくても、上げるのは成功のまま
                    let _ = blocking(move || {
                        prune_old_backups(&paths, &owned, &keep);
                        Ok(())
                    })
                    .await;
                }
                Ok(())
            }
            Err(err) => {
                // install は失敗で broken、キャンセルで installing を書く。遊べる元の状態に戻す
                instance::set_version(
                    &self.env.db,
                    id,
                    &before.mc_version,
                    before.loader_version.as_deref(),
                )?;
                instance::set_state(&self.env.db, id, before.state)?;
                Err(err)
            }
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::NewInstance;

    fn setup() -> (tempfile::TempDir, AppPaths, Instance) {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("data"));
        paths.ensure_dirs().unwrap();
        let db = crate::db::Db::open_in_memory().unwrap();
        let inst = instance::create(&db, &paths, &NewInstance::vanilla("x", "1.20.1"), 1).unwrap();
        (temp, paths, inst)
    }

    #[test]
    fn a_cancelled_backup_leaves_nothing_behind() {
        let (_t, paths, inst) = setup();
        let saves = instance::game_dir(&paths, &inst).join("saves/world");
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(saves.join("level.dat"), b"world").unwrap();
        let token = CancellationToken::new();
        token.cancel();

        let err = backup_saves(&paths, &inst, 5, &token, &mut |_, _| {}).unwrap_err();

        assert!(matches!(err, CoreError::Cancelled), "{err:?}");
        assert!(
            !paths
                .backups_dir()
                .join(&inst.id)
                .join("upgrade-5")
                .exists()
        );
    }

    #[test]
    fn a_missing_saves_folder_means_no_backup() {
        let (_t, paths, inst) = setup();
        let token = CancellationToken::new();
        assert_eq!(
            backup_saves(&paths, &inst, 5, &token, &mut |_, _| {}).unwrap(),
            None
        );
    }
}
