mod commands;
mod events;
mod logging;
mod material;
mod state;

use std::sync::Arc;

use nexora_core::auth::http::AuthHttp;
use nexora_core::auth::service::AuthService;
use nexora_core::auth::store::{KeyringStore, SecretStore};
use nexora_core::auth::{self, AuthUrls};
use nexora_core::db::Db;
use nexora_core::error::{CoreError, ErrorPayload};
use nexora_core::paths::{AppPaths, Profile};
use nexora_core::settings;
use nexora_core::task::TaskManager;
use tauri::Manager;

use crate::state::{AppState, Services, Startup};

fn start_services(app: &tauri::AppHandle) -> Result<Services, ErrorPayload> {
    let override_dir = if cfg!(debug_assertions) {
        std::env::var_os(nexora_core::paths::DATA_DIR_ENV)
    } else {
        None
    };
    let paths = AppPaths::resolve_with(Profile::current(), override_dir)?;
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
    // 資格情報ストアを使えなければ、ログインをメモリだけに持つ
    let store: Option<Arc<dyn SecretStore>> = match KeyringStore::new(Profile::current()) {
        Ok(store) => Some(Arc::new(store)),
        Err(err) => {
            tracing::warn!(error = %err, "credential store is unavailable; sign-ins last until the launcher closes");
            None
        }
    };
    let auth = AuthService::new(
        db.clone(),
        paths.clone(),
        AuthHttp::new()?,
        AuthUrls::default(),
        store,
        auth::client_id().map(str::to_string),
    )
    .with_on_change({
        let app = app.clone();
        Arc::new(move || events::emit_accounts_changed(&app))
    });
    let launcher = nexora_core::launcher::Launcher::new(
        paths.clone(),
        db.clone(),
        tasks.clone(),
        Arc::new(events::TauriGameNotifier::new(app.clone())),
        auth,
    )?;
    Ok(Services {
        paths,
        db,
        tasks,
        launcher,
    })
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
                    match settings::load(&services.db) {
                        Ok(s) => commands::apply_window_theme(&handle, s.theme),
                        Err(err) => {
                            tracing::warn!(code = %err.code(), "failed to load settings at startup");
                        }
                    }
                    // コマンドを受け付ける前に、途中で止まった中身の操作を同期的に復旧する
                    // （復旧が終わるまで走査で DB を更新させない。spec 0 の 3.7 節）
                    if let Err(err) = services.launcher.recover_operations() {
                        tracing::warn!(code = err.code(), "failed to recover content operations");
                    }
                    // コマンドを受け付ける前に、実行中のゲームを一度だけ引き継ぐ。
                    // tokio の実行環境の中で動かすため block_on で待つ。
                    let launcher = services.launcher.clone();
                    tauri::async_runtime::block_on(async move {
                        if let Err(err) = launcher.reattach() {
                            tracing::warn!(code = err.code(), "failed to reattach running games");
                        }
                    });
                    // 使用中のアカウントのセッションを裏で用意する
                    let auth = services.launcher.auth().clone();
                    tauri::async_runtime::spawn(async move { auth.warm_up().await });
                    state.set(Startup::Ready(Box::new(services)));
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
            commands::list_versions,
            commands::list_loader_game_versions,
            commands::list_loader_versions,
            commands::list_instances,
            commands::create_instance,
            commands::launch_instance,
            commands::list_running,
            commands::get_dev_offline_username,
            commands::set_dev_offline_username,
            commands::list_accounts,
            commands::set_active_account,
            commands::set_dev_offline_active,
            commands::remove_account,
            commands::begin_login,
            commands::get_skin,
            commands::update_instance,
            commands::delete_instance,
            commands::list_content,
            commands::identify_content,
            commands::set_content_enabled,
            commands::remove_content,
            commands::add_content_files,
            commands::list_content_categories,
            commands::search_modrinth,
            commands::preview_content_install,
            commands::install_content,
            commands::check_content_updates,
            commands::update_content,
            commands::list_content_versions,
            commands::change_content_version,
            commands::content_task_report,
            commands::list_sync_profiles,
            commands::create_sync_profile_from_instance,
            commands::rename_sync_profile,
            commands::delete_sync_profile,
            commands::set_sync_categories,
            commands::preview_join_sync_profile,
            commands::join_sync_profile,
            commands::leave_sync_profile,
            commands::get_instance_sync,
            commands::set_sync_excluded,
            commands::list_sync_fields,
            commands::get_sync_profile_values,
            commands::set_sync_override,
            commands::pending_sync_proposals,
            commands::resolve_sync_proposal,
            commands::open_modrinth_project,
            commands::get_content_icon,
            commands::duplicate_instance,
            commands::get_current_skin,
            commands::set_cape,
            commands::list_saved_skins,
            commands::get_saved_skin_image,
            commands::add_saved_skin,
            commands::add_saved_skin_from_path,
            commands::update_saved_skin,
            commands::delete_saved_skin,
            commands::apply_saved_skin,
            commands::list_default_skins,
            commands::apply_default_skin,
            commands::import_current_skin,
            commands::stop_instance,
            commands::open_instance_folder,
            commands::list_screenshots,
            commands::delete_screenshot,
            commands::import_screenshots,
            commands::list_game_logs,
            commands::read_game_log,
            commands::system_memory_mb,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Nexora");
}
