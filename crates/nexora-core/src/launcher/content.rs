//! Modrinth からの MOD の検索、導入、更新。導入と更新は `CONTENT_KIND` のタスクとして動かす。

use std::collections::{HashMap, VecDeque};

use super::*;
use crate::content::apply::{
    ApplyContext, ApplyOutcome, ContentTaskReport, ItemFailure, apply_plan,
};
use crate::content::modrinth::{
    self, FullVersion, SearchPage, SearchQuery, loader_slugs, updates_by_hash,
};
use crate::content::resolve::{ApiSource, InstallPlan, Installed, resolve};
use crate::content::store::Row;
use crate::content::updates::{self, UpdateCheck};
use crate::error::DependencyCause;

/// 1 回の検索で返す件数。
const SEARCH_LIMIT: u32 = 20;
/// 結果を覚えておくタスクの数（新しいものから）。
const REPORT_KEEP: usize = 50;

/// 導入の前に見せる内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstallPreview {
    pub version_number: String,
    /// 一緒に入る依存の数。
    pub dependency_count: u32,
    /// 新しい版が非互換と宣言している導入済みの MOD の表示名。
    pub incompatible: Vec<String>,
    /// 同じ project の MOD が無効で入っていて、ダウンロードせずにそれを有効にするだけか。
    pub enables_installed: bool,
}

/// タスクの結果。古いものから捨てて、一定数だけ持つ。
#[derive(Default)]
pub(super) struct ReportStore {
    order: VecDeque<String>,
    reports: HashMap<String, ContentTaskReport>,
}

impl ReportStore {
    fn insert(&mut self, task_id: &str, report: ContentTaskReport) {
        if self.reports.insert(task_id.to_string(), report).is_none() {
            self.order.push_back(task_id.to_string());
        }
        while self.order.len() > REPORT_KEEP {
            if let Some(old) = self.order.pop_front() {
                self.reports.remove(&old);
            }
        }
    }
}

/// インスタンスの値のうち、導入と更新に使うもの。
struct Target {
    inst: Instance,
    game_dir: PathBuf,
    loaders: Vec<String>,
}

fn display_name(row: &Row) -> String {
    row.item
        .name
        .clone()
        .unwrap_or_else(|| row.item.file_name.clone())
}

fn installed_of(rows: &[Row]) -> Vec<Installed> {
    rows.iter()
        .filter(|r| r.item.kind == ContentKind::Mod)
        .map(|r| Installed {
            rel_path: r.item.rel_path.clone(),
            enabled: r.item.enabled,
            project_id: r.item.project_id.clone(),
        })
        .collect()
}

/// 計画が非互換と宣言する導入済みの MOD の表示名。
fn incompatible_names(plan: &InstallPlan, rows: &[Row]) -> Vec<String> {
    plan.incompatible_installed
        .iter()
        .map(|rel| {
            rows.iter()
                .find(|r| &r.item.rel_path == rel)
                .map_or_else(|| rel.clone(), display_name)
        })
        .collect()
}

fn no_version(project_id: &str) -> CoreError {
    CoreError::ContentDependency {
        project_id: project_id.to_string(),
        cause: DependencyCause::NoVersion,
        name: None,
    }
}

/// 導入の計画。同じ project の無効の jar があれば、ダウンロードせずにそれを有効にする。
enum Planned<'r> {
    Download(InstallPlan),
    Enable(&'r Row),
}

impl Planned<'_> {
    fn into_plan(self) -> InstallPlan {
        match self {
            Planned::Download(plan) => plan,
            Planned::Enable(row) => InstallPlan {
                enable: vec![row.item.rel_path.clone()],
                ..InstallPlan::default()
            },
        }
    }
}

/// 導入しようとする project がすでに入っているか。有効なものがあれば断り、無効なものだけならそれを返す。
fn installed_root<'r>(rows: &'r [Row], project_id: &str) -> Result<Option<&'r Row>, CoreError> {
    let mut disabled = None;
    for row in rows
        .iter()
        .filter(|r| r.item.project_id.as_deref() == Some(project_id))
    {
        if row.item.enabled {
            return Err(CoreError::ContentAlreadyInstalled {
                name: display_name(row),
            });
        }
        disabled.get_or_insert(row);
    }
    Ok(disabled)
}

impl Launcher {
    fn content_target(&self, id: &str) -> Result<Target, CoreError> {
        let inst = instance::get(&self.env.db, id)?;
        Ok(Target {
            game_dir: instance::game_dir(&self.env.paths, &inst),
            loaders: loader_slugs(Some(inst.loader.as_str())),
            inst,
        })
    }

