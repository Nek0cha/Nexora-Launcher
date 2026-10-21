//! MOD の更新の確認。入っているファイルの SHA-1 を使い、Modrinth に一括で問い合わせる。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use ts_rs::TS;

use super::modrinth::updates_by_hash;
use super::store::Row;
use super::{ContentKind, Provider};
use crate::error::CoreError;
use crate::net::Http;

/// 確認の結果を使い回す時間。
pub const CACHE_TTL: Duration = Duration::from_secs(15 * 60);

/// 更新できる 1 件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AvailableUpdate {
    pub rel_path: String,
    pub version_id: String,
    pub version_number: String,
}

/// 更新の確認の結果。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateCheck {
    pub updates: Vec<AvailableUpdate>,
    /// このインスタンスの Minecraft とローダーに合う版が無かった、Modrinth の MOD の `rel_path`。
    pub no_compatible: Vec<String>,
}

/// 現在時刻の取り方。テストでは差し替える。
pub type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// インスタンスごとの確認結果のキャッシュ。
pub struct UpdateCache {
    entries: Mutex<HashMap<String, (Instant, UpdateCheck)>>,
    clock: Clock,
    /// 捨てるたびに増やす。確認の最中に捨てられたら、その結果を入れない。
    generation: AtomicU64,
}

impl Default for UpdateCache {
    fn default() -> Self {
        Self::with_clock(Arc::new(Instant::now))
    }
}

impl UpdateCache {
    pub fn with_clock(clock: Clock) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            clock,
            generation: AtomicU64::new(0),
        }
    }

    /// 期限内の結果。
    pub fn get(&self, instance_id: &str) -> Option<UpdateCheck> {
        let entries = self.entries.lock().unwrap();
        let (at, check) = entries.get(instance_id)?;
        ((self.clock)().saturating_duration_since(*at) < CACHE_TTL).then(|| check.clone())
    }

    /// 確認を始める前に呼び、`put` に渡す。
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// `generation` を取った後に捨てられていなければ入れる。
    pub fn put(&self, instance_id: &str, generation: u64, check: UpdateCheck) {
        let mut entries = self.entries.lock().unwrap();
        if self.generation.load(Ordering::SeqCst) == generation {
            entries.insert(instance_id.to_string(), ((self.clock)(), check));
        }
    }

    /// そのインスタンスの結果を捨てる。
    pub fn invalidate(&self, instance_id: &str) {
        let mut entries = self.entries.lock().unwrap();
        self.generation.fetch_add(1, Ordering::SeqCst);
        entries.remove(instance_id);
    }
}

/// 更新を確認する。対象は固定していない MOD で、有効無効は問わない。
/// ハッシュが分かれば、入手元が `local` の行も対象にする。通信は 500 件ごとに 1 回。
pub(crate) async fn check(
    http: &Http,
    api: &str,
    rows: &[Row],
    mc_version: &str,
    loaders: &[String],
) -> Result<UpdateCheck, CoreError> {
    let targets: Vec<&Row> = rows
        .iter()
        .filter(|r| r.item.kind == ContentKind::Mod && !r.item.pinned && !r.item.sha1.is_empty())
        .collect();
    let mut hashes: Vec<String> = Vec::new();
    for r in &targets {
        if !hashes.contains(&r.item.sha1) {
            hashes.push(r.item.sha1.clone());
        }
    }
    if hashes.is_empty() {
        return Ok(UpdateCheck::default());
    }
    let found = updates_by_hash(http, api, &hashes, mc_version, loaders).await?;
    let mut out = UpdateCheck::default();
    for r in targets {
        match found.get(&r.item.sha1) {
            Some(v) => {
                if r.item.version_id.as_deref() != Some(v.id.as_str()) {
                    out.updates.push(AvailableUpdate {
                        rel_path: r.item.rel_path.clone(),
                        version_id: v.id.clone(),
                        version_number: v.version_number.clone(),
                    });
                }
            }
            // 入手元が分かっていて、合う版が無いもの。分からない行は何も言えない
            None if r.item.provider == Provider::Modrinth => {
                out.no_compatible.push(r.item.rel_path.clone());
            }
            None => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock_at(offset: Arc<AtomicU64>) -> Clock {
        let base = Instant::now();
        Arc::new(move || base + Duration::from_secs(offset.load(Ordering::SeqCst)))
    }

    #[test]
    fn cache_expires_after_the_ttl() {
        let offset = Arc::new(AtomicU64::new(0));
        let cache = UpdateCache::with_clock(clock_at(offset.clone()));
        cache.put("a", cache.generation(), UpdateCheck::default());
        assert!(cache.get("a").is_some());
        offset.store(14 * 60, Ordering::SeqCst);
        assert!(cache.get("a").is_some());
        offset.store(15 * 60, Ordering::SeqCst);
        assert!(cache.get("a").is_none());
    }

    #[test]
    fn put_after_an_invalidation_is_dropped() {
        let cache = UpdateCache::default();
        let generation = cache.generation();
        cache.invalidate("a");
        cache.put("a", generation, UpdateCheck::default());
        assert!(cache.get("a").is_none());
        cache.put("a", cache.generation(), UpdateCheck::default());
        assert!(cache.get("a").is_some());
    }
}
