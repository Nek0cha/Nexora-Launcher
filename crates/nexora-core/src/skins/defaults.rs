//! デフォルトスキン。画像はアプリに同梱せず、ゲーム本体のクライアント jar から取り出す。
//!
//! 取得元は Mojang の公式のメタデータ（version manifest、version JSON の `downloads.client`）で、
//! ランチャーが起動のために使うものと同じ。jar の `assets/minecraft/textures/entity/player/` の下の
//! `wide/<名前>.png`（classic）と `slim/<名前>.png`（slim）が対象。取り出した画像は
//! `cache/default-skins/` に置き、2 回目からは通信しない。

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;
use uuid::Uuid;

use crate::auth::minecraft::{SKIN_MAX_BYTES, SkinVariant, validate_skin_png};
use crate::auth::service::AuthService;
use crate::error::CoreError;
use crate::meta::mojang::manifest::{MojangMeta, client_jar_path, release_versions};
use crate::meta::write_atomic;
use crate::net::{Depth, DownloadItem, Http, fetch_all, io_err, needs_download};
use crate::paths::AppPaths;

/// jar の中のデフォルトスキンの置き場所。
const JAR_PREFIX: &str = "assets/minecraft/textures/entity/player/";
/// キャッシュの索引のファイル名。画像を全部書いたあとで最後に書く。
const INDEX_FILE: &str = "index.json";

/// 画面に渡すデフォルトスキン。`image` は `data:image/png;base64,...`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DefaultSkin {
    /// 画面と適用の命令で使う識別子（`classic-steve`、`slim-alex` など）。
    pub key: String,
    pub name: String,
    pub variant: SkinVariant,
    pub image: String,
    /// 画素の内容のハッシュ（保存したスキンと同じ `pixel_hash`）。着ているスキンとの突き合わせに使う。
    pub sha256: String,
}

/// キャッシュの索引の 1 件。画像は `<key>.png` に別に置く。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexEntry {
    key: String,
    name: String,
    variant: SkinVariant,
}

/// 画像を持つデフォルトスキン。
struct Entry {
    meta: IndexEntry,
    png: Vec<u8>,
}

/// 同時に呼ばれても jar を二重に取らないための鍵（後の呼び出しはキャッシュを読む）。
static FETCH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn cache_dir(paths: &AppPaths) -> PathBuf {
    paths.cache_dir().join("default-skins")
}

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::InvalidMetadata {
        message: message.into(),
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, CoreError> + Send + 'static,
) -> Result<T, CoreError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| CoreError::Unexpected {
            message: e.to_string(),
        })?
}

/// すべてのデフォルトスキンを返す。キャッシュがあれば通信しない。
/// 取得元に届かない、または使える画像が 1 枚もないときは `Err`。
pub async fn list_defaults(
    http: &Http,
    paths: &AppPaths,
    manifest_url: &str,
) -> Result<Vec<DefaultSkin>, CoreError> {
    let entries = load(http, paths, manifest_url).await?;
    Ok(entries
        .into_iter()
        .map(|e| DefaultSkin {
            sha256: crate::skins::pixel_hash(&e.png),
            image: format!("data:image/png;base64,{}", STANDARD.encode(&e.png)),
            key: e.meta.key,
            name: e.meta.name,
            variant: e.meta.variant,
        })
        .collect())
}

/// キャッシュ済みのデフォルトスキンの画素のハッシュ。通信はしない（キャッシュが無ければ空）。
/// 現在のスキンの取り込みで、デフォルトスキンを除くのに使う。
pub async fn cached_hashes(paths: &AppPaths) -> HashSet<String> {
    let dir = cache_dir(paths);
    blocking(move || {
        Ok(read_cache(&dir)
            .unwrap_or_default()
            .iter()
            .map(|e| crate::skins::pixel_hash(&e.png))
            .collect())
    })
    .await
    .unwrap_or_default()
}

