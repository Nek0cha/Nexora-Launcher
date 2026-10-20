//! ローダー（Fabric、Quilt など）の種類とメタデータ。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// インスタンスのローダー。DB には小文字の名前（`vanilla`、`fabric` など）で保存する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum LoaderKind {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    Neoforge,
}

impl LoaderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LoaderKind::Vanilla => "vanilla",
            LoaderKind::Fabric => "fabric",
            LoaderKind::Quilt => "quilt",
            LoaderKind::Forge => "forge",
            LoaderKind::Neoforge => "neoforge",
        }
    }

    /// 小文字の名前から読む。大文字を含むものや未知の名前は `None`。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "vanilla" => Some(LoaderKind::Vanilla),
            "fabric" => Some(LoaderKind::Fabric),
            "quilt" => Some(LoaderKind::Quilt),
            "forge" => Some(LoaderKind::Forge),
            "neoforge" => Some(LoaderKind::Neoforge),
            _ => None,
        }
    }

    /// いまインストールして起動できるローダーか。
    pub fn is_supported(self) -> bool {
        matches!(
            self,
            LoaderKind::Vanilla | LoaderKind::Fabric | LoaderKind::Quilt
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [LoaderKind; 5] = [
        LoaderKind::Vanilla,
        LoaderKind::Fabric,
        LoaderKind::Quilt,
        LoaderKind::Forge,
        LoaderKind::Neoforge,
    ];

    #[test]
    fn parse_and_as_str_round_trip() {
        for kind in ALL {
            assert_eq!(LoaderKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(LoaderKind::parse("neoforge"), Some(LoaderKind::Neoforge));
    }

    #[test]
    fn parse_is_case_sensitive_and_rejects_unknown() {
        assert_eq!(LoaderKind::parse("Fabric"), None);
        assert_eq!(LoaderKind::parse(""), None);
        assert_eq!(LoaderKind::parse("liteloader"), None);
    }

    #[test]
    fn only_vanilla_fabric_and_quilt_are_supported() {
        let supported: Vec<_> = ALL.into_iter().filter(|k| k.is_supported()).collect();
        assert_eq!(
            supported,
            vec![LoaderKind::Vanilla, LoaderKind::Fabric, LoaderKind::Quilt]
        );
    }

    #[test]
    fn serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&LoaderKind::Neoforge).unwrap(),
            "\"neoforge\""
        );
        let back: LoaderKind = serde_json::from_str("\"quilt\"").unwrap();
        assert_eq!(back, LoaderKind::Quilt);
    }
}
