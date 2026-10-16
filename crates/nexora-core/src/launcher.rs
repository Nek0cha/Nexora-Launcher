//! UI からの操作の入口。インストールと起動をタスクとして動かす。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use ts_rs::TS;

use crate::db::Db;
use crate::error::CoreError;
use crate::install::plan::{check_profile, plan_game};
use crate::install::{InstallEnv, Urls, install, kib, now_ms};
use crate::instance::lock::InstanceLocks;
use crate::instance::{self, Instance, InstanceState};
use crate::java::verify::{JavaProbe, SystemProbe};
use crate::java::{find_registered, finish_runtime, prepare_runtime, requirement};
use crate::launch::args::{ArgInputs, CommandLine, WINDOWS_COMMAND_LIMIT, build_args, fit_command};
use crate::launch::natives::extract_natives;
use crate::launch::process::{GameNotifier, GameProcesses, RunningSession, SpawnSpec};
use crate::launch::session::LaunchSession;
use crate::meta::mojang::manifest::{
    VersionEntry, load_pinned_asset_index, load_pinned_version, release_versions,
};
use crate::meta::mojang::rules::RuleEnv;
use crate::meta::write_atomic;
use crate::net::{Depth, Http, fetch_all};
use crate::paths::AppPaths;
use crate::settings;
use crate::task::{TaskHandle, TaskManager};

pub const INSTALL_KIND: &str = "install";
pub const LAUNCH_KIND: &str = "launch";
/// フェーズ 2 のメモリの割り当て（自動設定はフェーズ 9）。
const MEMORY_MB: u32 = 2048;

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
    locks: InstanceLocks,
    /// reattach を一度だけ行うための印。二度目以降は監視を重ねないよう何もしない。
    reattached: Arc<AtomicBool>,
}

fn already_running(instance_id: &str) -> CoreError {
    CoreError::AlreadyRunning {
        instance_id: instance_id.to_string(),
    }
}

impl Launcher {
    pub fn new(
        paths: AppPaths,
        db: Db,
        tasks: TaskManager,
        notifier: Arc<dyn GameNotifier>,
    ) -> Result<Self, CoreError> {
        Ok(Self::with(
            paths,
            db,
            tasks,
            notifier,
            Http::new()?,
            Urls::default(),
            Arc::new(SystemProbe),
        ))
    }