/// デフォルトスキンをアカウントに適用する。保存したスキンと同じ `change_skin` の経路で、
/// ライブラリには足さない。知らないキーは `SkinNotFound`（アップロードしない）。
pub async fn apply_default(
    http: &Http,
    paths: &AppPaths,
    manifest_url: &str,
    auth: &AuthService,
    account_id: Uuid,
    key: &str,
) -> Result<(), CoreError> {
    let entry = load(http, paths, manifest_url)
        .await?
        .into_iter()
        .find(|e| e.meta.key == key)
        .ok_or(CoreError::SkinNotFound)?;
    auth.change_skin(account_id, entry.meta.variant, entry.png)
        .await
}

/// キャッシュを読み、無ければ取得して書く。
async fn load(http: &Http, paths: &AppPaths, manifest_url: &str) -> Result<Vec<Entry>, CoreError> {
    let _guard = FETCH_LOCK.lock().await;
    let dir = cache_dir(paths);
    let cached = blocking({
        let dir = dir.clone();
        move || Ok(read_cache(&dir))
    })
    .await?;
    if let Some(entries) = cached {
        return Ok(entries);
    }
    let entries = fetch(http, paths, manifest_url, &dir).await?;
    blocking({
        let (dir, indexed) = (dir.clone(), entries_for_write(&entries));
        move || write_cache(&dir, &indexed)
    })
    .await?;
    Ok(entries)
}

fn entries_for_write(entries: &[Entry]) -> Vec<(IndexEntry, Vec<u8>)> {
    entries
        .iter()
        .map(|e| (e.meta.clone(), e.png.clone()))
        .collect()
}

/// キャッシュを読む。索引が無い、読めない、使える画像が 0 枚なら `None`（取り直す）。
/// 画像が欠けている、壊れている 1 件は捨てる。
fn read_cache(dir: &Path) -> Option<Vec<Entry>> {
    let index: Vec<IndexEntry> =
        serde_json::from_slice(&std::fs::read(dir.join(INDEX_FILE)).ok()?).ok()?;
    let entries: Vec<Entry> = index
        .into_iter()
        .filter(|meta| safe_stem(&meta.key))
        .filter_map(|meta| {
            let png = std::fs::read(dir.join(format!("{}.png", meta.key))).ok()?;
            validate_skin_png(&png).ok()?;
            Some(Entry { meta, png })
        })
        .collect();
    (!entries.is_empty()).then_some(entries)
}

/// 画像を書いてから、最後に索引を書く（途中で止まっても「取得済み」にならない）。
fn write_cache(dir: &Path, entries: &[(IndexEntry, Vec<u8>)]) -> Result<(), CoreError> {
    for (meta, png) in entries {
        write_atomic(&dir.join(format!("{}.png", meta.key)), png)?;
    }
    let index: Vec<&IndexEntry> = entries.iter().map(|(meta, _)| meta).collect();
    let bytes = serde_json::to_vec(&index).map_err(|e| CoreError::Unexpected {
        message: e.to_string(),
    })?;
    write_atomic(&dir.join(INDEX_FILE), &bytes)
}

/// 公式のメタデータをたどって最新の正式版のクライアント jar を用意し、デフォルトスキンを取り出す。
async fn fetch(
    http: &Http,
    paths: &AppPaths,
    manifest_url: &str,
    dir: &Path,
) -> Result<Vec<Entry>, CoreError> {
    let meta = MojangMeta::new(http.clone(), paths.clone(), manifest_url);
    let id = release_versions(&meta.manifest().await?)?
        .into_iter()
        .next()
        .ok_or_else(|| invalid("the version manifest has no release"))?
        .id;
    let token = CancellationToken::new();
    let client = meta.pin_version(&id, &token).await?.downloads.client;
    let shared = client_jar_path(paths, &id);
    let item = DownloadItem {
        url: client.url,
        path: shared.clone(),
        size: Some(client.size),
        sha1: Some(client.sha1),
        executable: false,
    };
    let missing = {
        let item = item.clone();
        blocking(move || needs_download(&item, Depth::Full).map_err(|e| io_err(&item.path, e)))
            .await?
    };
    if !missing {
        // 起動のために取得済みの jar があれば、それを読む（約 40 MB を取り直さない）
        return extract_skins(shared).await;
    }
    // 無ければ、キャッシュの下へ一時的に取得し、取り出したら消す
    std::fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    let temp = dir.join(format!("client-{id}.jar"));
    let item = DownloadItem {
        path: temp.clone(),
        ..item
    };
    let result = match fetch_all(http, vec![item], Depth::Full, 1, &token, |_, _| {}).await {
        Ok(()) => extract_skins(temp.clone()).await,
        Err(err) => Err(err),
    };
    let _ = std::fs::remove_file(&temp);
    result
}

