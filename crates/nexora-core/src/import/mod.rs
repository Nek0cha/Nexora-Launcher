//! 他のランチャーのインスタンスを、選んだ中身だけコピーして取り込む。

pub mod copy;
pub mod curseforge;
pub mod duplicates;
pub mod modrinth_app;
pub mod official;
pub mod prism;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::CoreError;
use crate::meta::loader::LoaderKind;

/// 取り込める中身の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ImportKind {
    Mods,
    Screenshots,
    Saves,
    /// ゲームと MOD の設定（`config/`、`options.txt`、`servers.dat`）。
    Settings,
}

impl ImportKind {
    pub const ALL: [ImportKind; 4] = [
        ImportKind::Mods,
        ImportKind::Screenshots,
        ImportKind::Saves,
        ImportKind::Settings,
    ];

    /// インスタンスのフォルダの中での、フォルダまたはファイルの名前。種類によっては複数になる。
    pub fn parts(self) -> &'static [&'static str] {
        match self {
            ImportKind::Mods => &["mods"],
            ImportKind::Screenshots => &["screenshots"],
            ImportKind::Saves => &["saves"],
            ImportKind::Settings => &["config", "options.txt", "servers.dat"],
        }
    }
}

/// 読み込み元で見つかった 1 つのインスタンス。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceInstance {
    /// その読み込み元の中で一意。
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    /// 実際に中身がある種類。
    pub available: Vec<ImportKind>,
    /// 取り込めない理由。あれば行を選べない。
    pub unsupported: Option<String>,
}

/// 読み込み元のランチャー。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ImportSourceKind {
    ModrinthApp,
    PrismLauncher,
    OfficialLauncher,
    CurseForge,
}

/// 取り込むインスタンスと、その中で取り込む種類。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportRequest {
    pub id: String,
    pub kinds: Vec<ImportKind>,
}

/// 読み込み元。ランチャーごとに 1 つ実装する。元のデータは読むだけ。
pub trait ImportSource: Send + Sync {
    /// 見つかったインスタンスの一覧。
    fn list(&self) -> Result<Vec<SourceInstance>, CoreError>;
    /// 指定の部分（`ImportKind::parts` の 1 つ）のコピー元。フォルダまたはファイル。無ければ `None`。
    fn source_path(&self, id: &str, part: &str) -> Result<Option<PathBuf>, CoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;

    #[test]
    fn kinds_map_to_their_places_in_the_instance() {
        assert_eq!(ImportKind::Mods.parts(), ["mods"]);
        assert_eq!(ImportKind::Screenshots.parts(), ["screenshots"]);
        assert_eq!(ImportKind::Saves.parts(), ["saves"]);
        assert_eq!(
            ImportKind::Settings.parts(),
            ["config", "options.txt", "servers.dat"]
        );
        assert_eq!(ImportKind::ALL.len(), 4);
    }

    #[test]
    fn source_kinds_use_snake_case_names() {
        let names: Vec<String> = [
            ImportSourceKind::ModrinthApp,
            ImportSourceKind::PrismLauncher,
            ImportSourceKind::OfficialLauncher,
            ImportSourceKind::CurseForge,
        ]
        .iter()
        .map(|k| serde_json::to_string(k).unwrap())
        .collect();
        assert_eq!(
            names,
            [
                "\"modrinth_app\"",
                "\"prism_launcher\"",
                "\"official_launcher\"",
                "\"curse_forge\""
            ]
        );
    }

    #[test]
    fn import_errors_have_their_own_codes() {
        let unavailable = CoreError::ImportUnavailable { reason: "r".into() };
        assert_eq!(unavailable.code(), "import.unavailable");
        assert_eq!(
            unavailable.to_payload().params.get("reason"),
            Some(&crate::error::ParamValue::Str("r".into()))
        );
        assert_eq!(CoreError::ImportDuplicate.code(), "import.duplicate");
    }
}
