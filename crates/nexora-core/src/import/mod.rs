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

/// 取り込めない理由の種類。文言は UI が `launcherImport.unsupported.<code>` から組み立てる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UnsupportedCode {
    /// フォルダ名やキーが安全でない。
    UnsafeName,
    /// ゲームのフォルダの指定が使えない（絶対パスでない、`..` を含むなど）。
    UnsafeFolder,
    /// フォルダがリンクで、たどらない。
    LinkedFolder,
    /// Nexora が扱わないローダー。
    UnknownLoader,
    /// Minecraft の版が分からない。
    MissingVersion,
    /// ローダーの版が分からない。
    MissingLoaderVersion,
    /// ローダーの版が導入処理に無い。
    LoaderUnavailable,
    /// 元のランチャーでの導入が終わっていない。
    InstallUnfinished,
    /// 必要なファイルが無い。
    MissingMetadata,
    /// 必要なファイルを読めない。
    UnreadableMetadata,
    /// ローダーが複数入っている。
    MultipleLoaders,
    /// 最新版に追従していて、版が決まらない。
    LatestVersion,
    /// 対応していない版。
    UnsupportedVersion,
}

/// 取り込めない理由。`detail` は、文言に差し込む値（ローダー名、版、フォルダ名など）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Unsupported {
    pub code: UnsupportedCode,
    pub detail: Option<String>,
}

impl Unsupported {
    pub fn new(code: UnsupportedCode) -> Self {
        Self { code, detail: None }
    }

    pub fn with(code: UnsupportedCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: Some(detail.into()),
        }
    }
}

/// ログやエラーに出す英語の文。画面には使わない。
impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let detail = self.detail.as_deref().unwrap_or("");
        match self.code {
            UnsupportedCode::UnsafeName => write!(f, "{detail} is not a safe name"),
            UnsupportedCode::UnsafeFolder => write!(f, "the folder {detail} cannot be used"),
            UnsupportedCode::LinkedFolder => write!(f, "{detail} is a link"),
            UnsupportedCode::UnknownLoader => write!(f, "the loader {detail} is not supported"),
            UnsupportedCode::MissingVersion => write!(f, "the Minecraft version is missing"),
            UnsupportedCode::MissingLoaderVersion => write!(f, "the loader version is missing"),
            UnsupportedCode::LoaderUnavailable => write!(f, "{detail} is not available"),
            UnsupportedCode::InstallUnfinished => write!(f, "the installation is not finished"),
            UnsupportedCode::MissingMetadata => write!(f, "{detail} was not found"),
            UnsupportedCode::UnreadableMetadata => write!(f, "{detail} could not be read"),
            UnsupportedCode::MultipleLoaders => write!(f, "more than one loader is installed"),
            UnsupportedCode::LatestVersion => write!(f, "the profile follows the latest version"),
            UnsupportedCode::UnsupportedVersion => {
                write!(f, "the version {detail} is not supported")
            }
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
    pub unsupported: Option<Unsupported>,
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
    fn an_unsupported_reason_serializes_as_a_code_and_a_detail() {
        let json = serde_json::to_value(Unsupported::with(
            UnsupportedCode::UnknownLoader,
            "liteloader",
        ))
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "code": "unknown_loader", "detail": "liteloader" })
        );
        assert_eq!(
            Unsupported::new(UnsupportedCode::MissingVersion).to_string(),
            "the Minecraft version is missing"
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
