//! 起動の状態と、起動後に使うサービス。

use std::sync::RwLock;

use nexora_core::db::Db;
use nexora_core::error::ErrorPayload;
use nexora_core::paths::AppPaths;
use nexora_core::task::TaskManager;

#[derive(Clone)]
pub struct Services {
    pub paths: AppPaths,
    pub db: Db,
    pub tasks: TaskManager,
    pub launcher: nexora_core::launcher::Launcher,
}

pub enum Startup {
    Starting,
    Ready(Box<Services>),
    Failed(ErrorPayload),
}

pub struct AppState {
    startup: RwLock<Startup>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            startup: RwLock::new(Startup::Starting),
        }
    }

    pub fn set(&self, startup: Startup) {
        *self.startup.write().unwrap() = startup;
    }

    /// 起動が終わっていればサービスを返す。そうでなければ app.not_ready のエラー。
    pub fn services(&self) -> Result<Services, ErrorPayload> {
        match &*self.startup.read().unwrap() {
            Startup::Ready(services) => Ok(Services::clone(services)),
            _ => Err(ErrorPayload::new("app.not_ready")),
        }
    }

    pub fn status(&self) -> nexora_core::app_info::StartupStatus {
        use nexora_core::app_info::StartupStatus;
        match &*self.startup.read().unwrap() {
            Startup::Starting => StartupStatus::Starting,
            Startup::Ready(_) => StartupStatus::Ready,
            Startup::Failed(err) => StartupStatus::Failed { error: err.clone() },
        }
    }

    pub fn data_dir(&self) -> Option<String> {
        match &*self.startup.read().unwrap() {
            Startup::Ready(s) => Some(s.paths.root().display().to_string()),
            _ => None,
        }
    }
}
