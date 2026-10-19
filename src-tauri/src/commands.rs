//! UI から呼ぶ command。引数を受け取り、core を呼び、結果を返すだけにする。

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use nexora_core::app_info::{AppInfo, StartupStatus};
use nexora_core::auth::login::LoginMethod;
use nexora_core::auth::minecraft::SkinVariant;
use nexora_core::auth::service::AccountList;
use nexora_core::error::{CoreError, ErrorPayload};
use nexora_core::instance::{Instance, InstanceSettings};
use nexora_core::launch::process::RunningSession;
use nexora_core::launcher::CreatedInstance;
use nexora_core::meta::mojang::manifest::VersionEntry;
use nexora_core::settings::{self, SettingChange, Settings, Theme};
use nexora_core::task::TaskSnapshot;
use tauri::{AppHandle, Manager, State};

use crate::events::{TauriLoginNotifier, TauriUrlOpener};
use crate::state::AppState;

type CmdResult<T> = Result<T, ErrorPayload>;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// DB などの同期処理を、非同期の command からブロックせずに実行する。
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, CoreError> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| {
            CoreError::Unexpected {
                message: e.to_string(),
            }
            .to_payload()
        })?
        .map_err(ErrorPayload::from)
}

pub fn apply_window_theme(app: &AppHandle, theme: Theme) {
    if let Some(window) = app.get_webview_window("main") {
        let native = match theme {
            Theme::System => None,
            Theme::Dark => Some(tauri::Theme::Dark),
            Theme::Light => Some(tauri::Theme::Light),
        };
        if let Err(err) = window.set_theme(native) {
            tracing::warn!(%err, "failed to set window theme");
        }
    }
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> CmdResult<AppInfo> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        build: if cfg!(debug_assertions) {
            "dev"
        } else {
            "release"
        }
        .to_string(),
        os: std::env::consts::OS.to_string(),
        data_dir: state.data_dir().unwrap_or_default(),
        material_available: crate::material::material_available(),
    })
}

#[tauri::command]
pub fn get_startup_status(state: State<'_, AppState>) -> CmdResult<StartupStatus> {
    Ok(state.status())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    let services = state.services()?;
    blocking(move || settings::load(&services.db)).await
}

#[tauri::command]
pub async fn set_setting(
    app: AppHandle,
    state: State<'_, AppState>,
    change: SettingChange,
) -> CmdResult<Settings> {
    let services = state.services()?;
    let updated = blocking(move || settings::apply(&services.db, change, now_ms())).await?;
    apply_window_theme(&app, updated.theme);
    Ok(updated)
}

#[tauri::command]
pub fn list_tasks(state: State<'_, AppState>) -> CmdResult<Vec<TaskSnapshot>> {
    Ok(state.services()?.tasks.list())
}

#[tauri::command]
pub fn get_task(state: State<'_, AppState>, task_id: String) -> CmdResult<TaskSnapshot> {
    state
        .services()?
        .tasks
        .get(&task_id)
        .map_err(ErrorPayload::from)
}

#[tauri::command]
pub fn cancel_task(state: State<'_, AppState>, task_id: String) -> CmdResult<()> {
    state
        .services()?
        .tasks
        .cancel(&task_id)
        .map_err(ErrorPayload::from)
}

#[tauri::command]
pub async fn list_versions(state: State<'_, AppState>) -> CmdResult<Vec<VersionEntry>> {
    let services = state.services()?;
    services
        .launcher
        .list_versions()
        .await
        .map_err(ErrorPayload::from)
}

#[tauri::command]
pub async fn list_instances(state: State<'_, AppState>) -> CmdResult<Vec<Instance>> {
    let services = state.services()?;
    blocking(move || services.launcher.list_instances()).await
}

/// タスクを tokio の上で始めるため async にする。DB の処理は短い。
#[tauri::command]
pub async fn create_instance(
    state: State<'_, AppState>,
    name: String,
    mc_version: String,
) -> CmdResult<CreatedInstance> {
    let services = state.services()?;
    services
        .launcher
        .create_instance(&name, &mc_version)
        .map_err(ErrorPayload::from)
}

/// 設定を更新する。ゲームの起動中でも更新でき、反映は次の起動から。
#[tauri::command]
pub async fn update_instance(
    state: State<'_, AppState>,
    instance_id: String,
    settings: InstanceSettings,
) -> CmdResult<Instance> {
    let services = state.services()?;
    services
        .launcher
        .update_instance(&instance_id, &settings)
        .await
        .map_err(ErrorPayload::from)
}

/// インスタンスを削除する。起動中かインストール中なら `instance.busy`。
#[tauri::command]
pub async fn delete_instance(state: State<'_, AppState>, instance_id: String) -> CmdResult<()> {
    let services = state.services()?;
    services
        .launcher
        .delete_instance(&instance_id)
        .await
        .map_err(ErrorPayload::from)
}

