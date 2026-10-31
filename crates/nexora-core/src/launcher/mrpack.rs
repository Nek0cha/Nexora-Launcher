//! `.mrpack` のインポートとエクスポートを Launcher の操作として通す。

use std::path::{Path, PathBuf};

use super::*;
use crate::content::modrinth::{self, FileInfo, FullVersion};
use crate::install::kib;
use crate::mrpack::archive;
use crate::mrpack::export::{ExportEntry, loader_dependency_key, plan_entries, write_pack};
use crate::mrpack::fetch::{MAX_REDIRECTS, download_pack_files, staged_path};
use crate::mrpack::manifest::{Manifest, PackFile, PackTarget};
use crate::mrpack::safe_path::{PathGuard, ensure_contained};
use crate::mrpack::{
    ExportOptions, ExportPreview, Limits, MrpackInfo, loader_version_for, pack_version_for,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// ダウンロードしたファイルを、置き場所へ移す前に置いておく場所（インスタンスのフォルダーの中）。
const STAGING_DIR: &str = ".mrpack-staging";

impl Launcher {
    /// パックを読み、確認ダイアログに出す要約を返す。何も書き込まない。
    /// ローダーの版が導入処理の一覧に無ければ `MrpackUnsupported`。
    pub async fn inspect_mrpack(&self, path: PathBuf) -> Result<MrpackInfo, CoreError> {
        let (manifest, target, loader_version) = self.read_pack(path).await?;
        Ok(MrpackInfo {
            name: manifest.name.clone(),
            version_id: manifest.version_id.clone(),
            mc_version: target.mc_version,
            loader: target.loader,
            loader_version,
            file_count: u32::try_from(manifest.client_files().len()).unwrap_or(u32::MAX),
        })
    }

    /// 索引を読み、ローダーを導入処理の表記にする。
    async fn read_pack(
        &self,
        path: PathBuf,
    ) -> Result<(Manifest, PackTarget, Option<String>), CoreError> {
        let (manifest, target) = blocking(move || {
            let mut pack = archive::open(&path, &Limits::DEFAULT)?;
            let manifest = pack.manifest()?;
            let target = manifest.target()?;
            Ok((manifest, target))
        })
        .await?;
        let loader_version = self.resolve_pack_loader(&target).await?;
        Ok((manifest, target, loader_version))
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

    /// パックから新しいインスタンスを作り、中身を入れるタスクを始める。
    /// `name` が空白だけならパックの名前を使う。索引のパスが安全でなければ、何も作らずに断る。
    /// 途中の失敗とキャンセルでは、作ったインスタンスを消す。
    pub async fn import_mrpack(
        &self,
        path: PathBuf,
        name: String,
    ) -> Result<CreatedInstance, CoreError> {
        let (manifest, target, loader_version) = self.read_pack(path.clone()).await?;
        let mut guard = PathGuard::new();
        for file in manifest.client_files() {
            guard.admit(&file.path)?;
        }
        let name = match name.trim() {
            "" => manifest.name.clone(),
            trimmed => trimmed.to_string(),
        };
        let new = NewInstance {
            name,
            mc_version: target.mc_version,
            loader: target.loader,
            loader_version,
            ..NewInstance::vanilla("", "")
        };
        let instance = instance::create(&self.env.db, &self.env.paths, &new, now_ms())?;
        let this = self.clone();
        let id = instance.id.clone();
        let task_id = self
            .tasks
            .spawn(MRPACK_KIND, Some(id.clone()), move |handle| async move {
                let _guard = this.locks.lock(&id).await;
                let result = this.run_mrpack_import(&id, &path, &handle).await;
                if result.is_err() {
                    this.discard_import(&id).await;
                }
                result
            });
        Ok(CreatedInstance { instance, task_id })
    }

    async fn run_mrpack_import(
        &self,
        id: &str,
        path: &Path,
        handle: &TaskHandle,
    ) -> Result<(), CoreError> {
        install(&self.env, id, handle).await?;
        let inst = instance::get(&self.env.db, id)?;
        let game_dir = instance::game_dir(&self.env.paths, &inst);
        let manifest = {
            let path = path.to_path_buf();
            blocking(move || archive::open(&path, &Limits::DEFAULT)?.manifest()).await?
        };
        let files: Vec<PackFile> = manifest.client_files().into_iter().cloned().collect();
        let staging = game_dir.join(STAGING_DIR);

        handle.set_phase(TaskPhase::Downloading);
        let http = Http::restricted(&self.pack_policy, MAX_REDIRECTS)?
            .with_retry_base(self.env.http.retry_base());
        let refs: Vec<&PackFile> = files.iter().collect();
        let progress = handle.clone();
        download_pack_files(
            &http,
            &self.pack_policy,
            &refs,
            &staging,
            Limits::DEFAULT.file_bytes,
            handle.token(),
            move |done, total| progress.set_progress(kib(done), kib(total)),
        )
        .await?;

        handle.set_phase(TaskPhase::Finishing);
        let (path, token) = (path.to_path_buf(), handle.token().clone());
        blocking(move || place_pack(&path, &files, &staging, &game_dir, &token)).await?;

        // 入れたファイルの記録。失敗してもインポートは成功のまま（次にコンテンツタブを開いたときに再び試す）
        for kind in [
            ContentKind::Mod,
            ContentKind::ResourcePack,
            ContentKind::Shader,
        ] {
            if let Err(err) = self.list_content(id, kind).await {
                tracing::warn!(
                    instance_id = id,
                    code = err.code(),
                    "failed to scan the imported content"
                );
            }
        }
        if let Err(err) = self.identify_content(id).await {
            tracing::warn!(
                instance_id = id,
                code = err.code(),
                "failed to identify the imported content"
            );
        }
        Ok(())
    }

    /// 書き出す前の内訳。Modrinth に問い合わせるため、通信できなければ `Offline`。
    pub async fn preview_mrpack_export(
        &self,
        instance_id: &str,
    ) -> Result<ExportPreview, CoreError> {
        let gathered = self.gather_export(instance_id).await?;
        Ok(ExportPreview {
            referenced: u32::try_from(
                gathered
                    .entries
                    .iter()
                    .filter(|e| matches!(e, ExportEntry::Referenced { .. }))
                    .count(),
            )
            .unwrap_or(u32::MAX),
            unidentified: gathered.unidentified(),
            disabled: gathered.skips.disabled,
        })
    }

    /// インスタンスを `.mrpack` として `dest` に書く。起動中でも書ける（読むだけ）。
    /// 特定できないファイルがあるのに `accept_unidentified` が偽なら、何も書かずに断る。
    pub async fn export_mrpack(
        &self,
        instance_id: &str,
        dest: PathBuf,
        options: ExportOptions,
    ) -> Result<(), CoreError> {
        let version_id = options.version_id.trim().to_string();
        if version_id.is_empty() {
            return Err(CoreError::MrpackInvalid {
                reason: "the version is empty".into(),
            });
        }
        let gathered = self.gather_export(instance_id).await?;
        if !options.accept_unidentified && !gathered.unidentified().is_empty() {
            return Err(CoreError::MrpackNeedsConfirmation);
        }
        let inst = &gathered.instance;
        let mut dependencies = BTreeMap::new();
        dependencies.insert("minecraft".to_string(), inst.mc_version.clone());
        if let (Some(key), Some(version)) = (
            loader_dependency_key(inst.loader),
            inst.loader_version.as_deref(),
        ) {
            dependencies.insert(
                key.to_string(),
                pack_version_for(inst.loader, &inst.mc_version, version),
            );
        }
        let config_dir = options
            .include_config
            .then(|| gathered.game_dir.join("config"))
            .filter(|dir| dir.is_dir());
        let name = inst.name.clone();
        let entries = gathered.entries;
        blocking(move || {
            write_pack(
                &dest,
                &name,
                &version_id,
                dependencies,
                &entries,
                config_dir.as_deref(),
            )
        })
        .await
    }

    /// 書き出す中身を集める。有効な全ファイルを、その場で Modrinth に引き直す。
    async fn gather_export(&self, instance_id: &str) -> Result<Gathered, CoreError> {
        let instance = instance::get(&self.env.db, instance_id)?;
        let game_dir = instance::game_dir(&self.env.paths, &instance);
        let mut items = Vec::new();
        for kind in [
            ContentKind::Mod,
            ContentKind::ResourcePack,
            ContentKind::Shader,
        ] {
            items.extend(self.list_content(instance_id, kind).await?);
        }
        let sha1s: Vec<String> = items
            .iter()
            .filter(|i| i.enabled)
            .map(|i| i.sha1.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let found = self.lookup_files(&sha1s).await?;
        let (entries, skips) = plan_entries(&items, &found, &game_dir);
        Ok(Gathered {
            instance,
            game_dir,
            entries,
            skips,
        })
    }

    /// SHA-1 から、Modrinth のファイル情報（URL、ハッシュ、大きさ）を引く。分からないものは含まれない。
    async fn lookup_files(&self, sha1s: &[String]) -> Result<HashMap<String, FileInfo>, CoreError> {
        let (http, api) = (&self.env.http, self.env.urls.modrinth_api.as_str());
        let infos = modrinth::versions_by_hash(http, api, sha1s).await?;
        let ids: Vec<String> = infos
            .values()
            .map(|v| v.version_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut versions: HashMap<String, FullVersion> = HashMap::new();
        for chunk in ids.chunks(100) {
            for version in modrinth::versions_by_ids(http, api, chunk).await? {
                versions.insert(version.id.clone(), version);
            }
        }
        let mut out = HashMap::new();
        for (sha1, info) in infos {
            let file = versions
                .get(&info.version_id)
                .and_then(|v| v.files.iter().find(|f| f.sha1.eq_ignore_ascii_case(&sha1)));
            if let Some(file) = file {
                out.insert(sha1, file.clone());
            }
        }
        Ok(out)
    }

    /// 失敗したインポートのインスタンスを、行もフォルダーも消す。共有のキャッシュは消さない。
    async fn discard_import(&self, id: &str) {
        let (db, paths, owned) = (self.env.db.clone(), self.env.paths.clone(), id.to_string());
        if let Err(err) = blocking(move || instance::delete(&db, &paths, &owned)).await {
            tracing::warn!(
                instance_id = id,
                code = err.code(),
                "failed to remove the failed import"
            );
        }
    }
}

/// 書き出す中身と、その元になったインスタンス。
struct Gathered {
    instance: Instance,
    game_dir: PathBuf,
    entries: Vec<ExportEntry>,
    skips: crate::mrpack::export::ExportSkips,
}

impl Gathered {
    /// 実ファイルとして入れるファイルの名前。
    fn unidentified(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|e| match e {
                ExportEntry::Embedded { zip_path, .. } => {
                    Some(zip_path.rsplit('/').next().unwrap_or(zip_path).to_string())
                }
                ExportEntry::Referenced { .. } => None,
            })
            .collect()
    }
}

/// ステージングのファイルを置き場所へ移し、続けて `overrides/`、`client-overrides/` を展開する。
/// ステージングは最後に消す。
fn place_pack(
    pack: &Path,
    files: &[PackFile],
    staging: &Path,
    game_dir: &Path,
    token: &CancellationToken,
) -> Result<(), CoreError> {
    let mut guard = PathGuard::new();
    for (index, file) in files.iter().enumerate() {
        let dest = game_dir.join(guard.admit(&file.path)?);
        ensure_contained(game_dir, &dest)?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| crate::net::io_err(parent, e))?;
        }
        move_file(&staged_path(staging, index), &dest)?;
    }
    archive::open(pack, &Limits::DEFAULT)?.extract_overrides(game_dir, token)?;
    match std::fs::remove_dir_all(staging) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(crate::net::io_err(staging, err)),
    }
}

/// 同じボリュームなら名前の変更で、できなければコピーして元を消す。
fn move_file(from: &Path, to: &Path) -> Result<(), CoreError> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).map_err(|e| crate::net::io_err(to, e))?;
    std::fs::remove_file(from).map_err(|e| crate::net::io_err(from, e))
}
