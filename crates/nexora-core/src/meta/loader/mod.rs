//! ローダー（Fabric、Quilt など）の種類とメタデータ。

pub mod forge;
pub mod merge;

use std::cmp::Ordering;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::CoreError;
use crate::meta::mojang::version::{Arguments, Library};
use crate::meta::{fetch_cached, parse_json, write_atomic};
use crate::net::{Http, io_err};
use crate::paths::AppPaths;

pub const FABRIC_META_URL: &str = "https://meta.fabricmc.net";
pub const QUILT_META_URL: &str = "https://meta.quiltmc.org";

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
        true
    }
}

/// ローダーの版の一覧の 1 件。一覧は新しい順に並べる。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LoaderVersionEntry {
    pub version: String,
    pub stable: bool,
}

/// ローダーのプロファイル（`inheritsFrom` で Vanilla の version JSON に重ねる部分）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderProfile {
    pub id: String,
    pub inherits_from: String,
    pub main_class: String,
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub libraries: Vec<Library>,
}

/// 一覧（新しい順）のうち、試験版でない最初のもの。
pub fn latest_stable(entries: &[LoaderVersionEntry]) -> Option<&LoaderVersionEntry> {
    entries.iter().find(|e| e.stable)
}

/// ファイル名や URL の区切りに入れてよいバージョンの値か。
/// 空、`.`、`..` を含むもの、区切り文字（`/`、`\`、`:`）を含むものは、パスの外に出うるため認めない。
pub fn is_safe_version(s: &str) -> bool {
    !s.is_empty() && s != "." && !s.contains("..") && !s.contains(['/', '\\', ':'])
}

/// 安全でないバージョンの値を `InvalidMetadata` にする。
pub(crate) fn check_version(what: &str, value: &str) -> Result<(), CoreError> {
    if is_safe_version(value) {
        Ok(())
    } else {
        Err(CoreError::InvalidMetadata {
            message: format!("unsafe {what} {value:?}"),
        })
    }
}

/// 固定するプロファイルの ID。Fabric と Quilt だけに使う。
/// どちらかのバージョンが安全でなければ `InvalidMetadata`。
pub fn profile_id(
    kind: LoaderKind,
    loader_version: &str,
    mc_version: &str,
) -> Result<String, CoreError> {
    debug_assert!(
        matches!(kind, LoaderKind::Fabric | LoaderKind::Quilt),
        "profile_id is only for Fabric and Quilt"
    );
    check_version("loader version", loader_version)?;
    check_version("Minecraft version", mc_version)?;
    Ok(format!(
        "{}-loader-{loader_version}-{mc_version}",
        kind.as_str()
    ))
}

pub fn profile_path(paths: &AppPaths, id: &str) -> PathBuf {
    paths.versions_dir().join(id).join(format!("{id}.json"))
}

/// 固定済みのプロファイルを読む。ネットワークは使わない。読めない中身は `InvalidMetadata`。
pub fn load_pinned_profile(paths: &AppPaths, id: &str) -> Result<Option<LoaderProfile>, CoreError> {
    let path = profile_path(paths, id);
    match std::fs::read(&path) {
        Ok(bytes) => parse_json(&bytes, "loader profile").map(Some),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_err(&path, source)),
    }
}

/// Minecraft のバージョンを重複なく、新しい順にする。
fn newest_first_mc(mcs: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = mcs.collect();
    out.sort_by(|a, b| compare_parts(b, a));
    out.dedup();
    out
}

#[derive(Deserialize)]
struct GameVersion {
    version: String,
}

#[derive(Deserialize)]
struct LoaderListEntry {
    loader: LoaderInfo,
}

#[derive(Deserialize)]
struct LoaderInfo {
    version: String,
    /// Fabric だけが持つ。Quilt は版の名前で判断する。
    stable: Option<bool>,
}

/// ゲームごとのローダー一覧を読み、新しい順の `LoaderVersionEntry` にする。
/// Fabric は `loader.stable` を使い、メタデータの順（新しい順）を保つ。
/// Quilt は `-` を含む版を試験版とし、メタデータが新しい順に並んでいないため版で並べ直す。
fn parse_loader_versions(
    kind: LoaderKind,
    bytes: &[u8],
) -> Result<Vec<LoaderVersionEntry>, CoreError> {
    let list: Vec<LoaderListEntry> = parse_json(bytes, "loader versions")?;
    let mut out: Vec<LoaderVersionEntry> = list
        .into_iter()
        .map(|e| {
            let stable = match kind {
                LoaderKind::Quilt => !e.loader.version.contains('-'),
                _ => e.loader.stable.unwrap_or(false),
            };
            LoaderVersionEntry {
                version: e.loader.version,
                stable,
            }
        })
        .collect();
    if kind == LoaderKind::Quilt {
        out.sort_by(|a, b| compare_versions(&b.version, &a.version));
    }
    Ok(out)
}

