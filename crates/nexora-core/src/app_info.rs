//! UI に渡すアプリの情報。値は Tauri 側で埋める。

use serde::Serialize;
use ts_rs::TS;

use crate::error::ErrorPayload;

/// 起動の状態。setup の処理とウィンドウの読み込みの順序に依存しないよう、
/// UI は starting の間これを繰り返し取得する。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum StartupStatus {
    Starting,
    Ready,
    Failed { error: ErrorPayload },
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
    /// "release" または "dev"
    pub build: String,
    pub os: String,
    pub data_dir: String,
    pub material_available: bool,
}
