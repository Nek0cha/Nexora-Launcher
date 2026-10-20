//! インスタンスの中身（mod、リソースパック、シェーダー）の型。

// 走査や操作の側から使われるまでは未使用になる。
#[allow(dead_code)]
pub(crate) mod store;

pub mod local_meta;
pub mod modrinth;
pub mod ops;
pub mod scan;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 中身の種類。DB と JSON には `mod`、`resourcepack`、`shader` で持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ContentKind {
    Mod,
    ResourcePack,
    Shader,
}

impl ContentKind {
    /// インスタンスの .minecraft 直下のフォルダー名。
    pub fn dir(self) -> &'static str {
        match self {
            ContentKind::Mod => "mods",
            ContentKind::ResourcePack => "resourcepacks",
            ContentKind::Shader => "shaderpacks",
        }
    }

    /// 既定の拡張子（ドットなし）。
    pub fn ext(self) -> &'static str {
        match self {
            ContentKind::Mod => "jar",
            ContentKind::ResourcePack | ContentKind::Shader => "zip",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ContentKind::Mod => "mod",
            ContentKind::ResourcePack => "resourcepack",
            ContentKind::Shader => "shader",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mod" => Some(ContentKind::Mod),
            "resourcepack" => Some(ContentKind::ResourcePack),
            "shader" => Some(ContentKind::Shader),
            _ => None,
        }
    }
}

/// 入手元。DB には `modrinth`、`local` で持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Provider {
    Modrinth,
    Local,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Modrinth => "modrinth",
            Provider::Local => "local",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "modrinth" => Provider::Modrinth,
            _ => Provider::Local,
        }
    }
}

/// 入れた経緯。依存として入ったものは、使われなくなれば片付けの候補になる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum InstalledAs {
    Explicit,
    Dependency,
}

impl InstalledAs {
    pub fn as_str(self) -> &'static str {
        match self {
            InstalledAs::Explicit => "explicit",
            InstalledAs::Dependency => "dependency",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "dependency" => InstalledAs::Dependency,
            _ => InstalledAs::Explicit,
        }
    }
}

/// クライアント側、サーバー側での対応状況（Modrinth の `client_side` / `server_side`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Side {
    Required,
    Optional,
    Unsupported,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Required => "required",
            Side::Optional => "optional",
            Side::Unsupported => "unsupported",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "required" => Some(Side::Required),
            "optional" => Some(Side::Optional),
            "unsupported" => Some(Side::Unsupported),
            _ => None,
        }
    }
}

/// 画面に出す 1 件分。`rel_path` は実際のファイル名（無効なら `.disabled` 付き）で持つ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContentItem {
    pub kind: ContentKind,
    pub rel_path: String,
    pub file_name: String,
    pub enabled: bool,
    #[ts(type = "number")]
    pub size: u64,
    pub sha1: String,
    pub provider: Provider,
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub pinned: bool,
    pub installed_as: InstalledAs,
    pub name: Option<String>,
    pub version_number: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub has_local_icon: bool,
    pub client_side: Option<Side>,
    pub server_side: Option<Side>,
    pub categories: Vec<String>,
    pub dependencies: Vec<String>,
    pub identified: bool,
}
