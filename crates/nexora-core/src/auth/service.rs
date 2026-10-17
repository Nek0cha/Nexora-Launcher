//! アカウントのセッションをまとめて扱う。
//!
//! `accounts` テーブル、資格情報ストアの refresh token、メモリのトークン、アカウントごとの
//! ロックを束ね、トークンの連鎖（spec 4.3）、所有権の判定（4.4）、ログインし直しの判定（4.5）、
//! 起動の前のセッションの用意（4.6）、オフラインで遊ぶ条件（4.7）を受け持つ。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use secrecy::SecretString;
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;
use uuid::Uuid;

use super::accounts::{self, AccountRow, EntitlementState, ProfileRecord};
use super::http::AuthHttp;
use super::login::LoginProgress;
use super::microsoft::{self, MsTokens};
use super::minecraft::{self, McToken};
use super::store::SecretStore;
use super::{AuthUrls, OFFLINE_GRACE_MS, REFRESH_MARGIN_MS, xbox};
use crate::db::Db;
use crate::error::CoreError;
use crate::install::now_ms;
#[cfg(debug_assertions)]
use crate::launch::session;
use crate::launch::session::{LaunchSession, UserType};
use crate::paths::AppPaths;

type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;
/// アカウントの一覧に見える変化（追加、削除、切り替え、`needs_login`）を知らせる関数。
pub type ChangeFn = Arc<dyn Fn() + Send + Sync>;
/// ログインの進捗を知らせる関数。ログインのタスク（Send が必要）から呼ぶため Sync とする。
pub type ProgressFn<'a> = &'a (dyn Fn(LoginProgress) + Send + Sync);

/// アカウント画面に渡す 1 件。トークンは含めない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Account {
    /// UUID（ハイフンなしの 32 文字。`accounts.id` と同じ形）。
    pub id: String,
    pub username: String,
    pub is_active: bool,
    pub entitlement: EntitlementState,
    pub needs_login: bool,
    /// refresh token を資格情報ストアに保存できているか。
    pub login_saved: bool,
    pub has_skin: bool,
}

/// 開発用のオフラインアカウント。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DevOfflineAccount {
    pub username: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AccountList {
    pub accounts: Vec<Account>,
    /// 製品版のビルドでは常に None。
    pub dev_offline: Option<DevOfflineAccount>,
}

/// 「オフラインで遊ぶ」を出せるか（spec 4.7）。ログインし直しが必要なときは出さない。
pub fn offline_allowed(row: &AccountRow, now_ms: i64) -> bool {
    !row.needs_login
        && row.entitlement != EntitlementState::NotOwned
        && row
            .verified_at
            .is_some_and(|v| now_ms.saturating_sub(v) <= OFFLINE_GRACE_MS)
}

/// 起動の前の所有権の判定（spec 4.4 の表）。
fn check_ownership(row: &AccountRow) -> Result<(), CoreError> {
    match row.entitlement {
        EntitlementState::NotOwned => Err(CoreError::NotOwned),
        EntitlementState::Unknown if row.verified_at.is_none() => Err(CoreError::OwnershipUnknown),
        _ => Ok(()),
    }
}

/// 資格情報ストアのキーと、エラーに入れるアカウントの ID（ハイフンなし）。
fn key(id: Uuid) -> String {
    id.simple().to_string()
}

/// 連鎖の途中の HTTP 401（トークンが拒否された）。
fn unauthorized(e: &CoreError) -> bool {
    matches!(e, CoreError::HttpStatus { status: 401, .. })
}

fn login_required() -> CoreError {
    CoreError::LoginRequired {
        account_id: String::new(),
    }
}

/// メモリだけに持つ、アカウントごとの状態。
#[derive(Default)]
struct Cached {
    /// 資格情報ストアに保存できなかった、新しい refresh token。
    refresh: Option<SecretString>,
    /// ログインを保存できていない（ランチャーを閉じるまでの間だけ有効）。
    unsaved: bool,
    mc: Option<McToken>,
}

/// トークンの連鎖の結果。
struct SignedIn {
    mc: McToken,
    xuid: Option<String>,
    entitlement: EntitlementState,
    profile: minecraft::Profile,
}

