//! UI から呼ぶ command。引数を受け取り、core を呼び、結果を返すだけにする。

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use nexora_core::app_info::{AppInfo, StartupStatus};
use nexora_core::auth::login::LoginMethod;
use nexora_core::auth::minecraft::SkinVariant;
use nexora_core::auth::service::AccountList;
use nexora_core::content::apply::ContentTaskReport;
use nexora_core::content::identify::IdentifyReport;
use nexora_core::content::modrinth::{SearchPage, SearchQuery};
use nexora_core::content::ops::AddReport;
use nexora_core::content::updates::UpdateCheck;
use nexora_core::content::{ContentItem, ContentKind};
use nexora_core::error::{CoreError, ErrorPayload};
use nexora_core::instance::folder::FolderKind;
use nexora_core::instance::{Instance, InstanceSettings, NewInstance};
use nexora_core::launch::process::RunningSession;
use nexora_core::launcher::{CreatedInstance, InstallPreview};
use nexora_core::meta::loader::{LoaderKind, LoaderVersionEntry};
use nexora_core::meta::mojang::manifest::VersionEntry;
use nexora_core::settings::{self, SettingChange, Settings, Theme};
use nexora_core::task::TaskSnapshot;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

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

/// ローダーが対応するゲームの版。並べ替えず、ローダーのメタデータの順で返す。
#[tauri::command]
pub async fn list_loader_game_versions(
    state: State<'_, AppState>,
    loader: LoaderKind,
) -> CmdResult<Vec<String>> {
    let services = state.services()?;
    services
        .launcher
        .list_loader_game_versions(loader)
        .await
        .map_err(ErrorPayload::from)
}