async fn extract_skins(jar: PathBuf) -> Result<Vec<Entry>, CoreError> {
    blocking(move || {
        let file = std::fs::File::open(&jar).map_err(|e| io_err(&jar, e))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| invalid(format!("the client jar is not a zip: {e}")))?;
        let mut entries = Vec::new();
        for i in 0..archive.len() {
            // 読めない 1 件は捨てる
            let Ok(mut member) = archive.by_index(i) else {
                continue;
            };
            let Some((variant, stem)) = member.name().ok().and_then(|n| parse_member(&n)) else {
                continue;
            };
            if !member.is_file() || member.size() > SKIN_MAX_BYTES as u64 {
                continue;
            }
            let mut png = Vec::new();
            if (&mut member)
                .take(SKIN_MAX_BYTES as u64 + 1)
                .read_to_end(&mut png)
                .is_err()
                || validate_skin_png(&png).is_err()
            {
                continue;
            }
            let prefix = match variant {
                SkinVariant::Classic => "classic",
                SkinVariant::Slim => "slim",
            };
            entries.push(Entry {
                meta: IndexEntry {
                    key: format!("{prefix}-{stem}"),
                    name: capitalize(&stem),
                    variant,
                },
                png,
            });
        }
        if entries.is_empty() {
            return Err(invalid("the client jar holds no usable default skin"));
        }
        // classic を先に、同じ型の中は名前順
        entries.sort_by(|a, b| {
            let rank = |v: SkinVariant| u8::from(v == SkinVariant::Slim);
            (rank(a.meta.variant), &a.meta.key).cmp(&(rank(b.meta.variant), &b.meta.key))
        });
        entries.dedup_by(|a, b| a.meta.key == b.meta.key);
        Ok(entries)
    })
    .await
}

/// jar の中の名前が `wide/<名前>.png` か `slim/<名前>.png` なら、(型, 名前)。
fn parse_member(name: &str) -> Option<(SkinVariant, String)> {
    let (dir, file) = name.strip_prefix(JAR_PREFIX)?.split_once('/')?;
    let variant = match dir {
        "wide" => SkinVariant::Classic,
        "slim" => SkinVariant::Slim,
        _ => return None,
    };
    let stem = file.strip_suffix(".png")?;
    safe_stem(stem).then(|| (variant, stem.to_string()))
}

/// ファイル名に使う文字だけ（キャッシュのパスを組むため、区切りや `..` を通さない）。
fn safe_stem(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn capitalize(stem: &str) -> String {
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_member_accepts_only_wide_and_slim_pngs() {
        let p = |tail: &str| parse_member(&format!("{JAR_PREFIX}{tail}"));
        assert_eq!(
            p("wide/steve.png"),
            Some((SkinVariant::Classic, "steve".into()))
        );
        assert_eq!(p("slim/alex.png"), Some((SkinVariant::Slim, "alex".into())));
        assert_eq!(p("wide/steve.txt"), None);
        assert_eq!(p("wide/"), None);
        assert_eq!(p("wide/.png"), None);
        assert_eq!(p("other/steve.png"), None);
        assert_eq!(p("wide/sub/steve.png"), None);
        assert_eq!(p("wide/Steve.png"), None);
        assert_eq!(p("wide/..%2f.png"), None);
        assert_eq!(
            parse_member("assets/minecraft/textures/entity/steve.png"),
            None
        );
    }

    #[test]
    fn capitalize_uppercases_only_the_first_letter() {
        assert_eq!(capitalize("steve"), "Steve");
        assert_eq!(capitalize("a"), "A");
        assert_eq!(capitalize(""), "");
    }
}