#[derive(Clone)]
pub struct AuthService {
    db: Db,
    paths: AppPaths,
    http: AuthHttp,
    urls: AuthUrls,
    store: Option<Arc<dyn SecretStore>>,
    client_id: Option<String>,
    clock: Clock,
    on_change: Option<ChangeFn>,
    cache: Arc<Mutex<HashMap<Uuid, Cached>>>,
    locks: Arc<Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>>,
}

impl AuthService {
    /// `store` が None なら、資格情報ストアを使えない環境として、ログインをメモリだけに持つ。
    pub fn new(
        db: Db,
        paths: AppPaths,
        auth_http: AuthHttp,
        urls: AuthUrls,
        store: Option<Arc<dyn SecretStore>>,
        client_id: Option<String>,
    ) -> AuthService {
        AuthService {
            db,
            paths,
            http: auth_http,
            urls,
            store,
            client_id,
            clock: Arc::new(now_ms),
            on_change: None,
            cache: Arc::default(),
            locks: Arc::default(),
        }
    }

    /// 時刻の取り方を差し替える（テスト用）。
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// アカウントの一覧が変わったときに呼ぶ関数を設定する。
    pub fn with_on_change(mut self, on_change: ChangeFn) -> Self {
        self.on_change = Some(on_change);
        self
    }

    /// アカウントの一覧が変わったことを知らせる。開発用のオフラインの切り替えも含む。
    pub fn notify_changed(&self) {
        if let Some(f) = &self.on_change {
            f();
        }
    }

    /// Microsoft のクライアント ID。未設定なら None。
    pub fn client_id(&self) -> Option<&str> {
        self.client_id.as_deref()
    }

    pub(super) fn http(&self) -> &AuthHttp {
        &self.http
    }

    pub(super) fn urls(&self) -> &AuthUrls {
        &self.urls
    }

    pub(super) fn now(&self) -> i64 {
        (self.clock)()
    }

    /// 起動に使うセッションを用意する。Minecraft のトークンが有効ならそのまま使い、
    /// 期限まで 5 分を切っていれば refresh token から連鎖をやり直す。
    pub async fn session_for_launch(&self) -> Result<LaunchSession, CoreError> {
        #[cfg(debug_assertions)]
        if session::dev_offline_active(&self.db)? {
            return Ok(session::offline_session(&session::load_dev_username(
                &self.db,
            )?));
        }
        let id = accounts::active(&self.db)?
            .ok_or(CoreError::AuthNoAccount)?
            .id;
        let mc = self.minecraft_token(id).await?;
        let row = accounts::get(&self.db, id)?;
        check_ownership(&row)?;
        Ok(LaunchSession {
            uuid: row.id,
            username: row.username,
            access_token: mc.access,
            xuid: row.xuid,
            user_type: UserType::Msa,
        })
    }

    /// 通信せずに、`accounts` に保存してある UUID、名前、XUID で遊ぶ（spec 4.7）。
    pub fn offline_session(&self) -> Result<LaunchSession, CoreError> {
        #[cfg(debug_assertions)]
        if session::dev_offline_active(&self.db)? {
            return Ok(session::offline_session(&session::load_dev_username(
                &self.db,
            )?));
        }
        let row = accounts::active(&self.db)?.ok_or(CoreError::AuthNoAccount)?;
        if !offline_allowed(&row, self.now()) {
            return Err(CoreError::AuthOffline {
                offline_available: false,
            });
        }
        Ok(LaunchSession {
            uuid: row.id,
            username: row.username,
            access_token: SecretString::from("0".to_string()),
            xuid: row.xuid,
            user_type: UserType::Msa,
        })
    }

    /// 使用中のアカウントのセッションを裏で 1 回だけ用意する。失敗は捨てる
    /// （ログインし直しが必要なら、その印だけが残る）。
    pub async fn warm_up(&self) {
        if let Err(e) = self.session_for_launch().await {
            tracing::debug!(code = e.code(), "session warm-up did not complete");
        }
    }