/// ゲームの版に使えるローダーの版（新しい順）。
#[tauri::command]
pub async fn list_loader_versions(
    state: State<'_, AppState>,
    loader: LoaderKind,
    mc_version: String,
) -> CmdResult<Vec<LoaderVersionEntry>> {
    let services = state.services()?;
    services
        .launcher
        .list_loader_versions(loader, &mc_version)
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
    request: NewInstance,
) -> CmdResult<CreatedInstance> {
    let services = state.services()?;
    services
        .launcher
        .create_instance(&request)
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

/// 中身（mod、リソースパック、シェーダー）の一覧。ディスクを走査してから返す。
#[tauri::command]
pub async fn list_content(
    state: State<'_, AppState>,
    instance_id: String,
    kind: ContentKind,
) -> CmdResult<Vec<ContentItem>> {
    let services = state.services()?;
    services
        .launcher
        .list_content(&instance_id, kind)
        .await
        .map_err(ErrorPayload::from)
}

/// 未特定の中身を Modrinth で特定する。
#[tauri::command]
pub async fn identify_content(
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<IdentifyReport> {
    let services = state.services()?;
    services
        .launcher
        .identify_content(&instance_id)
        .await
        .map_err(ErrorPayload::from)
}

/// 中身を有効または無効にする。起動中なら `instance.busy`。
#[tauri::command]
pub async fn set_content_enabled(
    state: State<'_, AppState>,
    instance_id: String,
    kind: ContentKind,
    rel_path: String,
    enabled: bool,
) -> CmdResult<()> {
    let services = state.services()?;
    services
        .launcher
        .set_content_enabled(&instance_id, kind, &rel_path, enabled)
        .await
        .map_err(ErrorPayload::from)
}

/// 中身を削除する。起動中なら `instance.busy`。
#[tauri::command]
pub async fn remove_content(
    state: State<'_, AppState>,
    instance_id: String,
    kind: ContentKind,
    rel_path: String,
) -> CmdResult<()> {
    let services = state.services()?;
    services
        .launcher
        .remove_content(&instance_id, kind, &rel_path)
        .await
        .map_err(ErrorPayload::from)
}

/// ファイルを中身として追加する。起動中なら `instance.busy`。
#[tauri::command]
pub async fn add_content_files(
    state: State<'_, AppState>,
    instance_id: String,
    kind: ContentKind,
    paths: Vec<String>,
    overwrite: bool,
) -> CmdResult<AddReport> {
    let services = state.services()?;
    let paths = paths.into_iter().map(std::path::PathBuf::from).collect();
    services
        .launcher
        .add_content_files(&instance_id, kind, paths, overwrite)
        .await
        .map_err(ErrorPayload::from)
}

/// Modrinth で MOD を検索する。`instance_id` があれば、そのインスタンスの Minecraft とローダーで絞る。
#[tauri::command]
pub async fn search_modrinth(
    state: State<'_, AppState>,
    instance_id: Option<String>,
    query: SearchQuery,
    offset: u32,
) -> CmdResult<SearchPage> {
    let services = state.services()?;
    services
        .launcher
        .search_modrinth(instance_id.as_deref(), query, offset)
        .await
        .map_err(ErrorPayload::from)
}

/// 導入の前に、入る版、依存の数、非互換の導入済みの MOD を調べる。
#[tauri::command]
pub async fn preview_content_install(
    state: State<'_, AppState>,
    instance_id: String,
    project_id: String,
) -> CmdResult<InstallPreview> {
    let services = state.services()?;
    services
        .launcher
        .preview_content_install(&instance_id, &project_id)
        .await
        .map_err(ErrorPayload::from)
}

/// MOD を導入するタスクを始め、そのタスクの ID を返す。起動中は `instance.busy`。
/// タスクの起動に tokio の実行環境が要るため、`async` にする（同期のコマンドは UI のスレッドで動く）。
#[tauri::command]
pub async fn install_content(
    state: State<'_, AppState>,
    instance_id: String,
    project_id: String,
    accept_incompatible: bool,
) -> CmdResult<String> {
    state
        .services()?
        .launcher
        .install_content(&instance_id, &project_id, accept_incompatible)
        .map_err(ErrorPayload::from)
}

/// MOD の更新を確認する。`force` でキャッシュを使わない。
#[tauri::command]
pub async fn check_content_updates(
    state: State<'_, AppState>,
    instance_id: String,
    force: bool,
) -> CmdResult<UpdateCheck> {
    let services = state.services()?;
    services
        .launcher
        .check_content_updates(&instance_id, force)
        .await
        .map_err(ErrorPayload::from)
}

/// 選んだ MOD を更新するタスクを始め、そのタスクの ID を返す。
/// `install_content` と同じ理由で `async` にする。
#[tauri::command]
pub async fn update_content(
    state: State<'_, AppState>,
    instance_id: String,
    rel_paths: Vec<String>,
) -> CmdResult<String> {
    state
        .services()?
        .launcher
        .update_content(&instance_id, rel_paths)
        .map_err(ErrorPayload::from)
}

/// 導入や更新のタスクの結果。覚えていなければ `null`。
#[tauri::command]
pub fn content_task_report(
    state: State<'_, AppState>,
    task_id: String,
) -> CmdResult<Option<ContentTaskReport>> {
    Ok(state.services()?.launcher.content_task_report(&task_id))
}

/// Modrinth のプロジェクトのページ。`slug` は小文字の英数字、`-`、`_` だけを受け付ける。
fn modrinth_project_url(slug: &str) -> Option<String> {
    let valid = !slug.is_empty()
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    valid.then(|| format!("https://modrinth.com/mod/{slug}"))
}

/// Modrinth のプロジェクトのページを、システムのブラウザで開く。
#[tauri::command]
pub fn open_modrinth_project(app: AppHandle, slug: String) -> CmdResult<()> {
    let url = modrinth_project_url(&slug).ok_or_else(|| {
        CoreError::InvalidInstanceSetting {
            field: "slug".to_string(),
        }
        .to_payload()
    })?;
    app.opener().open_url(url, None::<&str>).map_err(|err| {
        tracing::warn!(%err, "failed to open the Modrinth page");
        CoreError::Unexpected {
            message: "failed to open the browser".to_string(),
        }
        .to_payload()
    })
}

/// Modrinth のカテゴリー名のうち、種類に合うもの。
#[tauri::command]
pub async fn list_content_categories(
    state: State<'_, AppState>,
    kind: ContentKind,
) -> CmdResult<Vec<String>> {
    let services = state.services()?;
    services
        .launcher
        .content_categories(kind)
        .await
        .map_err(ErrorPayload::from)
}

/// 中身のアイコンを `data:image/png;base64,...` で返す。無ければ `None`。
#[tauri::command]
pub async fn get_content_icon(
    state: State<'_, AppState>,
    key: String,
) -> CmdResult<Option<String>> {
    let services = state.services()?;
    blocking(move || {
        Ok(services
            .launcher
            .content_icon(&key)?
            .map(|bytes| format!("data:image/png;base64,{}", STANDARD.encode(bytes))))
    })
    .await
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

/// 動いているゲームを止める。強制終了は外部プロセスを呼ぶことがあるため、別スレッドで動かす。
#[tauri::command]
pub async fn stop_instance(
    state: State<'_, AppState>,
    instance_id: String,
    force: bool,
) -> CmdResult<()> {
    let services = state.services()?;
    blocking(move || services.launcher.stop_instance(&instance_id, force)).await
}

/// インスタンスのフォルダをファイルマネージャで開く。パスは Rust 側で決める。
#[tauri::command]
pub async fn open_instance_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    kind: FolderKind,
) -> CmdResult<()> {
    let services = state.services()?;
    let path = blocking(move || services.launcher.instance_folder(&instance_id, kind)).await?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|err| {
            tracing::warn!(%err, "failed to open the instance folder");
            CoreError::Unexpected {
                message: "failed to open the folder".to_string(),
            }
            .to_payload()
        })
}

#[tauri::command]
pub fn system_memory_mb() -> u32 {
    nexora_core::system::total_memory_mb()
}

#[cfg(test)]
mod tests {
    use super::modrinth_project_url;

    #[test]
    fn project_url_accepts_only_slug_characters() {
        assert_eq!(
            modrinth_project_url("fabric-api").as_deref(),
            Some("https://modrinth.com/mod/fabric-api")
        );
        assert_eq!(
            modrinth_project_url("a_b-1").as_deref(),
            Some("https://modrinth.com/mod/a_b-1")
        );
        for bad in ["", "A", "a/b", "a b", "a?x=1", "a#b", "..", r"a\b", "日本"] {
            assert_eq!(modrinth_project_url(bad), None, "{bad}");
        }
    }
}