    /// MOD を入れる先。ローダーの無い（バニラの）インスタンスは断る。
    fn install_target(&self, id: &str) -> Result<Target, CoreError> {
        let t = self.content_target(id)?;
        if t.loaders.is_empty() {
            return Err(CoreError::ContentNoLoader);
        }
        Ok(t)
    }

    /// 依存のエラーに、分かれば project の表示名を足す。引けなければそのまま返す。
    async fn with_dependency_name(&self, err: CoreError) -> CoreError {
        match err {
            CoreError::ContentDependency {
                project_id,
                cause,
                name: None,
            } => {
                let name = modrinth::projects(
                    &self.env.http,
                    &self.env.urls.modrinth_api,
                    std::slice::from_ref(&project_id),
                )
                .await
                .ok()
                .and_then(|found| found.into_iter().find(|p| p.project_id == project_id))
                .map(|p| p.name);
                CoreError::ContentDependency {
                    project_id,
                    cause,
                    name,
                }
            }
            other => other,
        }
    }

    fn api_source(&self, t: &Target) -> ApiSource {
        ApiSource {
            http: self.env.http.clone(),
            api: self.env.urls.modrinth_api.clone(),
            mc_version: t.inst.mc_version.clone(),
            loaders: t.loaders.clone(),
        }
    }

    async fn mod_rows(&self, id: &str) -> Result<Vec<Row>, CoreError> {
        let (db, id) = (self.env.db.clone(), id.to_string());
        blocking(move || store::rows(&db, &id, ContentKind::Mod)).await
    }

    /// Modrinth で MOD を検索する。`instance_id` があれば、そのインスタンスの Minecraft とローダーで絞る。
    pub async fn search_modrinth(
        &self,
        instance_id: Option<&str>,
        mut query: SearchQuery,
        offset: u32,
    ) -> Result<SearchPage, CoreError> {
        if let Some(id) = instance_id {
            let t = self.content_target(id)?;
            query.mc_version = Some(t.inst.mc_version);
            query.loaders = t.loaders;
        }
        modrinth::search(
            &self.env.http,
            &self.env.urls.modrinth_api,
            &query,
            offset,
            SEARCH_LIMIT,
        )
        .await
    }

