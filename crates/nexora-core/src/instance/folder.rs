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
    /// ゲームのログ。ゲームのディレクトリではなく、ランチャーのデータの下（logs/game/<id>）にある。
    Logs,
}

impl FolderKind {
    /// ゲームのディレクトリの下の名前。`Game` はディレクトリそのもの。
    /// `Logs` はゲームのディレクトリの下に無いので None（場所は `Launcher::instance_folder` で決める）。
    pub fn subdir(self) -> Option<&'static str> {
        match self {
            FolderKind::Game | FolderKind::Logs => None,
            FolderKind::Mods => Some("mods"),
            FolderKind::Screenshots => Some("screenshots"),
            FolderKind::Saves => Some("saves"),
        }
    }
}
