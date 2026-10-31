//! Modrinth のモッドパック（`.mrpack`）のインポートとエクスポート。

pub mod archive;
pub mod export;
pub mod fetch;
pub mod manifest;
pub mod safe_path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::meta::loader::{LoaderKind, LoaderVersionEntry};

/// 読み込む前の確認に出す、パックの要約。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MrpackInfo {
    pub name: String,
    pub version_id: String,
    pub mc_version: String,
    pub loader: LoaderKind,
    /// 導入処理の一覧での表記（パックの表記と違うことがある）。
    pub loader_version: Option<String>,
    /// クライアントで使うファイルの数。
    pub file_count: u32,
}

/// 書き出す前の確認に出す、内訳。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportPreview {
    /// Modrinth の参照として書くファイルの数。
    pub referenced: u32,
    /// Modrinth で特定できず、実ファイルとして入れるファイルの名前。
    pub unidentified: Vec<String>,
    /// 無効のため書き出さないファイルの数。
    pub disabled: u32,
}

/// 書き出しの設定。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportOptions {
    /// パックの版の文字列。空白だけは断る。
    pub version_id: String,
    /// `config/` を含めるか。
    pub include_config: bool,
    /// 特定できないファイルを実ファイルとして入れることに、利用者が同意したか。
    pub accept_unidentified: bool,
}

/// 導入処理の表記のローダーの版を、`.mrpack` の `dependencies` の表記にする。
/// Forge と NeoForge の `<Minecraft の版>-<ビルド>` は、ビルドだけにする。
pub fn pack_version_for(kind: LoaderKind, mc_version: &str, installer_version: &str) -> String {
    if matches!(kind, LoaderKind::Forge | LoaderKind::Neoforge)
        && let Some(build) = installer_version.strip_prefix(&format!("{mc_version}-"))
    {
        return build.to_string();
    }
    installer_version.to_string()
}

/// パックのローダーの版を、導入処理の一覧にある表記に写す。一覧に無ければ `None`。
/// Fabric と Quilt は同じ表記。Forge と NeoForge は、`<Minecraft の版>-<ビルド>` の形で
/// 持つものがあるため、その形でも探す。
pub fn loader_version_for(
    kind: LoaderKind,
    mc_version: &str,
    pack_version: &str,
    entries: &[LoaderVersionEntry],
) -> Option<String> {
    let prefixed = matches!(kind, LoaderKind::Forge | LoaderKind::Neoforge)
        .then(|| format!("{mc_version}-{pack_version}"));
    entries
        .iter()
        .find(|e| e.version == pack_version || prefixed.as_deref() == Some(e.version.as_str()))
        .map(|e| e.version.clone())
}

/// 読み込みの安全性のための上限。パックは信用しない入力として扱う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// `modrinth.index.json` の大きさ。
    pub manifest_bytes: u64,
    /// ZIP のエントリー数。
    pub entries: usize,
    /// 展開後の合計の大きさ。
    pub total_uncompressed: u64,
    /// 1 エントリーの展開後の大きさ ÷ 圧縮後の大きさ。
    pub ratio: u64,
    /// ダウンロードする 1 ファイルの大きさ。
    pub file_bytes: u64,
}

impl Limits {
    pub const DEFAULT: Limits = Limits {
        manifest_bytes: 5 * 1024 * 1024,
        entries: 50_000,
        total_uncompressed: 4 * 1024 * 1024 * 1024,
        ratio: 200,
        file_bytes: 512 * 1024 * 1024,
    };
}

#[cfg(test)]
mod tests {
    use super::loader_version_for;
    use crate::meta::loader::{LoaderKind, LoaderVersionEntry};

    fn entries(versions: &[&str]) -> Vec<LoaderVersionEntry> {
        versions
            .iter()
            .map(|v| LoaderVersionEntry {
                version: v.to_string(),
                stable: true,
            })
            .collect()
    }

    #[test]
    fn loader_version_for_maps_pack_versions_to_installer_versions() {
        let cases = [
            (
                LoaderKind::Fabric,
                "1.21.1",
                "0.16.10",
                vec!["0.17.0", "0.16.10"],
                Some("0.16.10"),
            ),
            (
                LoaderKind::Forge,
                "1.20.1",
                "47.2.0",
                vec!["1.20.1-47.2.0"],
                Some("1.20.1-47.2.0"),
            ),
            (
                LoaderKind::Forge,
                "1.12.2",
                "14.23.5.2860",
                vec!["1.12.2-14.23.5.2860"],
                Some("1.12.2-14.23.5.2860"),
            ),
            (
                LoaderKind::Neoforge,
                "1.21.1",
                "21.1.77",
                vec!["21.1.77"],
                Some("21.1.77"),
            ),
            (
                LoaderKind::Neoforge,
                "1.20.1",
                "47.1.106",
                vec!["1.20.1-47.1.106"],
                Some("1.20.1-47.1.106"),
            ),
            (
                LoaderKind::Forge,
                "1.20.1",
                "1.20.1-47.2.0",
                vec!["1.20.1-47.2.0"],
                Some("1.20.1-47.2.0"),
            ),
            (LoaderKind::Fabric, "1.21.1", "0.0.1", vec!["0.16.10"], None),
        ];
        for (kind, mc, pack, list, expected) in cases {
            assert_eq!(
                loader_version_for(kind, mc, pack, &entries(&list)).as_deref(),
                expected,
                "{kind:?} {mc} {pack}"
            );
        }
    }
}
