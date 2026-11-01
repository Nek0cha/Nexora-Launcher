//! 他のランチャーのインスタンスを、選んだ中身だけコピーして取り込む。

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
}

impl ImportKind {
    pub const ALL: [ImportKind; 3] = [ImportKind::Mods, ImportKind::Screenshots, ImportKind::Saves];

    /// インスタンスのフォルダの中での名前。
    pub fn dir_name(self) -> &'static str {
        match self {
            ImportKind::Mods => "mods",
            ImportKind::Screenshots => "screenshots",
            ImportKind::Saves => "saves",
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

/// 取り込むインスタンスと、その中で取り込む種類。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportRequest {
    pub id: String,
    pub kinds: Vec<ImportKind>,
}

/// 読み込み元。ランチャーごとに 1 つ実装する。元のデータは読むだけ。
pub trait ImportSource {
    /// 見つかったインスタンスの一覧。
    fn list(&self) -> Result<Vec<SourceInstance>, CoreError>;
    /// 指定の種類のコピー元のフォルダ。無ければ `None`。
    fn folder(&self, id: &str, kind: ImportKind) -> Result<Option<PathBuf>, CoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;

    #[test]
    fn kinds_map_to_the_folder_names_of_the_instance() {
        assert_eq!(ImportKind::Mods.dir_name(), "mods");
        assert_eq!(ImportKind::Screenshots.dir_name(), "screenshots");
        assert_eq!(ImportKind::Saves.dir_name(), "saves");
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