    /// 導入の計画を立てる。ファイルには触らない。同じ project がすでに有効で入っていれば断る。
    async fn plan_install<'r>(
        &self,
        t: &Target,
        rows: &'r [Row],
        project_id: &str,
    ) -> Result<Planned<'r>, CoreError> {
        modrinth::check_project_id(project_id)?;
        if let Some(row) = installed_root(rows, project_id)? {
            return Ok(Planned::Enable(row));
        }
        let source = self.api_source(t);
        let planned = async {
            let versions = modrinth::project_versions(
                &source.http,
                &source.api,
                project_id,
                &source.mc_version,
                &source.loaders,
            )
            .await?;
            let root = modrinth::pick_version(&versions)
                .cloned()
                .ok_or_else(|| no_version(project_id))?;
            resolve(&source, root, &installed_of(rows), None).await
        }
        .await;
        match planned {
            Ok(plan) => Ok(Planned::Download(plan)),
            Err(err) => Err(self.with_dependency_name(err).await),
        }
    }

    /// 導入の前に、入る版と依存の数、非互換の導入済みの MOD を調べる。
    pub async fn preview_content_install(
        &self,
        instance_id: &str,
        project_id: &str,
    ) -> Result<InstallPreview, CoreError> {
        let t = self.install_target(instance_id)?;
        let rows = self.mod_rows(instance_id).await?;
        let plan = match self.plan_install(&t, &rows, project_id).await? {
            Planned::Download(plan) => plan,
            Planned::Enable(row) => {
                return Ok(InstallPreview {
                    version_number: row.item.version_number.clone().unwrap_or_default(),
                    dependency_count: 0,
                    incompatible: Vec::new(),
                    enables_installed: true,
                });
            }
        };
        let root = plan.files.last().ok_or_else(|| no_version(project_id))?;
        Ok(InstallPreview {
            version_number: root.version.version_number.clone(),
            dependency_count: clamp_u32(plan.files.len().saturating_sub(1) as u64),
            incompatible: incompatible_names(&plan, &rows),
            enables_installed: false,
        })
    }

    /// MOD と必須の依存を導入するタスクを始める。非互換の導入済みの MOD があって
    /// `accept_incompatible` が偽なら、何も入れずに失敗する。同じ project が有効で入っていれば
    /// `ContentAlreadyInstalled` で失敗し、無効で入っていればそれを有効にするだけにする。
    pub fn install_content(
        &self,
        instance_id: &str,
        project_id: &str,
        accept_incompatible: bool,
    ) -> Result<String, CoreError> {
        self.install_target(instance_id)?;
        self.ensure_idle(instance_id)?;
        self.ensure_no_pending_operation(instance_id)?;
        let this = self.clone();
        let (id, project_id) = (instance_id.to_string(), project_id.to_string());
        Ok(self
            .tasks
            .spawn(CONTENT_KIND, Some(id.clone()), move |handle| async move {
                let _guard = this.locks.lock(&id).await;
                let result = this
                    .run_install(&id, &project_id, accept_incompatible, &handle)
                    .await;
                this.updates.invalidate(&id);
                result
            }))
    }

    /// タスクの本体の最初の確認。`ensure_idle` は自分自身のタスクを数えるので使わない。
    fn ensure_content_task_may_run(&self, id: &str) -> Result<(), CoreError> {
        if self.processes.is_running(id)? {
            return Err(CoreError::InstanceBusy {
                instance_id: id.to_string(),
            });
        }
        self.ensure_no_pending_operation(id)
    }

    async fn run_install(
        &self,
        id: &str,
        project_id: &str,
        accept_incompatible: bool,
        handle: &TaskHandle,
    ) -> Result<(), CoreError> {
        self.ensure_content_task_may_run(id)?;
        handle.set_phase(TaskPhase::Resolving);
        let t = self.install_target(id)?;
        let rows = self.mod_rows(id).await?;
        let planned = self.plan_install(&t, &rows, project_id).await?;
        let enables_installed = matches!(planned, Planned::Enable(_));
        let plan = planned.into_plan();
        let names = incompatible_names(&plan, &rows);
        if !names.is_empty() && !accept_incompatible {
            return Err(CoreError::ContentIncompatible {
                names: names.join(", "),
            });
        }
        handle.set_phase(TaskPhase::Downloading);
        let progress = handle.clone();
        let outcome = self
            .apply(&t, id, handle, &plan, move |done, total| {
                progress.set_progress(clamp_u32(done / 1024), clamp_u32(total / 1024));
            })
            .await?;
        self.store_report(
            handle,
            ContentTaskReport {
                // 無効の jar を有効にしただけのときも、頼まれた MOD が入ったことにする
                installed: outcome.installed + u32::from(enables_installed),
                dependencies_added: outcome.dependencies_added,
                updated: 0,
                failures: Vec::new(),
            },
        );
        if let Err(err) = settings::record_last_added_instance(&self.env.db, id, now_ms()) {
            tracing::warn!(
                instance_id = id,
                code = err.code(),
                "failed to record the last added instance"
            );
        }
        Ok(())
    }

    async fn apply(
        &self,
        t: &Target,
        id: &str,
        handle: &TaskHandle,
        plan: &InstallPlan,
        on_progress: impl Fn(u64, u64) + Send + Sync,
    ) -> Result<ApplyOutcome, CoreError> {
        let cx = ApplyContext {
            db: &self.env.db,
            http: &self.env.http,
            paths: &self.env.paths,
            game_dir: &t.game_dir,
            instance_id: id,
            token: handle.token(),
            cdn: &self.env.urls.modrinth_cdn,
        };
        apply_plan(&cx, plan, now_ms(), on_progress).await
    }

    fn store_report(&self, handle: &TaskHandle, report: ContentTaskReport) {
        self.reports.lock().unwrap().insert(handle.id(), report);
    }

    /// 導入と更新のタスクの結果。取り出しても残す（新しい 50 件まで）。
    pub fn content_task_report(&self, task_id: &str) -> Option<ContentTaskReport> {
        self.reports.lock().unwrap().reports.get(task_id).cloned()
    }

    /// 更新を確認する。結果はインスタンスごとに 15 分使い回し、`force` で通信し直す。
    pub async fn check_content_updates(
        &self,
        instance_id: &str,
        force: bool,
    ) -> Result<UpdateCheck, CoreError> {
        let t = self.content_target(instance_id)?;
        if !force && let Some(cached) = self.updates.get(instance_id) {
            return Ok(cached);
        }
        let generation = self.updates.generation();
        let rows = self.mod_rows(instance_id).await?;
        let check = updates::check(
            &self.env.http,
            &self.env.urls.modrinth_api,
            &rows,
            &t.inst.mc_version,
            &t.loaders,
        )
        .await?;
        self.updates.put(instance_id, generation, check.clone());
        Ok(check)
    }

    /// 選んだ MOD を更新するタスクを始める（「すべて更新」も同じ）。
    /// 1 件ずつ解決して入れ替え、失敗した分は結果に集めて続ける。
    pub fn update_content(
        &self,
        instance_id: &str,
        rel_paths: Vec<String>,
    ) -> Result<String, CoreError> {
        self.ensure_idle(instance_id)?;
        self.ensure_no_pending_operation(instance_id)?;
        let this = self.clone();
        let id = instance_id.to_string();
        Ok(self
            .tasks
            .spawn(CONTENT_KIND, Some(id.clone()), move |handle| async move {
                let _guard = this.locks.lock(&id).await;
                let result = this.run_update(&id, &rel_paths, &handle).await;
                this.updates.invalidate(&id);
                result
            }))
    }

    async fn run_update(
        &self,
        id: &str,
        rel_paths: &[String],
        handle: &TaskHandle,
    ) -> Result<(), CoreError> {
        self.ensure_content_task_may_run(id)?;
        handle.set_phase(TaskPhase::Resolving);
        let t = self.content_target(id)?;
        // 最新の版は、対象のハッシュをまとめて 1 回で引く
        let rows = self.mod_rows(id).await?;
        let mut hashes: Vec<String> = Vec::new();
        for row in rows.iter().filter(|r| rel_paths.contains(&r.item.rel_path)) {
            if !row.item.sha1.is_empty() && !hashes.contains(&row.item.sha1) {
                hashes.push(row.item.sha1.clone());
            }
        }
        let latest = updates_by_hash(
            &self.env.http,
            &self.env.urls.modrinth_api,
            &hashes,
            &t.inst.mc_version,
            &t.loaders,
        )
        .await?;

        handle.set_phase(TaskPhase::Downloading);
        let total = clamp_u32(rel_paths.len() as u64);
        handle.set_progress(0, total);
        let mut report = ContentTaskReport::default();
        let mut cancelled = false;
        for (i, rel) in rel_paths.iter().enumerate() {
            if handle.token().is_cancelled() {
                cancelled = true;
                break;
            }
            match self.update_one(&t, id, rel, &latest, handle).await {
                Ok(outcome) => {
                    report.updated += outcome.replaced;
                    report.dependencies_added += outcome.dependencies_added;
                }
                Err(CoreError::Cancelled) => {
                    cancelled = true;
                    break;
                }
                Err(err) => report.failures.push(ItemFailure {
                    name: self.failure_name(id, rel),
                    error: err.to_payload(),
                }),
            }
            handle.set_progress(clamp_u32(i as u64 + 1), total);
        }
        self.store_report(handle, report);
        if cancelled {
            return Err(CoreError::Cancelled);
        }
        Ok(())
    }

    fn failure_name(&self, id: &str, rel: &str) -> String {
        store::get(&self.env.db, id, rel)
            .ok()
            .flatten()
            .map_or_else(
                || rel.rsplit('/').next().unwrap_or(rel).to_string(),
                |row| display_name(&row),
            )
    }

    /// 1 件を新しい版に入れ替える。前の更新で行が変わっているので、行は毎回読み直す。
    async fn update_one(
        &self,
        t: &Target,
        id: &str,
        rel: &str,
        latest: &HashMap<String, FullVersion>,
        handle: &TaskHandle,
    ) -> Result<ApplyOutcome, CoreError> {
        let rows = self.mod_rows(id).await?;
        let row = rows
            .iter()
            .find(|r| r.item.rel_path == rel)
            .ok_or_else(|| CoreError::ContentNotFound {
                path: rel.to_string(),
            })?;
        let version =
            latest
                .get(&row.item.sha1)
                .cloned()
                .ok_or_else(|| CoreError::ContentDependency {
                    project_id: row
                        .item
                        .project_id
                        .clone()
                        .unwrap_or_else(|| rel.to_string()),
                    cause: DependencyCause::NoVersion,
                    name: Some(display_name(row)),
                })?;
        let plan = match resolve(
            &self.api_source(t),
            version,
            &installed_of(&rows),
            Some(rel),
        )
        .await
        {
            Ok(plan) => plan,
            Err(err) => return Err(self.with_dependency_name(err).await),
        };
        let names = incompatible_names(&plan, &rows);
        if !names.is_empty() {
            // 更新では確認を取れないので、非互換の MOD がある更新は入れない
            return Err(CoreError::ContentIncompatible {
                names: names.join(", "),
            });
        }
        self.apply(t, id, handle, &plan, |_, _| {}).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_store_keeps_only_the_newest() {
        let mut store = ReportStore::default();
        for i in 0..REPORT_KEEP + 5 {
            store.insert(
                &format!("t{i}"),
                ContentTaskReport {
                    installed: i as u32,
                    ..ContentTaskReport::default()
                },
            );
        }
        assert_eq!(store.reports.len(), REPORT_KEEP);
        assert!(!store.reports.contains_key("t0"));
        assert!(store.reports.contains_key("t5"));
        assert!(store.reports.contains_key(&format!("t{}", REPORT_KEEP + 4)));
    }
}
