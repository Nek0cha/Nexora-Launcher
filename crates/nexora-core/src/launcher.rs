//! UI からの操作の入口。インストールと起動をタスクとして動かす。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::auth::login::{LOGIN_KIND, LoginMethod, LoginNotifier, UrlOpener, run_login};
use crate::auth::service::AuthService;
use crate::db::Db;
use crate::error::CoreError;
use crate::install::plan::{check_profile, plan_game};
use crate::install::{InstallEnv, Urls, install, kib, now_ms};
use crate::instance::copy::copy_dir;
use crate::instance::folder::FolderKind;
use crate::instance::lock::InstanceLocks;
use crate::instance::{self, Instance, InstanceState, JavaMode, MemoryMode, NewInstance};
use crate::java::verify::{JavaProbe, SystemProbe};
use crate::java::{find_registered, finish_runtime, prepare_runtime, requirement};
use crate::launch::argfile;
use crate::launch::args::{ArgInputs, CommandLine, WINDOWS_COMMAND_LIMIT, build_args, fit_command};
use crate::launch::jvm_args;
use crate::launch::natives::extract_natives;
use crate::launch::process::{GameNotifier, GameProcesses, RunningSession, SpawnSpec};
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
pub const DUPLICATE_KIND: &str = "duplicate";
/// メモリが自動のときの割り当て（自動設定はフェーズ 9）。
const AUTO_MEMORY_MB: u32 = 2048;

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
    auth: AuthService,
    /// reattach を一度だけ行うための印。二度目以降は監視を重ねないよう何もしない。
    reattached: Arc<AtomicBool>,
}

fn already_running(instance_id: &str) -> CoreError {
    CoreError::AlreadyRunning {
        instance_id: instance_id.to_string(),
    }
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
        let processes = GameProcesses::new(db.clone(), paths.clone(), notifier);
        Self {
            env: InstallEnv::new(paths, db, http, urls, probe),
            tasks,
            processes,
            locks: InstanceLocks::default(),
            auth,
            reattached: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn db(&self) -> &Db {
        &self.env.db
    }

    pub fn auth(&self) -> &AuthService {
        &self.auth
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
        if let Some(active) = self.active_task(instance_id) {
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

    fn start_launch(&self, instance_id: &str, offline: bool) -> String {
        let this = self.clone();
        let id = instance_id.to_string();
        self.tasks
            .spawn_unique(LAUNCH_KIND, Some(id.clone()), move |handle| async move {
                let _guard = this.locks.lock(&id).await;
                this.launch(&id, offline, &handle).await
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
        let chosen = match manual_java {
            Some(path) => Some(path),
            None => find_registered(&env.db, &component)?,
        };
        let java = match chosen {
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
