//! インスタンスのフォルダの種類。UI から「開く」ときに使う。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FolderKind {
    Game,
    Mods,
    Screenshots,
    Saves,
}

impl FolderKind {
    /// ゲームのディレクトリの下の名前。`Game` はディレクトリそのもの。
    pub fn subdir(self) -> Option<&'static str> {
        match self {
            FolderKind::Game => None,
            FolderKind::Mods => Some("mods"),
            FolderKind::Screenshots => Some("screenshots"),
            FolderKind::Saves => Some("saves"),
        }
    }
}