    /// Microsoft のトークンからアカウントを追加し、使用中にする。
    /// Java 版を所有していなければ、何も保存せずに `NotOwned` を返す。
    ///
    /// 通信の間（トークンの連鎖とロックの待ち）は `cancel` で打ち切れる（`Cancelled`、何も保存しない）。
    /// 保存を始めたら、途中で打ち切らずに最後まで書く。
    pub async fn complete_login(
        &self,
        tokens: MsTokens,
        progress: ProgressFn<'_>,
        cancel: &CancellationToken,
    ) -> Result<Uuid, CoreError> {
        let signed = tokio::select! {
            _ = cancel.cancelled() => return Err(CoreError::Cancelled),
            r = self.sign_in(&tokens.access, progress) => r,
        }
        .map_err(|e| match e {
            CoreError::Offline { .. } => CoreError::AuthOffline {
                offline_available: false,
            },
            other => other,
        })?;
        if signed.entitlement == EntitlementState::NotOwned {
            return Err(CoreError::NotOwned);
        }
        let id = signed.profile.id;
        let lock = self.lock_for(id);
        let _guard = tokio::select! {
            _ = cancel.cancelled() => return Err(CoreError::Cancelled),
            g = lock.lock() => g,
        };
        // ここから先は保存を伴うため、打ち切らない
        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        self.record(&signed)?;
        match tokens.refresh {
            Some(refresh) => self.keep_refresh(id, refresh).await,
            None => {
                tracing::warn!(
                    "sign-in returned no refresh token; it lasts until the launcher closes"
                );
                let mut cache = self.cache.lock().unwrap();
                let entry = cache.entry(id).or_default();
                entry.refresh = None;
                entry.unsaved = true;
            }
        }
        accounts::set_active(&self.db, id)?;
        self.cache.lock().unwrap().entry(id).or_default().mc = Some(signed.mc);
        self.notify_changed();
        Ok(id)
    }

    pub fn list(&self) -> Result<AccountList, CoreError> {
        let rows = accounts::list(&self.db)?;
        #[cfg(debug_assertions)]
        let dev_offline = Some(DevOfflineAccount {
            username: session::load_dev_username(&self.db)?,
            active: session::dev_offline_active(&self.db)?,
        });
        #[cfg(not(debug_assertions))]
        let dev_offline: Option<DevOfflineAccount> = None;
        let dev_active = dev_offline.as_ref().is_some_and(|d| d.active);
        let cache = self.cache.lock().unwrap();
        let accounts = rows
            .into_iter()
            .map(|r| Account {
                id: key(r.id),
                is_active: r.is_active && !dev_active,
                entitlement: r.entitlement,
                needs_login: r.needs_login,
                login_saved: self.store.is_some() && !cache.get(&r.id).is_some_and(|c| c.unsaved),
                has_skin: r.skin_url.is_some(),
                username: r.username,
            })
            .collect();
        Ok(AccountList {
            accounts,
            dev_offline,
        })
    }

    pub fn set_active(&self, id: Uuid) -> Result<(), CoreError> {
        accounts::set_active(&self.db, id)?;
        self.notify_changed();
        Ok(())
    }

    /// `accounts` の行、資格情報ストアの refresh token、メモリのトークンを消す。
    /// 実行中のゲームの確認は、呼び出し側（`Launcher`）が先に行う。
    pub async fn remove(&self, id: Uuid) -> Result<(), CoreError> {
        let lock = self.lock_for(id);
        let _guard = lock.lock().await;
        accounts::remove(&self.db, id)?;
        self.notify_changed();
        self.cache.lock().unwrap().remove(&id);
        if let Some(store) = self.store.clone() {
            let k = key(id);
            match tokio::task::spawn_blocking(move || store.delete(&k)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!(error = %e, "could not delete the saved sign-in"),
                Err(e) => tracing::warn!(error = %e, "credential store task failed"),
            }
        }
        Ok(())
    }

    /// スキンの画像のキャッシュのパス。スキンがない、または取得できなければ None。
    pub async fn skin_path(&self, id: Uuid) -> Result<Option<PathBuf>, CoreError> {
        let Some(url) = accounts::get(&self.db, id)?.skin_url else {
            return Ok(None);
        };
        match minecraft::cached_skin(&self.paths, &url).await {
            Ok(path) => Ok(Some(path)),
            Err(e) => {
                tracing::warn!(code = e.code(), "could not fetch the skin");
                Ok(None)
            }
        }
    }

