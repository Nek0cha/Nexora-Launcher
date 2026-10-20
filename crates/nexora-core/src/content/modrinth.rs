//! Modrinth の読み取りクライアント（ハッシュからの版の特定、プロジェクトの情報、カテゴリー）。

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use super::Side;
use crate::error::CoreError;
use crate::meta::{fetch_cached, parse_json};
use crate::net::Http;

pub const MODRINTH_API_URL: &str = "https://api.modrinth.com/v2";

/// `POST /version_files` で分かる、ファイルが属する版。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInfo {
    pub version_id: String,
    pub project_id: String,
    pub version_number: String,
    /// 必須の依存の project_id。
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInfo {
    pub project_id: String,
    pub name: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub client_side: Option<Side>,
    pub server_side: Option<Side>,
    /// `categories` と `additional_categories` を合わせたもの。
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CategoryTag {
    pub name: String,
    pub project_type: String,
}

#[derive(Deserialize)]
struct RawVersion {
    id: String,
    project_id: String,
    version_number: String,
    #[serde(default)]
    dependencies: Vec<RawDependency>,
}

#[derive(Deserialize)]
struct RawDependency {
    project_id: Option<String>,
    dependency_type: String,
}

#[derive(Deserialize)]
struct RawProject {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    icon_url: Option<String>,
    client_side: Option<String>,
    server_side: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    additional_categories: Vec<String>,
}

/// SHA-1 の一覧から版を引く。結果は渡したハッシュをキーにし、分からなかったものは含まれない。
/// 一度に渡せる件数の上限は確かめていない（呼び出し側で分けていない）。
pub async fn versions_by_hash(
    http: &Http,
    api: &str,
    sha1s: &[String],
) -> Result<HashMap<String, VersionInfo>, CoreError> {
    if sha1s.is_empty() {
        return Ok(HashMap::new());
    }
    let url = format!("{api}/version_files");
    let body = serde_json::json!({ "hashes": sha1s, "algorithm": "sha1" });
    let bytes = http.post_json(&url, &body).await?;
    let raw: HashMap<String, RawVersion> = parse_json(&bytes, "modrinth version_files")?;
    // 応答のキーは小文字なので、大文字小文字を問わず入力のハッシュに対応づける
    let by_lower: HashMap<String, RawVersion> = raw
        .into_iter()
        .map(|(hash, version)| (hash.to_ascii_lowercase(), version))
        .collect();
    let mut out = HashMap::new();
    for hash in sha1s {
        if let Some(v) = by_lower.get(&hash.to_ascii_lowercase()) {
            out.insert(
                hash.clone(),
                VersionInfo {
                    version_id: v.id.clone(),
                    project_id: v.project_id.clone(),
                    version_number: v.version_number.clone(),
                    dependencies: v
                        .dependencies
                        .iter()
                        .filter(|d| d.dependency_type == "required")
                        .filter_map(|d| d.project_id.clone())
                        .collect(),
                },
            );
        }
    }
    Ok(out)
}

/// プロジェクトの情報をまとめて取る。
pub async fn projects(
    http: &Http,
    api: &str,
    ids: &[String],
) -> Result<Vec<ProjectInfo>, CoreError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids_json = serde_json::to_string(ids).map_err(|e| CoreError::Unexpected {
        message: e.to_string(),
    })?;
    let url = reqwest::Url::parse_with_params(&format!("{api}/projects"), [("ids", ids_json)])
        .map_err(|e| CoreError::Unexpected {
            message: format!("invalid modrinth url: {e}"),
        })?;
    let bytes = http.get_rate_limited(url.as_str()).await?;
    let raw: Vec<RawProject> = parse_json(&bytes, "modrinth projects")?;
    Ok(raw
        .into_iter()
        .map(|p| {
            let mut categories = p.categories;
            categories.extend(p.additional_categories);
            ProjectInfo {
                project_id: p.id,
                name: p.title,
                description: p.description,
                icon_url: p.icon_url,
                // unknown など、分からない値は None にする
                client_side: p.client_side.as_deref().and_then(Side::parse),
                server_side: p.server_side.as_deref().and_then(Side::parse),
                categories,
            }
        })
        .collect())
}

/// カテゴリーの一覧。キャッシュを使い、通信できないときもキャッシュがあれば返す。
pub async fn categories(
    http: &Http,
    api: &str,
    cache_dir: &Path,
) -> Result<Vec<CategoryTag>, CoreError> {
    let url = format!("{api}/tag/category");
    let bytes = fetch_cached(http, cache_dir, "modrinth-categories.json", &url).await?;
    parse_json(&bytes, "modrinth categories")
}
