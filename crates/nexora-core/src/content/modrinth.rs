//! Modrinth のクライアント（ハッシュからの版の特定、プロジェクトの情報、カテゴリー、検索、版の詳細、更新の確認）。

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::Side;
use crate::error::CoreError;
use crate::meta::{fetch_cached, parse_json};
use crate::net::Http;

pub const MODRINTH_API_URL: &str = "https://api.modrinth.com/v2";

/// `POST /version_files/update` に一度に送るハッシュの数。実測では 5000 件まで受け付けられたが、上限は分からないので余裕を見て分ける。
pub const UPDATE_BATCH: usize = 500;

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
/// 件数が多いときの分割は呼び出し側で行わない（実測では 5000 件まで受け付けられた）。
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

// ---- 検索、版の詳細、更新の確認 ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchSort {
    #[default]
    Relevance,
    Downloads,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub query: String,
    pub mc_version: Option<String>,
    pub loaders: Vec<String>,
    pub categories: Vec<String>,
    pub client_sides: Vec<Side>,
    pub server_sides: Vec<Side>,
    pub sort: SearchSort,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    #[ts(type = "number")]
    pub downloads: u64,
    pub categories: Vec<String>,
    pub versions: Vec<String>,
    pub client_side: Option<Side>,
    pub server_side: Option<Side>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    pub offset: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    Required,
    Optional,
    Incompatible,
    Embedded,
}

