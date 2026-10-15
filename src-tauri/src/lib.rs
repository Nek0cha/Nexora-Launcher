mod commands;
mod events;
mod logging;
mod material;
mod state;

use std::sync::Arc;

use nexora_core::db::Db;
use nexora_core::error::{CoreError, ErrorPayload};
use nexora_core::paths::{AppPaths, Profile};
use nexora_core::settings;
use nexora_core::task::TaskManager;
use tauri::Manager;

use crate::state::{AppState, Services, Startup};

fn start_services(app: &tauri::AppHandle) -> Result<Services, ErrorPayload> {
    let paths = AppPaths::resolve(Profile::current())?;
    paths.ensure_dirs().map_err(|source| {
        CoreError::Io {
            path: paths.root().display().to_string(),
            source,
        }
        .to_payload()
    })?;

    match logging::init(&paths.logs_dir()) {
        Ok(guard) => {
            app.manage(guard);
        }
        Err(err) => eprintln!("failed to initialise logging: {err}"),
    }
    tracing::info!(data_dir = %paths.root().display(), "starting Nexora");

    let db = Db::open(&paths.db_file())?;
    let tasks = TaskManager::new(Arc::new(events::TauriNotifier::new(app.clone())));
    Ok(Services { paths, db, tasks })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // single-instance は最初に登録する
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<AppState>();
            match start_services(&handle) {
                Ok(services) => {
                    if let Ok(s) = settings::load(&services.db) {
                        commands::apply_window_theme(&handle, s.theme);
                    }
                    state.set(Startup::Ready(services));
                }
                Err(err) => {
                    eprintln!("startup failed: {}", err.code);
                    tracing::error!(code = %err.code, "startup failed");
                    state.set(Startup::Failed(err));
                }
            }
            if cfg!(debug_assertions)
                && let Some(window) = app.get_webview_window("main")
            {
                let _ = window.set_title("Nexora Dev");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_startup_status,
            commands::get_settings,
            commands::set_setting,
            commands::list_tasks,
            commands::get_task,
            commands::cancel_task,
            commands::start_demo_task,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Nexora");
}
