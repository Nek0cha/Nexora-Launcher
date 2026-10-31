//! UI からの操作の入口。インストールと起動をタスクとして動かす。

mod content;

pub use content::{InstallPreview, VersionChoice};

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;
use uuid::Uuid;

use crate::auth::login::{LOGIN_KIND, LoginMethod, LoginNotifier, UrlOpener, run_login};
use crate::auth::service::AuthService;
use crate::content::identify::{IdentifyReport, identify};
use crate::content::journal;
use crate::content::modrinth::categories;
use crate::content::ops::{self, AddReport};
use crate::content::scan::scan;
use crate::content::store;
use crate::content::updates::UpdateCache;
use crate::content::{ContentItem, ContentKind};
use crate::db::Db;
use crate::error::CoreError;
use crate::install::plan::{check_profile, plan_game};
use crate::install::{
    InstallEnv, Urls, cancellable, fetch_with_phases, forge, install, is_forge_like, now_ms,
    overlay_loader,
};
use crate::instance::copy::copy_dir;
use crate::instance::folder::FolderKind;
use crate::instance::lock::InstanceLocks;
use crate::instance::screenshots::{self, ImportReport, Screenshot};
use crate::instance::{self, Instance, InstanceState, JavaMode, MemoryMode, NewInstance};
use crate::java::verify::{JavaProbe, SystemProbe};
use crate::java::{find_registered, finish_runtime, prepare_runtime, requirement};
use crate::launch::argfile;
use crate::launch::args::{ArgInputs, CommandLine, WINDOWS_COMMAND_LIMIT, build_args, fit_command};
use crate::launch::jvm_args;
use crate::launch::natives::extract_natives;
use crate::launch::process::{GameNotifier, GameProcesses, RunningSession, SpawnSpec};
use crate::meta::loader::{LoaderKind, LoaderVersionEntry};
use crate::meta::mojang::manifest::{
    VersionEntry, load_pinned_asset_index, load_pinned_version, release_versions,
};
use crate::meta::mojang::rules::RuleEnv;
use crate::meta::write_atomic;
use crate::net::{Depth, Http};
use crate::paths::AppPaths;
use crate::settings;
use crate::skins::defaults::{self, DefaultSkin};
use crate::sync::apply::{ApplyFailure, ApplyOutcome, FailReason, apply_before_launch};
use crate::sync::proposals::{self, Resolution};
use crate::sync::registry::Category;
use crate::sync::store as sync_store;
use crate::sync::wire::{
    InstanceSync, JoinResult, PendingView, SyncApplyFinishedEvent, SyncField, SyncFieldValue,
    SyncProfileView,
};
use crate::sync::{self, JoinMode, SyncCtx};
use crate::task::{TaskHandle, TaskManager, TaskPhase};

pub const INSTALL_KIND: &str = "install";
pub const LAUNCH_KIND: &str = "launch";
pub const DUPLICATE_KIND: &str = "duplicate";
/// Modrinth からの導入と更新のタスク。
pub const CONTENT_KIND: &str = "content";
/// メモリが自動のときの割り当て（自動設定はフェーズ 9）。
const AUTO_MEMORY_MB: u32 = 2048;

/// 同期の処理を `spawn_blocking` で動かす。
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, CoreError> + Send + 'static,
) -> Result<T, CoreError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| CoreError::Unexpected {
            message: e.to_string(),
        })?
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreatedInstance {
    pub instance: Instance,
    pub task_id: String,
}

#[derive(Clone)]
pub struct Launcher {
    env: InstallEnv,
    tasks: TaskManager,
    processes: GameProcesses,
    /// ゲームの通知先。起動前の共通設定の適用の結果もここに送る。
    notifier: Arc<dyn GameNotifier>,
    locks: InstanceLocks,
    auth: AuthService,
    /// reattach を一度だけ行うための印。二度目以降は監視を重ねないよう何もしない。
    reattached: Arc<AtomicBool>,
    /// 更新の確認の結果（インスタンスごと、15 分）。
    updates: Arc<UpdateCache>,
    /// 導入と更新のタスクの結果（新しい順に一定数だけ）。
    reports: Arc<Mutex<content::ReportStore>>,
}

fn already_running(instance_id: &str) -> CoreError {
    CoreError::AlreadyRunning {
        instance_id: instance_id.to_string(),
    }
}

