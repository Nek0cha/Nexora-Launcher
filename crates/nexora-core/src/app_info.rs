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
    /// アプリの中から自己更新できるか（`self_update_supported`）
    pub self_update: bool,
}

/// アプリの中から自己更新できるか。Linux では AppImage だけが置き換えられる
/// （deb などのパッケージは更新を適用できない）。
pub fn self_update_supported(os: &str, appimage: bool) -> bool {
    os != "linux" || appimage
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_update_supported_by_os() {
        assert!(!self_update_supported("linux", false));
        assert!(self_update_supported("linux", true));
        assert!(self_update_supported("windows", false));
        assert!(self_update_supported("macos", false));
    }
}