impl DependencyKind {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "required" => Some(Self::Required),
            "optional" => Some(Self::Optional),
            "incompatible" => Some(Self::Incompatible),
            "embedded" => Some(Self::Embedded),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub kind: DependencyKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub url: String,
    pub filename: String,
    pub sha1: String,
    pub sha512: Option<String>,
    pub size: u64,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullVersion {
    pub id: String,
    pub project_id: String,
    pub version_number: String,
    pub version_type: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<FileInfo>,
    pub dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
struct RawSearchPage {
    #[serde(default)]
    hits: Vec<RawSearchHit>,
    #[serde(default)]
    offset: u32,
    #[serde(default)]
    total_hits: u32,
}

#[derive(Deserialize)]
struct RawSearchHit {
    project_id: String,
    slug: String,
    title: String,
    #[serde(default)]
    description: String,
    icon_url: Option<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    versions: Vec<String>,
    client_side: Option<String>,
    server_side: Option<String>,
}

/// 版の詳細。5-3 の `RawVersion` とは別に、ファイルと依存の詳細まで読む。
#[derive(Deserialize)]
struct RawFullVersion {
    id: String,
    project_id: String,
    version_number: String,
    #[serde(default)]
    version_type: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    files: Vec<RawFile>,
    #[serde(default)]
    dependencies: Vec<RawFullDependency>,
}

#[derive(Deserialize)]
struct RawFile {
    url: String,
    filename: String,
    #[serde(default)]
    hashes: HashMap<String, String>,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    primary: bool,
}

#[derive(Deserialize)]
struct RawFullDependency {
    project_id: Option<String>,
    version_id: Option<String>,
    dependency_type: String,
}

impl From<RawFullVersion> for FullVersion {
    fn from(v: RawFullVersion) -> Self {
        FullVersion {
            id: v.id,
            project_id: v.project_id,
            version_number: v.version_number,
            version_type: v.version_type,
            game_versions: v.game_versions,
            loaders: v.loaders,
            files: v
                .files
                .into_iter()
                .map(|f| FileInfo {
                    url: f.url,
                    filename: f.filename,
                    sha1: f.hashes.get("sha1").cloned().unwrap_or_default(),
                    sha512: f.hashes.get("sha512").cloned(),
                    size: f.size,
                    primary: f.primary,
                })
                .collect(),
            dependencies: v
                .dependencies
                .into_iter()
                .filter_map(|d| {
                    Some(Dependency {
                        project_id: d.project_id,
                        version_id: d.version_id,
                        // 知らない種類の依存は捨てる
                        kind: DependencyKind::parse(&d.dependency_type)?,
                    })
                })
                .collect(),
        }
    }
}

fn json_array(values: &[String]) -> Result<String, CoreError> {
    serde_json::to_string(values).map_err(|e| CoreError::Unexpected {
        message: e.to_string(),
    })
}

fn build_url(base: String, params: &[(&str, String)]) -> Result<reqwest::Url, CoreError> {
    reqwest::Url::parse_with_params(&base, params).map_err(|e| CoreError::Unexpected {
        message: format!("invalid modrinth url: {e}"),
    })
}

/// facets は、外側の配列どうしが AND、内側の配列の中が OR になる。
fn facets(q: &SearchQuery) -> Vec<Vec<String>> {
    let mut groups = vec![vec!["project_type:mod".to_string()]];
    if let Some(v) = &q.mc_version {
        groups.push(vec![format!("versions:{v}")]);
    }
    if !q.loaders.is_empty() {
        groups.push(
            q.loaders
                .iter()
                .map(|l| format!("categories:{l}"))
                .collect(),
        );
    }
    for c in &q.categories {
        groups.push(vec![format!("categories:{c}")]);
    }
    if !q.client_sides.is_empty() {
        groups.push(
            q.client_sides
                .iter()
                .map(|s| format!("client_side:{}", s.as_str()))
                .collect(),
        );
    }
    if !q.server_sides.is_empty() {
        groups.push(
            q.server_sides
                .iter()
                .map(|s| format!("server_side:{}", s.as_str()))
                .collect(),
        );
    }
    groups
}

/// MOD を検索する。
pub async fn search(
    http: &Http,
    api: &str,
    q: &SearchQuery,
    offset: u32,
    limit: u32,
) -> Result<SearchPage, CoreError> {
    let facets_json = serde_json::to_string(&facets(q)).map_err(|e| CoreError::Unexpected {
        message: e.to_string(),
    })?;
    let index = match q.sort {
        SearchSort::Relevance => "relevance",
        SearchSort::Downloads => "downloads",
    };
    let url = build_url(
        format!("{api}/search"),
        &[
            ("query", q.query.clone()),
            ("facets", facets_json),
            ("index", index.to_string()),
            ("offset", offset.to_string()),
            ("limit", limit.to_string()),
        ],
    )?;
    let bytes = http.get_rate_limited(url.as_str()).await?;
    let raw: RawSearchPage = parse_json(&bytes, "modrinth search")?;
    Ok(SearchPage {
        hits: raw
            .hits
            .into_iter()
            .map(|h| SearchHit {
                project_id: h.project_id,
                slug: h.slug,
                title: h.title,
                description: h.description,
                icon_url: h.icon_url,
                downloads: h.downloads,
                categories: h.categories,
                versions: h.versions,
                client_side: h.client_side.as_deref().and_then(Side::parse),
                server_side: h.server_side.as_deref().and_then(Side::parse),
            })
            .collect(),
        offset: raw.offset,
        total: raw.total_hits,
    })
}

/// プロジェクトの版を、指定の Minecraft とローダーで絞って新しい順に取る。
pub async fn project_versions(
    http: &Http,
    api: &str,
    project_id: &str,
    mc_version: &str,
    loaders: &[String],
) -> Result<Vec<FullVersion>, CoreError> {
    let mut params = vec![("game_versions", json_array(&[mc_version.to_string()])?)];
    if !loaders.is_empty() {
        params.push(("loaders", json_array(loaders)?));
    }
    let url = build_url(format!("{api}/project/{project_id}/version"), &params)?;
    let bytes = http.get_rate_limited(url.as_str()).await?;
    let raw: Vec<RawFullVersion> = parse_json(&bytes, "modrinth project versions")?;
    Ok(raw.into_iter().map(FullVersion::from).collect())
}

/// 版の ID から詳細をまとめて取る。
pub async fn versions_by_ids(
    http: &Http,
    api: &str,
    ids: &[String],
) -> Result<Vec<FullVersion>, CoreError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let url = build_url(format!("{api}/versions"), &[("ids", json_array(ids)?)])?;
    let bytes = http.get_rate_limited(url.as_str()).await?;
    let raw: Vec<RawFullVersion> = parse_json(&bytes, "modrinth versions")?;
    Ok(raw.into_iter().map(FullVersion::from).collect())
}

/// 入っているファイルの SHA-1 から、指定の Minecraft とローダーでの最新の版を引く。
/// 結果は渡したハッシュをキーにし、該当しないものは含まれない。
pub async fn updates_by_hash(
    http: &Http,
    api: &str,
    sha1s: &[String],
    mc_version: &str,
    loaders: &[String],
) -> Result<HashMap<String, FullVersion>, CoreError> {
    let mut out = HashMap::new();
    let url = format!("{api}/version_files/update");
    for chunk in sha1s.chunks(UPDATE_BATCH) {
        let body = serde_json::json!({
            "hashes": chunk,
            "algorithm": "sha1",
            "loaders": loaders,
            "game_versions": [mc_version],
        });
        let bytes = http.post_json(&url, &body).await?;
        let raw: HashMap<String, RawFullVersion> =
            parse_json(&bytes, "modrinth version_files update")?;
        // 応答のキーは小文字なので、大文字小文字を問わず入力のハッシュに対応づける
        let mut by_lower: HashMap<String, RawFullVersion> = raw
            .into_iter()
            .map(|(hash, v)| (hash.to_ascii_lowercase(), v))
            .collect();
        for hash in chunk {
            if let Some(v) = by_lower.remove(&hash.to_ascii_lowercase()) {
                out.insert(hash.clone(), FullVersion::from(v));
            }
        }
    }
    Ok(out)
}

/// 新しい順の版から、最新の `release` を選ぶ。無ければ先頭。
pub fn pick_version(versions: &[FullVersion]) -> Option<&FullVersion> {
    versions
        .iter()
        .find(|v| v.version_type == "release")
        .or_else(|| versions.first())
}

/// 主ファイル。`primary` が無ければ先頭。
pub fn primary_file(v: &FullVersion) -> Option<&FileInfo> {
    v.files
        .iter()
        .find(|f| f.primary)
        .or_else(|| v.files.first())
}

/// Modrinth のローダー名に直す。Quilt は Fabric 用の MOD も動くので両方を返す。`vanilla` や不明は空。
pub fn loader_slugs(loader: Option<&str>) -> Vec<String> {
    match loader {
        Some("quilt") => vec!["quilt".into(), "fabric".into()],
        Some(l @ ("fabric" | "forge" | "neoforge")) => vec![l.into()],
        _ => Vec::new(),
    }
}