/// `1.2.3-beta.4` の形の版を比べる。本体は数で比べ、本体が同じなら試験版を古いとする。
/// 試験版の部分は `.` で区切り、両方とも数なら数で、それ以外は文字列で比べる。
fn compare_versions(a: &str, b: &str) -> Ordering {
    let (a_core, a_pre) = split_pre(a);
    let (b_core, b_pre) = split_pre(b);
    compare_parts(a_core, b_core).then_with(|| match (a_pre, b_pre) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => compare_parts(x, y),
    })
}

fn split_pre(v: &str) -> (&str, Option<&str>) {
    match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (v, None),
    }
}

pub(crate) fn compare_parts(a: &str, b: &str) -> Ordering {
    let mut xs = a.split('.');
    let mut ys = b.split('.');
    loop {
        let ord = match (xs.next(), ys.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(x), Some(y)) => match (x.parse::<u64>(), y.parse::<u64>()) {
                (Ok(x), Ok(y)) => x.cmp(&y),
                _ => x.cmp(y),
            },
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
}

/// Fabric（v2）と Quilt（v3）の公式メタデータ、Forge と NeoForge の公式 Maven のメタデータ。
#[derive(Clone)]
pub struct LoaderMeta {
    http: Http,
    paths: AppPaths,
    fabric_url: String,
    quilt_url: String,
    forge_maven: String,
    forge_promotions: String,
    neoforge_maven: String,
}

impl LoaderMeta {
    pub fn new(
        http: Http,
        paths: AppPaths,
        fabric_url: impl Into<String>,
        quilt_url: impl Into<String>,
    ) -> Self {
        Self {
            http,
            paths,
            fabric_url: fabric_url.into(),
            quilt_url: quilt_url.into(),
            forge_maven: forge::FORGE_MAVEN_URL.to_string(),
            forge_promotions: forge::FORGE_PROMOTIONS_URL.to_string(),
            neoforge_maven: forge::NEOFORGE_MAVEN_URL.to_string(),
        }
    }

    /// Forge と NeoForge の取得先を差し替える（試験用）。
    pub fn with_forge_urls(
        mut self,
        forge_maven: impl Into<String>,
        promotions: impl Into<String>,
        neoforge_maven: impl Into<String>,
    ) -> Self {
        self.forge_maven = forge_maven.into();
        self.forge_promotions = promotions.into();
        self.neoforge_maven = neoforge_maven.into();
        self
    }

    /// ローダーが対応する Minecraft のバージョン（メタデータの順）。
    pub async fn game_versions(&self, kind: LoaderKind) -> Result<Vec<String>, CoreError> {
        match kind {
            LoaderKind::Forge => return self.forge_game_versions().await,
            LoaderKind::Neoforge => return self.neoforge_game_versions().await,
            _ => {}
        }
        let url = self.url(kind, &["versions", "game"])?;
        let name = format!("{}-game.json", kind.as_str());
        let body = fetch_cached(&self.http, &self.paths.meta_cache_dir(), &name, &url).await?;
        let list: Vec<GameVersion> = parse_json(&body, "loader game versions")?;
        Ok(list.into_iter().map(|g| g.version).collect())
    }

    /// mc_version に使えるローダーの版（新しい順）。
    pub async fn loader_versions(
        &self,
        kind: LoaderKind,
        mc_version: &str,
    ) -> Result<Vec<LoaderVersionEntry>, CoreError> {
        match kind {
            LoaderKind::Forge => return self.forge_loader_versions(mc_version).await,
            LoaderKind::Neoforge => return self.neoforge_loader_versions(mc_version).await,
            _ => {}
        }
        let url = self.url(kind, &["versions", "loader", mc_version])?;
        // キャッシュのファイル名に入るため、区切り文字を含む値は弾く
        check_version("Minecraft version", mc_version)?;
        let name = format!("{}-loader-{mc_version}.json", kind.as_str());
        let body = fetch_cached(&self.http, &self.paths.meta_cache_dir(), &name, &url).await?;
        parse_loader_versions(kind, &body)
    }

    /// Maven の `maven-metadata.xml` の版を、並びのまま返す。
    async fn maven_versions(
        &self,
        base: &str,
        group_path: &str,
        cache_name: &str,
    ) -> Result<Vec<String>, CoreError> {
        let url = format!(
            "{}/{group_path}/maven-metadata.xml",
            base.trim_end_matches('/')
        );
        let body = fetch_cached(&self.http, &self.paths.meta_cache_dir(), cache_name, &url).await?;
        forge::parse_maven_versions(&body)
    }

    async fn forge_maven_versions(&self) -> Result<Vec<String>, CoreError> {
        self.maven_versions(
            &self.forge_maven,
            "net/minecraftforge/forge",
            "forge-maven.xml",
        )
        .await
    }

    /// NeoForge の 2 系統（新: `net.neoforged:neoforge`、旧: 1.20.1 の `net.neoforged:forge`）の版。
    async fn neoforge_maven_versions(&self) -> Result<(Vec<String>, Vec<String>), CoreError> {
        let new = self
            .maven_versions(
                &self.neoforge_maven,
                "net/neoforged/neoforge",
                "neoforge-maven.xml",
            )
            .await?;
        let old = self
            .maven_versions(
                &self.neoforge_maven,
                "net/neoforged/forge",
                "neoforge-forge-maven.xml",
            )
            .await?;
        Ok((new, old))
    }

    async fn forge_game_versions(&self) -> Result<Vec<String>, CoreError> {
        let versions = self.forge_maven_versions().await?;
        Ok(newest_first_mc(
            versions
                .iter()
                .filter_map(|v| forge::forge_parts(v))
                .map(|(mc, _)| mc.to_string())
                .filter(|mc| forge::forge_supported(mc)),
        ))
    }

    async fn neoforge_game_versions(&self) -> Result<Vec<String>, CoreError> {
        let (new, old) = self.neoforge_maven_versions().await?;
        let from_new = new.iter().filter_map(|v| forge::neoforge_mc_version(v));
        let from_old = old
            .iter()
            .filter_map(|v| forge::forge_parts(v))
            .map(|(mc, _)| mc.to_string());
        Ok(newest_first_mc(
            from_new
                .chain(from_old)
                .filter(|mc| forge::neoforge_supported(mc)),
        ))
    }

    async fn forge_loader_versions(
        &self,
        mc_version: &str,
    ) -> Result<Vec<LoaderVersionEntry>, CoreError> {
        check_version("Minecraft version", mc_version)?;
        let mut versions: Vec<String> = self
            .forge_maven_versions()
            .await?
            .into_iter()
            .filter(|v| forge::forge_parts(v).is_some_and(|(mc, _)| mc == mc_version))
            .collect();
        versions.sort_by(|a, b| forge::compare_forge_newest_first(a, b));
        // 取れなくても一覧は返す（その場合、おすすめの印は付かない）
        let promoted = self.forge_promoted(mc_version).await;
        Ok(versions
            .into_iter()
            .map(|version| LoaderVersionEntry {
                stable: promoted.as_deref() == Some(forge::forge_build(&version)),
                version,
            })
            .collect())
    }

    /// promotions の、その Minecraft のバージョンのおすすめ（なければ最新）のビルド番号。
    async fn forge_promoted(&self, mc_version: &str) -> Option<String> {
        #[derive(Deserialize)]
        struct Promotions {
            promos: std::collections::HashMap<String, String>,
        }
        let fetched = fetch_cached(
            &self.http,
            &self.paths.meta_cache_dir(),
            "forge-promotions.json",
            &self.forge_promotions,
        )
        .await;
        let parsed = fetched.and_then(|b| parse_json::<Promotions>(&b, "forge promotions"));
        match parsed {
            Ok(mut p) => p
                .promos
                .remove(&format!("{mc_version}-recommended"))
                .or_else(|| p.promos.remove(&format!("{mc_version}-latest"))),
            Err(err) => {
                tracing::warn!(code = err.code(), "Forge promotions are unavailable");
                None
            }
        }
    }

    async fn neoforge_loader_versions(
        &self,
        mc_version: &str,
    ) -> Result<Vec<LoaderVersionEntry>, CoreError> {
        check_version("Minecraft version", mc_version)?;
        let (new, old) = self.neoforge_maven_versions().await?;
        // 新系統は `-` より後ろ（beta、alpha など）があれば試験版。並びは版で決める
        let mut new: Vec<String> = new
            .into_iter()
            .filter(|v| forge::neoforge_mc_version(v).as_deref() == Some(mc_version))
            .collect();
        new.sort_by(|a, b| compare_versions(b, a));
        // 旧系統（`1.20.1-47.1.106`）は正式版だけが並ぶ
        let mut old: Vec<String> = old
            .into_iter()
            .filter(|v| forge::forge_parts(v).is_some_and(|(mc, _)| mc == mc_version))
            .collect();
        old.sort_by(|a, b| forge::compare_forge_newest_first(a, b));
        let new = new.into_iter().map(|version| LoaderVersionEntry {
            stable: !version.contains('-'),
            version,
        });
        let old = old.into_iter().map(|version| LoaderVersionEntry {
            stable: true,
            version,
        });
        Ok(new.chain(old).collect())
    }

    /// プロファイルを shared/versions に固定して返す。固定済みで読めるならネットワークに出ない。
    /// 読めない、または inheritsFrom が食い違うときは取り直し、読めることを確かめてから書く。
    /// バージョンの値が安全でなければ、ネットワークにもディスクにも触れずに `InvalidMetadata` にする。
    pub async fn pin_profile(
        &self,
        kind: LoaderKind,
        mc_version: &str,
        loader_version: &str,
    ) -> Result<LoaderProfile, CoreError> {
        let url = self.url(
            kind,
            &[
                "versions",
                "loader",
                mc_version,
                loader_version,
                "profile",
                "json",
            ],
        )?;
        // パスを作る前に、値を確かめる（安全でなければ `InvalidMetadata`）
        let id = profile_id(kind, loader_version, mc_version)?;
        match load_pinned_profile(&self.paths, &id) {
            Ok(Some(profile)) if profile.inherits_from == mc_version => return Ok(profile),
            Ok(Some(_)) => tracing::warn!(id, "pinned loader profile is for another version"),
            Ok(None) => {}
            Err(err) => tracing::warn!(id, code = err.code(), "refetching loader profile"),
        }
        let body = self.http.get_bytes(&url).await?;
        let profile: LoaderProfile = parse_json(&body, "loader profile")?;
        if profile.inherits_from != mc_version {
            return Err(CoreError::InvalidMetadata {
                message: format!(
                    "loader profile {id} inherits from {}, not {mc_version}",
                    profile.inherits_from
                ),
            });
        }
        write_atomic(&profile_path(&self.paths, &id), &body)?;
        Ok(profile)
    }

    /// `{基底}/{v2|v3}/{segments...}`。区切りに入る値は URL エンコードする。
    fn url(&self, kind: LoaderKind, segments: &[&str]) -> Result<String, CoreError> {
        let (base, api) = match kind {
            LoaderKind::Fabric => (&self.fabric_url, "v2"),
            LoaderKind::Quilt => (&self.quilt_url, "v3"),
            _ => {
                return Err(CoreError::UnsupportedProfile {
                    reason: format!("{} has no loader metadata", kind.as_str()),
                });
            }
        };
        let mut url = reqwest::Url::parse(base).map_err(|e| CoreError::Unexpected {
            message: format!("loader metadata URL {base}: {e}"),
        })?;
        url.path_segments_mut()
            .map_err(|()| CoreError::Unexpected {
                message: format!("loader metadata URL {base} cannot have a path"),
            })?
            .pop_if_empty()
            .push(api)
            .extend(segments);
        Ok(url.into())
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
    fn all_loaders_are_supported() {
        let supported: Vec<_> = ALL.into_iter().filter(|k| k.is_supported()).collect();
        assert_eq!(supported, ALL.to_vec());
    }

    const FABRIC_LOADERS: &str =
        include_str!("../../../tests/fixtures/loaders/fabric-loader-1.20.1.json");
    const QUILT_LOADERS: &str =
        include_str!("../../../tests/fixtures/loaders/quilt-loader-1.20.1.json");
    const FABRIC_PROFILE: &str =
        include_str!("../../../tests/fixtures/loaders/fabric-profile-1.20.1-0.16.10.json");
    const QUILT_PROFILE: &str =
        include_str!("../../../tests/fixtures/loaders/quilt-profile-1.20.1-0.28.1.json");

    #[test]
    fn latest_stable_skips_prereleases_even_when_listed_first() {
        let entries = parse_loader_versions(LoaderKind::Quilt, QUILT_LOADERS.as_bytes()).unwrap();
        assert_eq!(entries[0].version, "0.31.0-beta.4");
        assert!(!entries[0].stable);
        assert_eq!(latest_stable(&entries).unwrap().version, "0.28.1");
    }

    #[test]
    fn quilt_versions_are_sorted_newest_first() {
        let versions: Vec<_> = parse_loader_versions(LoaderKind::Quilt, QUILT_LOADERS.as_bytes())
            .unwrap()
            .into_iter()
            .map(|e| e.version)
            .collect();
        assert_eq!(
            versions,
            vec![
                "0.31.0-beta.4",
                "0.28.1",
                "0.28.0",
                "0.24.0",
                "0.20.0-beta.9",
                "0.20.0-beta.8",
                "0.20.0-beta.7",
                "0.20.0-beta.2",
                "0.20.0-beta.1",
            ]
        );
    }

    #[test]
    fn fabric_uses_the_stable_flag() {
        let entries = parse_loader_versions(LoaderKind::Fabric, FABRIC_LOADERS.as_bytes()).unwrap();
        assert_eq!(entries[0].version, "0.20.1");
        assert!(!entries[0].stable);
        assert_eq!(latest_stable(&entries).unwrap().version, "0.19.5");
    }

    #[test]
    fn latest_stable_is_none_without_a_stable_entry() {
        let entries = vec![LoaderVersionEntry {
            version: "1.0.0-beta.1".into(),
            stable: false,
        }];
        assert_eq!(latest_stable(&entries), None);
    }

    #[test]
    fn profile_id_formats() {
        assert_eq!(
            profile_id(LoaderKind::Fabric, "0.16.10", "1.20.1").unwrap(),
            "fabric-loader-0.16.10-1.20.1"
        );
        assert_eq!(
            profile_id(LoaderKind::Quilt, "0.28.1", "1.20.1").unwrap(),
            "quilt-loader-0.28.1-1.20.1"
        );
    }

    #[test]
    fn profile_id_rejects_unsafe_values() {
        for bad in ["", "..", "../x", "a/b", r"a\b", r"..\..\x", "a:b", "1..2"] {
            let err = profile_id(LoaderKind::Fabric, bad, "1.20.1").unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "loader {bad:?}");
            let err = profile_id(LoaderKind::Fabric, "0.16.10", bad).unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "mc {bad:?}");
        }
    }

    #[test]
    fn safe_versions() {
        for ok in [
            "1.20.1",
            "0.17.0-beta.1",
            "1.14 Pre-Release 1",
            "25w14a",
            "b1.7.3",
        ] {
            assert!(is_safe_version(ok), "{ok}");
        }
        for bad in ["", ".", "..", "a/b", r"a\b", "a:b", "1..2"] {
            assert!(!is_safe_version(bad), "{bad:?}");
        }
    }

    #[test]
    fn url_encodes_path_segments() {
        let paths = AppPaths::from_root(std::path::Path::new("unused"));
        let meta = LoaderMeta::new(
            Http::new().unwrap(),
            paths,
            "https://fabric.example/",
            "https://quilt.example/base",
        );
        assert_eq!(
            meta.url(
                LoaderKind::Fabric,
                &["versions", "loader", "1.14 Pre-Release 1/x"]
            )
            .unwrap(),
            "https://fabric.example/v2/versions/loader/1.14%20Pre-Release%201%2Fx"
        );
        assert_eq!(
            meta.url(LoaderKind::Quilt, &["versions", "game"]).unwrap(),
            "https://quilt.example/base/v3/versions/game"
        );
    }

    #[test]
    fn compare_versions_orders_prereleases_before_their_release() {
        let mut vs = vec![
            "0.9.0",
            "0.10.0-beta.10",
            "0.10.0",
            "0.10.0-beta.2",
            "0.10.0-pre.1",
            "0.10.1",
        ];
        vs.sort_by(|a, b| compare_versions(a, b));
        assert_eq!(
            vs,
            vec![
                "0.9.0",
                "0.10.0-beta.2",
                "0.10.0-beta.10",
                "0.10.0-pre.1",
                "0.10.0",
                "0.10.1"
            ]
        );
    }

    #[test]
    fn profile_parses() {
        for (body, id) in [
            (FABRIC_PROFILE, "fabric-loader-0.16.10-1.20.1"),
            (QUILT_PROFILE, "quilt-loader-0.28.1-1.20.1"),
        ] {
            let p: LoaderProfile = parse_json(body.as_bytes(), "loader profile").unwrap();
            assert_eq!(p.id, id);
            assert_eq!(p.inherits_from, "1.20.1");
            assert!(p.main_class.ends_with("KnotClient"), "{}", p.main_class);
            assert!(!p.libraries.is_empty());
            assert!(p.libraries.iter().all(|l| l.url.is_some()));
        }
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
