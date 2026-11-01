//! 他のランチャーからの取り込みを Launcher の操作として通す。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use super::*;
use crate::import::copy::copy_tree;
use crate::import::duplicates::find_duplicates;
use crate::import::modrinth_app::ModrinthApp;
use crate::import::{ImportKind, ImportRequest, ImportSource, ImportSourceKind, SourceInstance};
use crate::install::kib;
use crate::meta::loader::LoaderVersionEntry;
use crate::mrpack::loader_version_for;

/// 同じローダーと Minecraft の版は、一覧を 1 度だけ取りに行く。
type LoaderCache = HashMap<(&'static str, String), Vec<LoaderVersionEntry>>;

fn unavailable(reason: impl Into<String>) -> CoreError {
    CoreError::ImportUnavailable {
        reason: reason.into(),
    }
}

/// 読み込み元の種類とフォルダから、読み込み元を開く。`root` が無ければ既定の場所を探す。
fn open_source(
    kind: ImportSourceKind,
    root: Option<PathBuf>,
) -> Result<Arc<dyn ImportSource>, CoreError> {
    match kind {
        ImportSourceKind::ModrinthApp => {
            let root = root
                .or_else(ModrinthApp::locate)
                .ok_or_else(|| unavailable("Modrinth App was not found"))?;
            Ok(Arc::new(ModrinthApp::at(root)))
        }
    }
}

impl Launcher {
    /// 読み込み元のインスタンスの一覧。ローダーの版が導入処理に無い行は選べない印を付ける。
    pub async fn list_import_instances(
        &self,
        kind: ImportSourceKind,
        root: Option<PathBuf>,
    ) -> Result<Vec<SourceInstance>, CoreError> {
        self.list_source_instances(open_source(kind, root)?).await
    }

    pub async fn list_source_instances(
        &self,
        source: Arc<dyn ImportSource>,
    ) -> Result<Vec<SourceInstance>, CoreError> {
        let mut rows = blocking(move || source.list()).await?;
        let mut cache = LoaderCache::new();
        // 通信できなかった組み合わせ。同じ組み合わせで待ち直さない
        let mut unreachable: HashSet<(&'static str, String)> = HashSet::new();
        for row in rows.iter_mut().filter(|r| r.unsupported.is_none()) {
            let key = (row.loader.as_str(), row.mc_version.clone());
            if unreachable.contains(&key) {
                continue;
            }
            match self.resolve_import_loader(row, &mut cache).await {
                Err(CoreError::ImportUnavailable { reason }) => {
                    row.unsupported = Some(reason);
                    row.available.clear();
                }
                // 通信できないなどは、行を無効にせず、導入のときに失敗させる
                Err(_) => {
                    unreachable.insert(key);
                }
                Ok(_) => {}
            }
        }
        Ok(rows)
    }

    /// 選んだ行のうち、すでにあるインスタンスや先の行と重複するものの `id`。
    pub async fn find_import_duplicates(
        &self,
        kind: ImportSourceKind,
        root: Option<PathBuf>,
        ids: Vec<String>,
    ) -> Result<Vec<String>, CoreError> {
        self.find_source_duplicates(open_source(kind, root)?, ids)
            .await
    }

    pub async fn find_source_duplicates(
        &self,
        source: Arc<dyn ImportSource>,
        ids: Vec<String>,
    ) -> Result<Vec<String>, CoreError> {
        let rows = blocking(move || source.list()).await?;
        let picked = pick_rows(rows, &ids)?;
        let existing = instance::list(&self.env.db)?;
        Ok(find_duplicates(&existing, &picked))
    }

    /// 選んだインスタンスを作り、中身をコピーするタスクをインスタンスごとに始める。
    pub async fn import_from_launcher(
        &self,
        kind: ImportSourceKind,
        root: Option<PathBuf>,
        requests: Vec<ImportRequest>,
        allow_duplicates: bool,
    ) -> Result<Vec<CreatedInstance>, CoreError> {
        self.import_from_source(open_source(kind, root)?, requests, allow_duplicates)
            .await
    }

    /// 重複があって `allow_duplicates` が偽なら `ImportDuplicate`。選べない行や知らない `id` は
    /// `ImportUnavailable`。どれかで断る場合は、何も作らない。
    pub async fn import_from_source(
        &self,
        source: Arc<dyn ImportSource>,
        requests: Vec<ImportRequest>,
        allow_duplicates: bool,
    ) -> Result<Vec<CreatedInstance>, CoreError> {
        let listed = {
            let source = source.clone();
            blocking(move || source.list()).await?
        };
        let ids: Vec<String> = requests.iter().map(|r| r.id.clone()).collect();
        let picked = pick_rows(listed, &ids)?;
        for row in &picked {
            if let Some(reason) = &row.unsupported {
                return Err(unavailable(format!("{}: {reason}", row.name)));
            }
        }
        if !allow_duplicates && !find_duplicates(&instance::list(&self.env.db)?, &picked).is_empty()
        {
            return Err(CoreError::ImportDuplicate);
        }
        let mut cache = LoaderCache::new();
        let mut loader_versions = Vec::with_capacity(picked.len());
        for row in &picked {
            loader_versions.push(self.resolve_import_loader(row, &mut cache).await?);
        }

        // 全部の行を作ってからタスクを始める。途中で作れなければ、作った分を消して断る
        let mut made = Vec::with_capacity(picked.len());
        for (row, loader_version) in picked.iter().zip(loader_versions) {
            let new = NewInstance {
                name: row.name.trim().to_string(),
                mc_version: row.mc_version.clone(),
                loader: row.loader,
                loader_version,
                ..NewInstance::vanilla("", "")
            };
            match instance::create(&self.env.db, &self.env.paths, &new, now_ms()) {
                Ok(created) => made.push(created),
                Err(err) => {
                    for created in &made {
                        self.discard_import(&created.id).await;
                    }
                    return Err(err);
                }
            }
        }

        Ok(made
            .into_iter()
            .zip(requests)
            .map(|(created, request)| {
                let this = self.clone();
                let source = source.clone();
                let id = created.id.clone();
                let task_id =
                    self.tasks
                        .spawn(IMPORT_KIND, Some(id.clone()), move |handle| async move {
                            let _guard = this.locks.lock(&id).await;
                            let result = this
                                .run_launcher_import(&id, source, &request, &handle)
                                .await;
                            if result.is_err() {
                                this.discard_import(&id).await;
                            }
                            result
                        });
                CreatedInstance {
                    instance: created,
                    task_id,
                }
            })
            .collect())
    }

    /// ローダーを導入処理の表記にする。Vanilla は `None`。版が導入処理に無ければ `ImportUnavailable`。
    async fn resolve_import_loader(
        &self,
        row: &SourceInstance,
        cache: &mut LoaderCache,
    ) -> Result<Option<String>, CoreError> {
        if row.loader == LoaderKind::Vanilla {
            return Ok(None);
        }
        let Some(version) = row.loader_version.as_deref() else {
            return Err(unavailable("the loader version is missing"));
        };
        let key = (row.loader.as_str(), row.mc_version.clone());
        if !cache.contains_key(&key) {
            let entries = self
                .env
                .loader
                .loader_versions(row.loader, &row.mc_version)
                .await?;
            cache.insert(key.clone(), entries);
        }
        loader_version_for(row.loader, &row.mc_version, version, &cache[&key])
            .map(Some)
            .ok_or_else(|| {
                unavailable(format!(
                    "{} {version} for Minecraft {} is not available",
                    row.loader.as_str(),
                    row.mc_version
                ))
            })
    }

    async fn run_launcher_import(
        &self,
        id: &str,
        source: Arc<dyn ImportSource>,
        request: &ImportRequest,
        handle: &TaskHandle,
    ) -> Result<(), CoreError> {
        install(&self.env, id, handle).await?;
        // install は最後に ready にするが、中身はまだ入っていない。途中で落ちても使える状態に見えないよう戻しておく
        instance::set_state(&self.env.db, id, InstanceState::Installing)?;
        let inst = instance::get(&self.env.db, id)?;
        let game_dir = instance::game_dir(&self.env.paths, &inst);

        handle.set_phase(TaskPhase::Downloading);
        for kind in ImportKind::ALL
            .into_iter()
            .filter(|k| request.kinds.contains(k))
        {
            let from = {
                let (source, source_id) = (source.clone(), request.id.clone());
                blocking(move || source.folder(&source_id, kind)).await?
            };
            let Some(from) = from else { continue };
            let (root, token, progress) =
                (game_dir.clone(), handle.token().clone(), handle.clone());
            let dest = game_dir.join(kind.dir_name());
            blocking(move || {
                let report = copy_tree(&from, &root, &dest, &token, &mut |done, total| {
                    progress.set_progress(kib(done), kib(total))
                })?;
                if report.skipped_links > 0 {
                    tracing::warn!(
                        skipped_links = report.skipped_links,
                        "skipped links while importing from another launcher"
                    );
                }
                Ok(())
            })
            .await?;
        }

        handle.set_phase(TaskPhase::Finishing);
        // 入れたファイルの記録。失敗しても取り込みは成功のまま（次にコンテンツタブを開いたときに再び試す）
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
        instance::set_state(&self.env.db, id, InstanceState::Ready)?;
        Ok(())
    }
}

/// `ids` の順に行を取り出す。一覧に無い `id` は `ImportUnavailable`。
fn pick_rows(rows: Vec<SourceInstance>, ids: &[String]) -> Result<Vec<SourceInstance>, CoreError> {
    ids.iter()
        .map(|id| {
            rows.iter()
                .find(|r| &r.id == id)
                .cloned()
                .ok_or_else(|| unavailable(format!("{id} was not found")))
        })
        .collect()
}