/// 複製のタスクを始め、その ID を返す。
#[tauri::command]
pub async fn duplicate_instance(
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<String> {
    let services = state.services()?;
    services
        .launcher
        .duplicate_instance(&instance_id)
        .map_err(ErrorPayload::from)
}

#[tauri::command]
pub async fn launch_instance(
    state: State<'_, AppState>,
    instance_id: String,
    offline: bool,
) -> CmdResult<String> {
    let services = state.services()?;
    services
        .launcher
        .launch_instance(&instance_id, offline)
        .map_err(ErrorPayload::from)
}

#[tauri::command]
pub async fn list_running(state: State<'_, AppState>) -> CmdResult<Vec<RunningSession>> {
    let services = state.services()?;
    blocking(move || services.launcher.list_running()).await
}

/// UI からのアカウント ID（ハイフンの有無は問わない）。読めなければ見つからないものとして扱う。
fn parse_account_id(raw: &str) -> Result<uuid::Uuid, ErrorPayload> {
    uuid::Uuid::parse_str(raw).map_err(|_| {
        CoreError::AccountNotFound {
            account_id: raw.chars().take(64).collect(),
        }
        .to_payload()
    })
}

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> CmdResult<AccountList> {
    let services = state.services()?;
    blocking(move || services.launcher.auth().list()).await
}

#[tauri::command]
pub async fn set_active_account(state: State<'_, AppState>, account_id: String) -> CmdResult<()> {
    let services = state.services()?;
    let id = parse_account_id(&account_id)?;
    blocking(move || services.launcher.auth().set_active(id)).await
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn set_dev_offline_active(state: State<'_, AppState>, active: bool) -> CmdResult<()> {
    let services = state.services()?;
    let db = services.db.clone();
    blocking(move || nexora_core::launch::session::set_dev_offline_active(&db, active, now_ms()))
        .await?;
    services.launcher.auth().notify_changed();
    Ok(())
}

#[cfg(not(debug_assertions))]
#[tauri::command]
pub fn set_dev_offline_active(_active: bool) -> CmdResult<()> {
    Err(ErrorPayload::new("app.dev_only"))
}

#[tauri::command]
pub async fn remove_account(state: State<'_, AppState>, account_id: String) -> CmdResult<()> {
    let services = state.services()?;
    let id = parse_account_id(&account_id)?;
    services
        .launcher
        .remove_account(id)
        .await
        .map_err(ErrorPayload::from)
}

/// ログインのタスクを始め、その ID を返す。進捗は `login://progress` で届く。
#[tauri::command]
pub async fn begin_login(
    app: AppHandle,
    state: State<'_, AppState>,
    method: LoginMethod,
) -> CmdResult<String> {
    let services = state.services()?;
    Ok(services.launcher.begin_login(
        method,
        Arc::new(TauriUrlOpener::new(app.clone())),
        Arc::new(TauriLoginNotifier::new(app)),
    ))
}

/// スキンの画像全体を `data:image/png;base64,...` で返す。顔の切り出しは UI が行う。
#[tauri::command]
pub async fn get_skin(state: State<'_, AppState>, account_id: String) -> CmdResult<Option<String>> {
    let services = state.services()?;
    let id = parse_account_id(&account_id)?;
    let Some(path) = services
        .launcher
        .auth()
        .skin_path(id)
        .await
        .map_err(ErrorPayload::from)?
    else {
        return Ok(None);
    };
    blocking(move || {
        let bytes = std::fs::read(&path).map_err(|source| CoreError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Ok(Some(format!(
            "data:image/png;base64,{}",
            STANDARD.encode(bytes)
        )))
    })
    .await
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_dev_offline_username(state: State<'_, AppState>) -> CmdResult<String> {
    let services = state.services()?;
    blocking(move || nexora_core::launch::session::load_dev_username(&services.db)).await
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn set_dev_offline_username(
    state: State<'_, AppState>,
    name: String,
) -> CmdResult<String> {
    let services = state.services()?;
    blocking(move || nexora_core::launch::session::save_dev_username(&services.db, &name, now_ms()))
        .await
}

#[cfg(not(debug_assertions))]
#[tauri::command]
pub fn get_dev_offline_username() -> CmdResult<String> {
    Err(ErrorPayload::new("app.dev_only"))
}

#[cfg(not(debug_assertions))]
#[tauri::command]
pub fn set_dev_offline_username(_name: String) -> CmdResult<String> {
    Err(ErrorPayload::new("app.dev_only"))
}

/// アカウントの現在のスキンの型（classic か slim）を返す。
#[tauri::command]
pub async fn get_skin_variant(
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<SkinVariant> {
    let services = state.services()?;
    let id = parse_account_id(&account_id)?;
    services
        .launcher
        .auth()
        .skin_variant(id)
        .await
        .map_err(ErrorPayload::from)
}

/// スキンを変更する。`png` は IPC の `number[]` で受ける。
#[tauri::command]
pub async fn change_skin(
    state: State<'_, AppState>,
    account_id: String,
    variant: SkinVariant,
    png: Vec<u8>,
) -> CmdResult<()> {
    let services = state.services()?;
    let id = parse_account_id(&account_id)?;
    services
        .launcher
        .auth()
        .change_skin(id, variant, png)
        .await
        .map_err(ErrorPayload::from)
}
