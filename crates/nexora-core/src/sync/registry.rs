//! 共通設定の登録表。ここでは種別（カテゴリ）だけを定義する。

use serde::{Deserialize, Serialize};

/// 共通設定の項目をまとめる種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Keys,
    Mouse,
    Sound,
    Graphics,
    Chat,
    Accessibility,
}