    pub fn with(
        paths: AppPaths,
        db: Db,
        tasks: TaskManager,
        notifier: Arc<dyn GameNotifier>,
        http: Http,
        urls: Urls,
        probe: Arc<dyn JavaProbe>,
    ) -> Self {
        let processes = GameProcesses::new(db.clone(), paths.clone(), notifier);
        Self {
            env: InstallEnv::new(paths, db, http, urls, probe),
            tasks,
            processes,
            locks: InstanceLocks::default(),
            reattached: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn db(&self) -> &Db {
        &self.env.db
    }

    pub async fn list_versions(&self) -> Result<Vec<VersionEntry>, CoreError> {
        release_versions(&self.env.meta.manifest().await?)
    }

    pub fn list_instances(&self) -> Result<Vec<Instance>, CoreError> {
        instance::list(&self.env.db)
    }

    pub fn create_instance(
        &self,
        name: &str,
        mc_version: &str,
    ) -> Result<CreatedInstance, CoreError> {
        let instance = instance::create(&self.env.db, &self.env.paths, name, mc_version, now_ms())?;
        let task_id = self.start_install(&instance.id);
        Ok(CreatedInstance { instance, task_id })
    }

    /// state が ready なら起動のタスク、それ以外ならインストールのタスクの ID を返す。
    /// 同じインスタンスにインストールか起動のタスクが動いていれば、新しく作らずにその ID を返す。
    pub fn launch_instance(&self, instance_id: &str) -> Result<String, CoreError> {
        let inst = instance::get(&self.env.db, instance_id)?;
        if self.processes.is_running(instance_id)? {
            return Err(already_running(instance_id));
        }
        if let Some(active) = self.active_task(instance_id) {
            return Ok(active);
        }
        Ok(match inst.state {
            InstanceState::Ready => self.start_launch(instance_id),
            InstanceState::Installing | InstanceState::Broken => self.start_install(instance_id),
        })
    }

    pub fn list_running(&self) -> Result<Vec<RunningSession>, CoreError> {
        self.processes.list()
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

    /// このインスタンスの、終了していないインストールか起動のタスク。
    fn active_task(&self, instance_id: &str) -> Option<String> {
        self.tasks
            .list()
            .into_iter()
            .find(|t| {
                t.instance_id.as_deref() == Some(instance_id)
                    && (t.kind == INSTALL_KIND || t.kind == LAUNCH_KIND)
                    && !t.state.is_terminal()
            })
            .map(|t| t.id)
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

    fn start_launch(&self, instance_id: &str) -> String {
        let this = self.clone();
        let id = instance_id.to_string();
        self.tasks
            .spawn_unique(LAUNCH_KIND, Some(id.clone()), move |handle| async move {
                let _guard = this.locks.lock(&id).await;
                this.launch(&id, &handle).await
            })
    }

    async fn launch(&self, id: &str, handle: &TaskHandle) -> Result<(), CoreError> {
        if self.processes.is_running(id)? {
            return Err(already_running(id));
        }
        let env = &self.env;
        let token = handle.token();
        let inst = instance::get(&env.db, id)?;
        // 固定済みのメタデータだけで解決する。欠けていればそのときだけ取得する
        let version = match load_pinned_version(&env.paths, &inst.mc_version)? {
            Some(v) => v,
            None => env.meta.pin_version(&inst.mc_version, token).await?,
        };
        check_profile(&version)?;
        let index = match load_pinned_asset_index(&env.paths, &version.asset_index.id)? {
            Some(i) => i,
            None => env.meta.pin_asset_index(&version, token).await?,
        };
        let rule_env = RuleEnv::current();
        let game = plan_game(&version, &index, &rule_env, &env.paths, &env.urls.resources)?;
        let concurrency = usize::from(settings::load(&env.db)?.max_concurrent_downloads);
        fetch_all(
            &env.http,
            game.downloads.clone(),
            Depth::Quick,
            concurrency,
            token,
            |done, total| {
                handle.set_progress(kib(done), kib(total));
            },
        )
        .await?;

        let (component, major) = requirement(&version);
        let java = match find_registered(&env.db, &component)? {
            Some(path) => path,
            None => {
                let runtime = prepare_runtime(
                    &env.http,
                    &env.paths,
                    &env.urls.runtime_index,
                    &component,
                    major,
                )
                .await?;
                fetch_all(
                    &env.http,
                    runtime.downloads.clone(),
                    Depth::Full,
                    concurrency,
                    token,
                    |_, _| {},
                )
                .await?;
                finish_runtime(&env.db, &runtime, env.probe.as_ref(), now_ms()).await?
            }
        };
        if token.is_cancelled() {
            return Err(CoreError::Cancelled);
        }

        let natives_dir = env.paths.natives_dir().join(id);
        let natives = game.natives.clone();
        let dest = natives_dir.clone();
        tokio::task::spawn_blocking(move || extract_natives(&natives, &dest))
            .await
            .map_err(|e| CoreError::Unexpected {
                message: e.to_string(),
            })??;

        let session = current_session(&env.db)?;
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
            memory_mb: MEMORY_MB,
            launcher_version: env!("CARGO_PKG_VERSION"),
        });
        let argfile = env.paths.launch_cache_dir().join(format!("{id}.args"));
        let limit = if cfg!(windows) {
            WINDOWS_COMMAND_LIMIT
        } else {
            usize::MAX
        };
        let args = match fit_command(&java, args, major, limit, &argfile)? {
            CommandLine::Direct(args) => args,
            CommandLine::ArgFile { args, contents } => {
                write_atomic(&argfile, contents.as_bytes())?;
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
                account_id: session.uuid.to_string(),
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

#[cfg(debug_assertions)]
fn current_session(db: &Db) -> Result<LaunchSession, CoreError> {
    let name = crate::launch::session::load_dev_username(db)?;
    Ok(crate::launch::session::offline_session(&name))
}

/// フェーズ 3 で Microsoft 認証のアカウントにつなぐまで、リリースビルドでは起動できない。
#[cfg(not(debug_assertions))]
fn current_session(_db: &Db) -> Result<LaunchSession, CoreError> {
    Err(CoreError::AuthNoAccount)
}