/// 件数を u32 に収める。
fn clamp_count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// タスクの進捗（u32）に収める。
fn clamp_u32(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// インスタンスの設定による -Xmx の値（MB）。
fn memory_mb(inst: &Instance) -> u32 {
    match (inst.memory_mode, inst.memory_mb) {
        (MemoryMode::Manual, Some(mb)) => mb,
        // 手動で値がないのは保存時の検証で起きないが、起きても自動と同じ値で起動する
        (MemoryMode::Manual, None) | (MemoryMode::Auto, _) => AUTO_MEMORY_MB,
    }
}

/// 手動の Java の実行ファイル。自動なら None。存在しなければ `JavaPathMissing`。
/// Mojang の要求する major と合うかは確かめない（利用者の責任とする）。
fn manual_java(inst: &Instance) -> Result<Option<PathBuf>, CoreError> {
    if inst.java_mode == JavaMode::Auto {
        return Ok(None);
    }
    let path = inst.java_path.as_deref().unwrap_or("").trim();
    if path.is_empty() || !Path::new(path).is_file() {
        return Err(CoreError::JavaPathMissing {
            path: path.to_string(),
        });
    }
    Ok(Some(PathBuf::from(path)))
}

impl Launcher {
    pub fn new(
        paths: AppPaths,
        db: Db,
        tasks: TaskManager,
        notifier: Arc<dyn GameNotifier>,
        auth: AuthService,
    ) -> Result<Self, CoreError> {
        Ok(Self::with(
            paths,
            db,
            tasks,
            notifier,
            Http::new()?,
            Urls::default(),
            Arc::new(SystemProbe),
            auth,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with(
        paths: AppPaths,
        db: Db,
        tasks: TaskManager,
        notifier: Arc<dyn GameNotifier>,
        http: Http,
        urls: Urls,
        probe: Arc<dyn JavaProbe>,
        auth: AuthService,
    ) -> Self {
        let processes = GameProcesses::new(db.clone(), paths.clone(), notifier.clone());
        Self {
            env: InstallEnv::new(paths, db, http, urls, probe),
            tasks,
            processes,
            notifier,
            locks: InstanceLocks::default(),
            auth,
            reattached: Arc::new(AtomicBool::new(false)),
            updates: Arc::new(UpdateCache::default()),
            reports: Arc::default(),
        }
    }

    /// 結合テスト用。更新の確認のキャッシュが使う時計を差し替える。
    #[doc(hidden)]
    #[must_use]
    pub fn with_update_clock(mut self, clock: crate::content::updates::Clock) -> Self {
        self.updates = Arc::new(UpdateCache::with_clock(clock));
        self
    }

    pub fn db(&self) -> &Db {
        &self.env.db
    }

    pub fn auth(&self) -> &AuthService {
        &self.auth
    }

    /// デフォルトスキンの一覧（ゲームの jar から取り出し、キャッシュする）。
    pub async fn list_default_skins(&self) -> Result<Vec<DefaultSkin>, CoreError> {
        defaults::list_defaults(&self.env.http, &self.env.paths, &self.env.urls.manifest).await
    }

    /// キャッシュ済みのデフォルトスキンの画素のハッシュ（通信しない）。
    pub async fn cached_default_skin_hashes(&self) -> std::collections::HashSet<String> {
        defaults::cached_hashes(&self.env.paths).await
    }

    /// デフォルトスキンをアカウントに適用する（ライブラリには足さない）。
    pub async fn apply_default_skin(&self, account_id: Uuid, key: &str) -> Result<(), CoreError> {
        defaults::apply_default(
            &self.env.http,
            &self.env.paths,
            &self.env.urls.manifest,
            &self.auth,
            account_id,
            key,
        )
        .await
    }

    /// ログインのタスクを始める。ログインのタスクが動いていれば、新しく作らずにその ID を返す。
    pub fn begin_login(
        &self,
        method: LoginMethod,
        opener: Arc<dyn UrlOpener>,
        notifier: Arc<dyn LoginNotifier>,
    ) -> String {
        let auth = self.auth.clone();
        self.tasks
            .spawn_unique(LOGIN_KIND, None, move |handle| async move {
                run_login(&auth, method, opener.as_ref(), notifier.as_ref(), &handle)
                    .await
                    .map(|_| ())
            })
    }

    /// アカウントを削除する。そのアカウントでゲームが実行中なら `AccountInUse` で断る。
    pub async fn remove_account(&self, id: Uuid) -> Result<(), CoreError> {
        let account_id = id.simple().to_string();
        if self.processes.is_account_running(&account_id)? {
            return Err(CoreError::AccountInUse { account_id });
        }
        self.auth.remove(id).await
    }

    pub async fn list_versions(&self) -> Result<Vec<VersionEntry>, CoreError> {
        release_versions(&self.env.meta.manifest().await?)
    }

    /// ローダーが対応する Minecraft のバージョン（メタデータの順）。
    pub async fn list_loader_game_versions(
        &self,
        kind: LoaderKind,
    ) -> Result<Vec<String>, CoreError> {
        self.env.loader.game_versions(kind).await
    }

    /// mc_version に使えるローダーの版（新しい順）。
    pub async fn list_loader_versions(
        &self,
        kind: LoaderKind,
        mc_version: &str,
    ) -> Result<Vec<LoaderVersionEntry>, CoreError> {
        self.env.loader.loader_versions(kind, mc_version).await
    }

    pub fn list_instances(&self) -> Result<Vec<Instance>, CoreError> {
        instance::list(&self.env.db)
    }

    pub fn create_instance(&self, new: &NewInstance) -> Result<CreatedInstance, CoreError> {
        let instance = instance::create(&self.env.db, &self.env.paths, new, now_ms())?;
        let task_id = self.start_install(&instance.id);
        Ok(CreatedInstance { instance, task_id })
    }

    /// インスタンスを削除する。起動中、またはインストールか起動のタスクが動いていれば `InstanceBusy`。
    /// state の列が installing でも、タスクが動いていなければ削除できる。
    /// 確認は、待たずに断れるようロックの前にも行い、競合を防ぐためロックを取った後にもう一度行う。
    pub async fn delete_instance(&self, id: &str) -> Result<(), CoreError> {
        self.ensure_idle(id)?;
        let _guard = self.locks.lock(id).await;
        self.ensure_idle(id)?;
        instance::delete(&self.env.db, &self.env.paths, id)
    }

    /// インスタンスの設定を更新する。起動中でも更新でき（反映は次の起動から）、ロックの中で書く。
    pub async fn update_instance(
        &self,
        id: &str,
        settings: &instance::InstanceSettings,
    ) -> Result<Instance, CoreError> {
        let _guard = self.locks.lock(id).await;
        instance::update_settings(&self.env.db, id, settings)
    }

    /// 共通設定の処理を `spawn_blocking` で動かす。
    async fn sync_blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(&SyncCtx) -> Result<T, CoreError> + Send + 'static,
    ) -> Result<T, CoreError> {
        let (db, paths) = (self.env.db.clone(), self.env.paths.clone());
        blocking(move || {
            f(&SyncCtx {
                db: &db,
                paths: &paths,
            })
        })
        .await
    }

    pub async fn list_sync_profiles(&self) -> Result<Vec<SyncProfileView>, CoreError> {
        self.sync_blocking(sync::list_profile_views).await
    }

    /// 登録表の項目の一覧（除外の画面のため）。
    pub fn list_sync_fields(&self) -> Vec<SyncField> {
        sync::list_fields()
    }

    /// 共通設定の値（提案の横に共通の値を出すため）。
    pub async fn get_sync_profile_values(
        &self,
        profile_id: &str,
    ) -> Result<Vec<SyncFieldValue>, CoreError> {
        let profile_id = profile_id.to_string();
        self.sync_blocking(move |ctx| sync::profile_values(ctx, &profile_id))
            .await
    }

    /// インスタンスの例外（このインスタンスだけの値）を決める。`None` なら例外を外す。
    pub async fn set_sync_override(
        &self,
        instance_id: &str,
        field_id: &str,
        value: Option<String>,
    ) -> Result<(), CoreError> {
        let (instance_id, field_id) = (instance_id.to_string(), field_id.to_string());
        self.sync_blocking(move |ctx| {
            instance::get(ctx.db, &instance_id)?;
            sync::set_override(ctx, &instance_id, &field_id, value.as_deref())
        })
        .await
    }

    /// インスタンスの options.txt から共通設定を作り、そのインスタンスを参加させる。
    pub async fn create_sync_profile_from_instance(
        &self,
        instance_id: &str,
        name: &str,
    ) -> Result<SyncProfileView, CoreError> {
        let (instance_id, name) = (instance_id.to_string(), name.to_string());
        self.sync_blocking(move |ctx| {
            let profile = sync::create_profile_from_instance(ctx, &instance_id, &name)?;
            sync::profile_view(ctx, profile)
        })
        .await
    }

    pub async fn rename_sync_profile(&self, profile_id: &str, name: &str) -> Result<(), CoreError> {
        let (profile_id, name) = (profile_id.to_string(), name.to_string());
        self.sync_blocking(move |ctx| sync::rename_profile(ctx, &profile_id, &name))
            .await
    }

    /// 共通設定を消す。参加していたインスタンスの options.txt は触らない。
    pub async fn delete_sync_profile(&self, profile_id: &str) -> Result<(), CoreError> {
        let profile_id = profile_id.to_string();
        self.sync_blocking(move |ctx| {
            sync::find_profile(ctx, &profile_id)?;
            sync_store::delete_profile(ctx.db, &profile_id)
        })
        .await
    }

    pub async fn set_sync_categories(
        &self,
        profile_id: &str,
        categories: Vec<Category>,
    ) -> Result<(), CoreError> {
        let profile_id = profile_id.to_string();
        self.sync_blocking(move |ctx| {
            sync::find_profile(ctx, &profile_id)?;
            sync::set_categories(ctx, &profile_id, &categories)
        })
        .await
    }

    /// 参加させたときの差の件数。書き込まない。
    pub async fn preview_join_sync_profile(
        &self,
        instance_id: &str,
        profile_id: &str,
    ) -> Result<u32, CoreError> {
        let (instance_id, profile_id) = (instance_id.to_string(), profile_id.to_string());
        self.sync_blocking(move |ctx| {
            sync::preview_join(ctx, &instance_id, &profile_id).map(clamp_count)
        })
        .await
    }

    /// 参加させる。起動中のインスタンスも参加できる（書き込みは起動前だけ）。
    pub async fn join_sync_profile(
        &self,
        instance_id: &str,
        profile_id: &str,
        mode: JoinMode,
    ) -> Result<JoinResult, CoreError> {
        let (instance_id, profile_id) = (instance_id.to_string(), profile_id.to_string());
        self.sync_blocking(move |ctx| {
            let report = sync::join_instance(ctx, &instance_id, &profile_id, mode)?;
            Ok(JoinResult {
                differing: clamp_count(report.differing),
            })
        })
        .await
    }

    pub async fn leave_sync_profile(&self, instance_id: &str) -> Result<(), CoreError> {
        let instance_id = instance_id.to_string();
        self.sync_blocking(move |ctx| {
            instance::get(ctx.db, &instance_id)?;
            sync::leave_instance(ctx, &instance_id)
        })
        .await
    }

    /// インスタンスの共通設定の状態。参加していなければ `profile` が `None`。
    pub async fn get_instance_sync(&self, instance_id: &str) -> Result<InstanceSync, CoreError> {
        let instance_id = instance_id.to_string();
        self.sync_blocking(move |ctx| sync::instance_sync(ctx, &instance_id))
            .await
    }

    pub async fn set_sync_excluded(
        &self,
        instance_id: &str,
        field_ids: Vec<String>,
    ) -> Result<(), CoreError> {
        let instance_id = instance_id.to_string();
        self.sync_blocking(move |ctx| {
            instance::get(ctx.db, &instance_id)?;
            if sync::membership(ctx, &instance_id)?.is_none() {
                return Err(CoreError::SyncNotMember);
            }
            sync::set_excluded(ctx, &instance_id, &field_ids)
        })
        .await
    }

    pub async fn pending_sync_proposals(&self, profile_id: &str) -> Result<PendingView, CoreError> {
        let profile_id = profile_id.to_string();
        self.sync_blocking(move |ctx| proposals::pending(ctx, &profile_id).map(PendingView::from))
            .await
    }

    pub async fn resolve_sync_proposal(
        &self,
        instance_id: &str,
        field_id: &str,
        resolution: Resolution,
        chosen_value: Option<String>,
    ) -> Result<(), CoreError> {
        let (instance_id, field_id) = (instance_id.to_string(), field_id.to_string());
        self.sync_blocking(move |ctx| {
            proposals::resolve(ctx, &instance_id, &field_id, resolution, chosen_value)
        })
        .await
    }

    /// 中身の一覧。ディスクを走査して DB を合わせてから返す。
    /// 起動中でも読めるようロックは取らない（走査はロックを待たず、ops との競合は次の走査で直る）。
    pub async fn list_content(
        &self,
        id: &str,
        kind: ContentKind,
    ) -> Result<Vec<ContentItem>, CoreError> {
        let inst = instance::get(&self.env.db, id)?;
        let (db, game_dir, id) = (
            self.env.db.clone(),
            instance::game_dir(&self.env.paths, &inst),
            id.to_string(),
        );
        blocking(move || {
            scan(&db, &game_dir, &id, kind, &CancellationToken::new())?;
            store::list(&db, &id, kind)
        })
        .await
    }

    /// 未特定の中身を Modrinth で特定する（3 種類まとめて）。
    /// 通信は長くかかりうるので、ロックも `ensure_idle` も使わず、遊ぶのを妨げない。
    pub async fn identify_content(&self, id: &str) -> Result<IdentifyReport, CoreError> {
        let inst = instance::get(&self.env.db, id)?;
        let game_dir = instance::game_dir(&self.env.paths, &inst);
        let result = identify(
            &self.env.db,
            &self.env.http,
            &self.env.urls.modrinth_api,
            &self.env.paths.content_icons_dir(),
            &game_dir,
            id,
            now_ms(),
        )
        .await;
        // 版や入手元が埋まると更新の判定が変わるので、成否にかかわらず確認の結果を捨てる
        self.updates.invalidate(id);
        result
    }

    /// 中身を有効または無効にする。起動中とタスク中は `InstanceBusy`（削除と同じ順で確認する）。
    pub async fn set_content_enabled(
        &self,
        id: &str,
        kind: ContentKind,
        rel_path: &str,
        enabled: bool,
    ) -> Result<(), CoreError> {
        let rel_path = rel_path.to_string();
        self.mutate_content(id, move |db, game_dir, id| {
            ops::set_enabled(db, game_dir, id, kind, &rel_path, enabled)
        })
        .await
    }

    /// 中身を削除する。断る条件は `set_content_enabled` と同じ。
    pub async fn remove_content(
        &self,
        id: &str,
        kind: ContentKind,
        rel_path: &str,
    ) -> Result<(), CoreError> {
        let rel_path = rel_path.to_string();
        self.mutate_content(id, move |db, game_dir, id| {
            ops::remove(db, game_dir, id, kind, &rel_path)
        })
        .await
    }

    /// ファイルを中身として追加する。断る条件は `set_content_enabled` と同じ。
    pub async fn add_content_files(
        &self,
        id: &str,
        kind: ContentKind,
        paths: Vec<PathBuf>,
        overwrite: bool,
    ) -> Result<AddReport, CoreError> {
        self.mutate_content(id, move |db, game_dir, id| {
            ops::add_files(db, game_dir, id, kind, &paths, overwrite)
        })
        .await
    }

    /// 中身を変える操作の共通部分。`ensure_idle`、ロック、もう一度 `ensure_idle`、`spawn_blocking`。
    async fn mutate_content<T: Send + 'static>(
        &self,
        id: &str,
        f: impl FnOnce(&Db, &Path, &str) -> Result<T, CoreError> + Send + 'static,
    ) -> Result<T, CoreError> {
        self.ensure_idle(id)?;
        let _guard = self.locks.lock(id).await;
        self.ensure_idle(id)?;
        self.ensure_no_pending_operation(id)?;
        let inst = instance::get(&self.env.db, id)?;
        let (db, game_dir, id) = (
            self.env.db.clone(),
            instance::game_dir(&self.env.paths, &inst),
            id.to_string(),
        );
        let result = blocking({
            let id = id.clone();
            move || f(&db, &game_dir, &id)
        })
        .await;
        // 中身が変わったので、更新の確認の結果は古い
        self.updates.invalidate(&id);
        result
    }

    /// Modrinth のカテゴリー名のうち、種類に合うもの。
    pub async fn content_categories(&self, kind: ContentKind) -> Result<Vec<String>, CoreError> {
        let tags = categories(
            &self.env.http,
            &self.env.urls.modrinth_api,
            &self.env.paths.meta_cache_dir(),
        )
        .await?;
        Ok(tags
            .into_iter()
            .filter(|t| t.project_type == kind.as_str())
            .map(|t| t.name)
            .collect())
    }

    /// アイコンの PNG。`key` は 40 桁の 16 進（sha1）だけを受け付け、それ以外と無いものは `None`。
    pub fn content_icon(&self, key: &str) -> Result<Option<Vec<u8>>, CoreError> {
        if key.len() != 40 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(None);
        }
        let path = self
            .env
            .paths
            .content_icons_dir()
            .join(format!("{key}.png"));
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(CoreError::Io {
                path: path.display().to_string(),
                source,
            }),
        }
    }

    /// 削除と複製の前の確認。起動中、またはインストールか起動のタスクが動いていれば `InstanceBusy`。
    fn ensure_idle(&self, id: &str) -> Result<(), CoreError> {
        instance::get(&self.env.db, id)?;
        // state の列だけでは断らない（インストール中に落ちた行を削除や複製できなくなるため）
        if self.processes.is_running(id)? || self.active_task(id).is_some() {
            return Err(CoreError::InstanceBusy {
                instance_id: id.to_string(),
            });
        }
        Ok(())
    }

    /// インスタンスを複製するタスクを始め、その ID を返す。タスクの `instance_id` は元のインスタンス。
    /// 断る条件と確認の順は削除と同じ（ロックの前に一度、ロックを取った後にもう一度）。
    /// 同じインスタンスの複製が動いていれば、新しく作らずにその ID を返す。
    pub fn duplicate_instance(&self, id: &str) -> Result<String, CoreError> {
        self.ensure_idle(id)?;
        let this = self.clone();
        let source_id = id.to_string();
        Ok(self.tasks.spawn_unique(
            DUPLICATE_KIND,
            Some(source_id.clone()),
            move |handle| async move {
                let _guard = this.locks.lock(&source_id).await;
                this.ensure_idle(&source_id)?;
                this.duplicate(&source_id, &handle).await
            },
        ))
    }

    /// 行をインストール中として作り、ゲームディレクトリをコピーし、終えたら元と同じ状態にする。
    /// 失敗か取り消しなら、コピー先と行を消す。
    /// 元のインスタンスの行とディレクトリには書き込まない。
    async fn duplicate(&self, source_id: &str, handle: &TaskHandle) -> Result<(), CoreError> {
        let env = &self.env;
        let source = instance::get(&env.db, source_id)?;
        let copy = instance::duplicate_row(&env.db, &env.paths, source_id, now_ms())?;
        // コピーの途中で、複製の側を起動や削除させない
        let _copy_guard = self.locks.lock(&copy.id).await;
        let src = instance::game_dir(&env.paths, &source);
        let dst = instance::game_dir(&env.paths, &copy);
        if dst.symlink_metadata().is_ok() {
            // 行を作った後に同じ名前のものが置かれた。他人のものなので消さない
            let _ = instance::remove_row(&env.db, &copy.id);
            return Err(CoreError::Io {
                path: dst.display().to_string(),
                source: std::io::ErrorKind::AlreadyExists.into(),
            });
        }
        let token = handle.token().clone();
        let progress = handle.clone();
        let target = dst.clone();
        let result = tokio::task::spawn_blocking(move || {
            copy_dir(&src, &target, &token, &mut |done, total| {
                progress.set_progress(clamp_u32(done), clamp_u32(total));
            })
        })
        .await
        .unwrap_or_else(|e| {
            Err(CoreError::Unexpected {
                message: e.to_string(),
            })
        })
        // コピーを終えてから元と同じ状態にする（途中で落ちた複製を使える状態に見せない）
        .and_then(|()| instance::set_state(&env.db, &copy.id, source.state));
        if result.is_err() {
            self.discard_copy(&copy.id, &dst);
        }
        result
    }

    /// 作りかけの複製を消す。ディレクトリを消せなければ、行は壊れた状態で残す
    /// （一覧に出して利用者が削除できるようにし、見えない残骸を作らないため）。
    fn discard_copy(&self, copy_id: &str, dst: &Path) {
        let db = &self.env.db;
        let removed = match std::fs::remove_dir_all(dst) {
            Ok(()) => true,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => true,
            Err(err) => {
                tracing::warn!(path = %dst.display(), %err, "could not remove a partial copy");
                false
            }
        };
        let cleanup = if removed {
            instance::remove_row(db, copy_id)
        } else {
            instance::set_state(db, copy_id, InstanceState::Broken)
        };
        if let Err(err) = cleanup {
            tracing::warn!(
                instance_id = copy_id,
                code = err.code(),
                "could not clean up a partial copy"
            );
        }
    }

    /// 結合テスト用。インスタンスごとの排他を外から取れるようにする。
    #[doc(hidden)]
    pub fn instance_locks(&self) -> &InstanceLocks {
        &self.locks
    }

    /// state が ready なら起動のタスク、それ以外ならインストールのタスクの ID を返す。
    /// 同じインスタンスにインストールか起動のタスクが動いていれば、新しく作らずにその ID を返す。
    /// `offline` が真なら、通信せずに保存してあるアカウントの情報で起動する（spec 4.7）。
    pub fn launch_instance(&self, instance_id: &str, offline: bool) -> Result<String, CoreError> {
        let inst = instance::get(&self.env.db, instance_id)?;
        if self.processes.is_running(instance_id)? {
            return Err(already_running(instance_id));
        }
        if let Some((active, kind)) = self.active_task_with_kind(instance_id) {
            // 中身の導入や更新は起動ではないので、同じタスクとして返さず断る
            if kind == CONTENT_KIND {
                return Err(CoreError::InstanceBusy {
                    instance_id: instance_id.to_string(),
                });
            }
            return Ok(active);
        }
        Ok(match inst.state {
            InstanceState::Ready => self.start_launch(instance_id, offline),
            InstanceState::Installing | InstanceState::Broken => self.start_install(instance_id),
        })
    }

    /// 動いているゲームを止める（spec 3.1）。終了は待たず、終了後の記録は終了の検知に任せる。
    pub fn stop_instance(&self, instance_id: &str, force: bool) -> Result<(), CoreError> {
        instance::get(&self.env.db, instance_id)?;
        self.processes.stop(instance_id, force)
    }

    /// インスタンスのフォルダの場所。無ければ作る。
    pub fn instance_folder(
        &self,
        instance_id: &str,
        kind: FolderKind,
    ) -> Result<PathBuf, CoreError> {
        let inst = instance::get(&self.env.db, instance_id)?;
        let dir = match kind {
            // 起動のときにゲームの出力を書く場所（start_launch の log_path）と同じ
            FolderKind::Logs => self.env.paths.game_logs_dir().join(&inst.id),
            _ => {
                let mut dir = instance::game_dir(&self.env.paths, &inst);
                if let Some(sub) = kind.subdir() {
                    dir.push(sub);
                }
                dir
            }
        };
        std::fs::create_dir_all(&dir).map_err(|source| CoreError::Io {
            path: dir.display().to_string(),
            source,
        })?;
        Ok(dir)
    }

    /// インスタンスのスクリーンショットを新しい順に返す。
    pub fn list_screenshots(&self, instance_id: &str) -> Result<Vec<Screenshot>, CoreError> {
        let dir = self.instance_folder(instance_id, FolderKind::Screenshots)?;
        screenshots::list(&dir)
    }

    pub fn delete_screenshot(&self, instance_id: &str, name: &str) -> Result<(), CoreError> {
        let dir = self.instance_folder(instance_id, FolderKind::Screenshots)?;
        screenshots::delete(&dir, name)
    }

    /// 画像のファイルをインスタンスのスクリーンショットのフォルダへ写す。
    pub fn import_screenshots(
        &self,
        instance_id: &str,
        paths: &[PathBuf],
    ) -> Result<ImportReport, CoreError> {
        let dir = self.instance_folder(instance_id, FolderKind::Screenshots)?;
        screenshots::import(&dir, paths)
    }

    pub fn list_running(&self) -> Result<Vec<RunningSession>, CoreError> {
        self.processes.list()
    }

    /// 途中で止まった中身の操作を復旧する（spec 0 の 3.7 節）。起動時に、コマンドを受け付ける前に
    /// 同期的に呼ぶ（復旧が終わるまで走査で DB を更新させないため）。1 件の失敗は警告に残して続ける。
    pub fn recover_operations(&self) -> Result<(), CoreError> {
        journal::recover(&self.env.db, &self.env.paths)
    }

    /// ランチャーの起動時に一度だけ呼ぶ（tokio の実行環境の中で）。二度目以降は何もしない。
    pub fn reattach(&self) -> Result<(), CoreError> {
        if self.reattached.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let result = self.processes.reattach();
        if result.is_err() {
            // 失敗は記録の読み出しの時点（監視を始める前）に限られるため、やり直せるようにする
            self.reattached.store(false, Ordering::SeqCst);
        }
        result
    }

    /// このインスタンスの、終了していないインストール、起動、中身のタスク。
    fn active_task(&self, instance_id: &str) -> Option<String> {
        self.active_task_with_kind(instance_id).map(|(id, _)| id)
    }

    fn active_task_with_kind(&self, instance_id: &str) -> Option<(String, String)> {
        self.tasks
            .list()
            .into_iter()
            .find(|t| {
                t.instance_id.as_deref() == Some(instance_id)
                    && (t.kind == INSTALL_KIND || t.kind == LAUNCH_KIND || t.kind == CONTENT_KIND)
                    && !t.state.is_terminal()
            })
            .map(|t| (t.id, t.kind))
    }

    /// 戻しきれなかった操作の記録が残っていれば `InstanceBusy`（次の起動の復旧まで中身を変えない）。
    fn ensure_no_pending_operation(&self, id: &str) -> Result<(), CoreError> {
        if journal::has_prepared(&self.env.db, id)? {
            return Err(CoreError::InstanceBusy {
                instance_id: id.to_string(),
            });
        }
        Ok(())
    }

    fn start_install(&self, instance_id: &str) -> String {
        let this = self.clone();
        let id = instance_id.to_string();
        self.tasks
            .spawn_unique(INSTALL_KIND, Some(id.clone()), move |handle| async move {
                let _guard = this.locks.lock(&id).await;
                if this.processes.is_running(&id)? {
                    return Err(already_running(&id));
                }
                install(&this.env, &id, &handle).await
            })
    }

    fn start_launch(&self, instance_id: &str, offline: bool) -> String {
        let this = self.clone();
        let id = instance_id.to_string();
        self.tasks
            .spawn_unique(LAUNCH_KIND, Some(id.clone()), move |handle| async move {
                let _guard = this.locks.lock(&id).await;
                let result = this.launch(&id, offline, &handle).await;
                if matches!(result, Err(CoreError::LoaderFilesMissing { .. })) {
                    // 起動では直せないため壊れた状態にし、次のプレイで導入し直す（launch_instance を参照）
                    if let Err(err) = instance::set_state(&this.env.db, &id, InstanceState::Broken)
                    {
                        tracing::warn!(
                            instance_id = %id,
                            code = err.code(),
                            "failed to mark the instance for reinstall"
                        );
                    }
                }
                result
            })
    }

    async fn launch(&self, id: &str, offline: bool, handle: &TaskHandle) -> Result<(), CoreError> {
        if self.processes.is_running(id)? {
            return Err(already_running(id));
        }
        let env = &self.env;
        let inst = instance::get(&env.db, id)?;
        // 手動の Java がなければ、ログインやダウンロードより前に知らせる
        let manual_java = manual_java(&inst)?;
        let (extra_jvm_args, had_xmx) = jvm_args::strip_memory(jvm_args::split(&inst.jvm_args));
        if had_xmx {
            tracing::info!(
                instance_id = id,
                "ignoring -Xmx in the JVM arguments; the memory setting is used"
            );
        }
        // ダウンロードより前にセッションを用意する（ログインし直しなどを先に知らせるため）
        let session = if offline {
            self.auth.offline_session()?
        } else {
            // 途中で打ち切ると、新しい refresh token を保存する前に捨てるおそれがあるため、最後まで待つ
            self.auth.session_for_launch().await?
        };
        let token = handle.token();
        handle.set_phase(TaskPhase::Resolving);
        // 固定済みのメタデータだけで解決する。欠けていればそのときだけ取得する
        let vanilla = match load_pinned_version(&env.paths, &inst.mc_version)? {
            Some(v) => v,
            None => env.meta.pin_version(&inst.mc_version, token).await?,
        };
        // 起動ではインストーラを取得しない（Forge と NeoForge は固定済みのプロファイルだけを使う）
        let version = overlay_loader(env, &inst, vanilla, true, token).await?;
        check_profile(&version)?;
        let index = match load_pinned_asset_index(&env.paths, &version.asset_index.id)? {
            Some(i) => i,
            None => env.meta.pin_asset_index(&version, token).await?,
        };
        let rule_env = RuleEnv::current();
        let game = plan_game(&version, &index, &rule_env, &env.paths, &env.urls.resources)?;
        // インストーラが置くファイルは起動では取得できない。欠けていれば導入し直してもらう
        // （`LoaderFilesMissing` は start_launch で壊れた状態にし、次のプレイで導入する）
        if let Some(missing) = game.generated.iter().find(|p| !p.is_file()) {
            return Err(CoreError::LoaderFilesMissing {
                path: missing.display().to_string(),
            });
        }
        // processor の出力や `maven/` から置いたもの（ゲームのライブラリに出ないものも）は、完了記録の一覧で確かめる
        if is_forge_like(inst.loader) {
            forge::check_installed_files(env, &inst)?;
        }
        let concurrency = usize::from(settings::load(&env.db)?.max_concurrent_downloads);
        fetch_with_phases(
            &env.http,
            game.downloads.clone(),
            Depth::Quick,
            concurrency,
            handle,
        )
        .await?;

        let (component, major) = requirement(&version);
        let chosen = match manual_java {
            Some(path) => Some(path),
            None => find_registered(&env.db, &component)?,
        };
        let java = match chosen {
            Some(path) => path,
            None => {
                handle.set_phase(TaskPhase::Resolving);
                let runtime = cancellable(
                    token,
                    prepare_runtime(
                        &env.http,
                        &env.paths,
                        &env.urls.runtime_index,
                        &component,
                        major,
                    ),
                )
                .await?;
                fetch_with_phases(
                    &env.http,
                    runtime.downloads.clone(),
                    Depth::Full,
                    concurrency,
                    handle,
                )
                .await?;
                handle.set_phase(TaskPhase::Finishing);
                cancellable(
                    token,
                    finish_runtime(&env.db, &runtime, env.probe.as_ref(), now_ms()),
                )
                .await?
            }
        };
        if token.is_cancelled() {
            return Err(CoreError::Cancelled);
        }

        // natives の展開から子プロセスの起動まで
        handle.set_phase(TaskPhase::Launching);
        let natives_dir = env.paths.natives_dir().join(id);
        let natives = game.natives.clone();
        let dest = natives_dir.clone();
        tokio::task::spawn_blocking(move || extract_natives(&natives, &dest))
            .await
            .map_err(|e| CoreError::Unexpected {
                message: e.to_string(),
            })??;

        // 共通設定の適用。結果は記録して通知するだけで、失敗しても起動は止めない
        {
            let db = env.db.clone();
            let paths = env.paths.clone();
            let inst = inst.clone();
            let outcome = match tokio::task::spawn_blocking(move || {
                let ctx = SyncCtx {
                    db: &db,
                    paths: &paths,
                };
                apply_before_launch(&ctx, &inst)
            })
            .await
            {
                Ok(outcome) => outcome,
                Err(e) => ApplyOutcome::Failed(ApplyFailure {
                    reason: FailReason::Unexpected,
                    message: format!("sync apply task failed: {e}"),
                }),
            };
            match &outcome {
                ApplyOutcome::Failed(failure) => {
                    let message = &failure.message;
                    tracing::warn!(instance_id = id, %message, "failed to apply the sync profile");
                }
                outcome => {
                    tracing::info!(instance_id = id, ?outcome, "sync profile apply finished");
                }
            }
            if let Some(event) = SyncApplyFinishedEvent::from_outcome(id, &outcome) {
                self.notifier.sync_apply_finished(&event);
            }
        }

        let game_dir = instance::game_dir(&env.paths, &inst);
        let args = build_args(&ArgInputs {
            version: &version,
            session: &session,
            env: &rule_env,
            game_dir: &game_dir,
            assets_root: &env.paths.assets_dir(),
            natives_dir: &natives_dir,
            library_dir: &env.paths.libraries_dir(),
            classpath: &game.classpath,
            logging_argument: game.logging.as_ref().map(|l| l.argument.as_str()),
            memory_mb: memory_mb(&inst),
            extra_jvm_args: &extra_jvm_args,
            launcher_version: env!("CARGO_PKG_VERSION"),
        });
        let argfile = env.paths.argfile(id);
        let limit = if cfg!(windows) {
            WINDOWS_COMMAND_LIMIT
        } else {
            usize::MAX
        };
        let args = match fit_command(&java, args, major, limit, &argfile)? {
            CommandLine::Direct(args) => args,
            CommandLine::ArgFile { args, jvm } => {
                write_atomic(&argfile, &argfile::encode(&jvm)?)?;
                args
            }
        };

        let launched_at = now_ms();
        self.processes
            .spawn(SpawnSpec {
                instance_id: id.to_string(),
                program: java,
                args,
                cwd: game_dir,
                log_path: env
                    .paths
                    .game_logs_dir()
                    .join(id)
                    .join(format!("{launched_at}.log")),
                // accounts.id と同じ、ハイフンなしの形
                account_id: session.uuid.simple().to_string(),
                launched_at,
                secret: Some(session.access_token.clone()),
            })
            .await?;
        // 起動できたときだけ「最後に遊んだ日時」を更新する（セッションと同じ時刻）。
        // ゲームはもう動いているので、記録に失敗してもタスクは失敗にしない
        if let Err(err) = instance::record_launch(&env.db, id, launched_at) {
            tracing::warn!(
                instance_id = id,
                code = err.code(),
                "failed to record the launch time"
            );
        }
        Ok(())
    }
}
