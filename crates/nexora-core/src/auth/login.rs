//! ログインのタスク。

use serde::Serialize;
use ts_rs::TS;

/// ログインの進み具合。UI のダイアログの表示に使う。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "stage", rename_all = "camelCase")]
#[ts(export)]
pub enum LoginProgress {
    WaitingForBrowser {
        url: String,
    },
    DeviceCode {
        user_code: String,
        verification_uri: String,
    },
    CheckingXbox,
    CheckingMinecraft,
}