    // ---- 内部 ----

    fn lock_for(&self, id: Uuid) -> Arc<tokio::sync::Mutex<()>> {
        self.locks.lock().unwrap().entry(id).or_default().clone()
    }

    /// 期限まで REFRESH_MARGIN_MS 以上残っている、メモリの Minecraft のトークン。
    fn fresh_mc(&self, id: Uuid) -> Option<McToken> {
        let now = self.now();
        let cache = self.cache.lock().unwrap();
        cache
            .get(&id)
            .and_then(|c| c.mc.as_ref())
            .filter(|t| t.expires_at_ms.saturating_sub(now) >= REFRESH_MARGIN_MS)
            .cloned()
    }

    /// 有効な Minecraft のトークンを返す。なければアカウントのロックを取って取り直す。
    async fn minecraft_token(&self, id: Uuid) -> Result<McToken, CoreError> {
        if let Some(t) = self.fresh_mc(id) {
            return Ok(t);
        }
        let lock = self.lock_for(id);
        let _guard = lock.lock().await;
        // 待っている間に、先の呼び出しが取り直したかもしれない
        if let Some(t) = self.fresh_mc(id) {
            return Ok(t);
        }
        // HTTP 401 は、連鎖を最初からやり直して 1 回だけ再試行する（spec 4.5）
        let result = match self.renew(id).await {
            Err(e) if unauthorized(&e) => self.renew(id).await,
            other => other,
        };
        result.map_err(|e| self.user_facing(id, e))
    }

    /// 連鎖の失敗を、UI に見せるエラーに変える。
    fn user_facing(&self, id: Uuid, err: CoreError) -> CoreError {
        match err {
            CoreError::Offline { .. } => CoreError::AuthOffline {
                offline_available: accounts::get(&self.db, id)
                    .is_ok_and(|row| offline_allowed(&row, self.now())),
            },
            CoreError::LoginRequired { .. } | CoreError::HttpStatus { status: 401, .. } => {
                match accounts::set_needs_login(&self.db, id, true) {
                    Ok(()) => self.notify_changed(),
                    Err(e) => {
                        tracing::warn!(code = e.code(), "could not mark the account for sign-in");
                    }
                }
                CoreError::LoginRequired {
                    account_id: key(id),
                }
            }
            other => other,
        }
    }

    /// refresh token から連鎖をやり直し、`accounts` とメモリを更新する。
    async fn renew(&self, id: Uuid) -> Result<McToken, CoreError> {
        let (refresh, unsaved) = self.refresh_token(id).await.ok_or_else(login_required)?;
        let client_id = self.client_id.as_deref().ok_or(CoreError::NoClientId)?;
        let ms =
            microsoft::refresh(&self.http, &self.urls, client_id, &refresh, self.now()).await?;
        // 新しい refresh token か、まだ保存できていないメモリの refresh token を保存し直す
        if let Some(token) = ms.refresh.or(unsaved.then_some(refresh)) {
            self.keep_refresh(id, token).await;
        }
        let signed = self.sign_in(&ms.access, &|_| {}).await?;
        if signed.profile.id != id {
            tracing::warn!("the refreshed sign-in belongs to a different profile");
            return Err(login_required());
        }
        self.record(&signed)?;
        self.cache.lock().unwrap().entry(id).or_default().mc = Some(signed.mc.clone());
        Ok(signed.mc)
    }

    /// Microsoft のアクセストークンから、Xbox、XSTS、Minecraft、所有権、プロフィールまで進める。
    async fn sign_in(
        &self,
        ms_access: &SecretString,
        progress: ProgressFn<'_>,
    ) -> Result<SignedIn, CoreError> {
        progress(LoginProgress::CheckingXbox);
        let user = xbox::user_token(&self.http, &self.urls, ms_access).await?;
        let xsts = xbox::xsts(&self.http, &self.urls, &user).await?;
        progress(LoginProgress::CheckingMinecraft);
        let mc = minecraft::login_with_xbox(&self.http, &self.urls, &xsts, self.now()).await?;
        let (entitlement, profile) = tokio::join!(
            minecraft::entitlement(&self.http, &self.urls, &mc),
            minecraft::profile(&self.http, &self.urls, &mc),
        );
        Ok(SignedIn {
            mc,
            xuid: xsts.xuid,
            entitlement,
            profile: profile?,
        })
    }

