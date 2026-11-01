//! 他のランチャーのインスタンスを、選んだ中身だけコピーして取り込む。

pub mod copy;
pub mod duplicates;
pub mod modrinth_app;

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
    /// MOD の設定（`config/`）。
    Config,
    Screenshots,
    Saves,
    /// ゲームの設定（`options.txt`）。
    Options,
    /// サーバー一覧（`servers.dat`）。
    Servers,
}

impl ImportKind {
    pub const ALL: [ImportKind; 6] = [
        ImportKind::Mods,
        ImportKind::Config,
        ImportKind::Screenshots,
        ImportKind::Saves,
        ImportKind::Options,
        ImportKind::Servers,
    ];

    /// インスタンスのフォルダの中での、フォルダまたはファイルの名前。
    pub fn rel_path(self) -> &'static str {
        match self {
            ImportKind::Mods => "mods",
            ImportKind::Config => "config",
            ImportKind::Screenshots => "screenshots",
            ImportKind::Saves => "saves",
            ImportKind::Options => "options.txt",
            ImportKind::Servers => "servers.dat",
        }
    }

    /// フォルダではなく、1 つのファイルか。
    pub fn is_file(self) -> bool {
        matches!(self, ImportKind::Options | ImportKind::Servers)
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
    /// 指定の種類のコピー元（フォルダまたはファイル）。無ければ `None`。
    fn source_path(&self, id: &str, kind: ImportKind) -> Result<Option<PathBuf>, CoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;

    #[test]
    fn kinds_map_to_their_place_in_the_instance() {
        for (kind, rel, file) in [
            (ImportKind::Mods, "mods", false),
            (ImportKind::Config, "config", false),
            (ImportKind::Screenshots, "screenshots", false),
            (ImportKind::Saves, "saves", false),
            (ImportKind::Options, "options.txt", true),
            (ImportKind::Servers, "servers.dat", true),
        ] {
            assert_eq!((kind.rel_path(), kind.is_file()), (rel, file));
        }
        assert_eq!(ImportKind::ALL.len(), 6);
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
