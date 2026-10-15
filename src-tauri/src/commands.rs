//! UI から呼ぶ command。引数を受け取り、core を呼び、結果を返すだけにする。

use nexora_core::app_info::{AppInfo, StartupStatus};
use nexora_core::demo;
use nexora_core::error::{CoreError, ErrorPayload};
use nexora_core::settings::{self, SettingChange, Settings, Theme};
use nexora_core::task::TaskSnapshot;
use tauri::{AppHandle, Manager, State};

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

/// tokio の上で動く必要があるため async にする。
#[tauri::command]
pub async fn start_demo_task(state: State<'_, AppState>, fail_at: Option<u8>) -> CmdResult<String> {
    let services = state.services()?;
    Ok(demo::start_demo(&services.tasks, fail_at))
}