    /// プロフィールと所有権を `accounts` に書く（`needs_login` は 0 に戻る）。
    fn record(&self, s: &SignedIn) -> Result<(), CoreError> {
        let record = ProfileRecord {
            id: s.profile.id,
            username: s.profile.name.clone(),
            xuid: s.xuid.clone(),
            skin_url: s.profile.skin_url.clone(),
        };
        accounts::save_profile(&self.db, &record, s.entitlement, self.now())?;
        self.notify_changed();
        Ok(())
    }

    /// 使う refresh token と、それがメモリだけにある（保存できていない）か。
    /// 保存に失敗してメモリに持っているものを優先する。
    async fn refresh_token(&self, id: Uuid) -> Option<(SecretString, bool)> {
        let unsaved = self
            .cache
            .lock()
            .unwrap()
            .get(&id)
            .and_then(|c| c.refresh.clone());
        if let Some(token) = unsaved {
            return Some((token, true));
        }
        let store = self.store.clone()?;
        let k = key(id);
        match tokio::task::spawn_blocking(move || store.get(&k)).await {
            Ok(Ok(token)) => token.map(|t| (t, false)),
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "could not read the saved sign-in");
                None
            }
            Err(e) => {
                tracing::warn!(error = %e, "credential store task failed");
                None
            }
        }
    }

    /// 新しい refresh token を保存する。保存できなければメモリに持ち、古いものは消さない。
    async fn keep_refresh(&self, id: Uuid, token: SecretString) {
        let saved = match self.store.clone() {
            Some(store) => {
                let (k, t) = (key(id), token.clone());
                match tokio::task::spawn_blocking(move || store.set(&k, &t)).await {
                    Ok(Ok(())) => true,
                    Ok(Err(e)) => {
                        tracing::warn!(error = %e, "could not save the sign-in; it lasts until the launcher closes");
                        false
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "credential store task failed");
                        false
                    }
                }
            }
            None => false,
        };
        let mut cache = self.cache.lock().unwrap();
        let entry = cache.entry(id).or_default();
        entry.refresh = (!saved).then_some(token);
        entry.unsaved = !saved;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>(_: &T) {}

    #[tokio::test]
    async fn on_change_fires_on_set_active_and_remove() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let db = Db::open_in_memory().unwrap();
        let auth = AuthService::new(
            db.clone(),
            AppPaths::from_root("unused"),
            AuthHttp::new().unwrap(),
            AuthUrls::default(),
            None,
            None,
        )
        .with_on_change(Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        let id = Uuid::from_u128(1);
        let record = ProfileRecord {
            id,
            username: "A".into(),
            xuid: None,
            skin_url: None,
        };
        accounts::save_profile(&db, &record, EntitlementState::Verified, 1).unwrap();
        auth.set_active(id).unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        // 失敗したときは知らせない
        assert!(auth.set_active(Uuid::from_u128(2)).is_err());
        assert_eq!(count.load(Ordering::SeqCst), 1);
        auth.remove(id).await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    /// ログインと起動はタスク（Send が必要）の中で動くため、各 future は Send でなければならない。
    #[test]
    fn futures_are_send() {
        let auth = AuthService::new(
            Db::open_in_memory().unwrap(),
            AppPaths::from_root("unused"),
            AuthHttp::new().unwrap(),
            AuthUrls::default(),
            None,
            None,
        );
        let id = Uuid::nil();
        let tokens = MsTokens {
            access: SecretString::from("a".to_string()),
            refresh: None,
            expires_at_ms: 0,
        };
        assert_send(&auth.session_for_launch());
        assert_send(&auth.warm_up());
        let cancel = CancellationToken::new();
        assert_send(&auth.complete_login(tokens, &|_| {}, &cancel));
        assert_send(&auth.remove(id));
        assert_send(&auth.skin_path(id));
    }
}
