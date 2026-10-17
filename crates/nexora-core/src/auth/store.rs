//! refresh token の保存先。OS の資格情報ストアを包む `SecretStore` と、テスト用のメモリ実装。
//!
//! 各メソッドは OS の API を同期で呼ぶためブロックする。async の呼び出し側は
//! `spawn_blocking` で包むこと。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use keyring_core::Entry;
use keyring_core::api::CredentialStore;
use secrecy::{ExposeSecret, SecretString};

use crate::paths::Profile;

/// 資格情報ストアのエラー。`message` に秘密の値は含めない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError {
    pub message: String,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for StoreError {}

impl StoreError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// refresh token などの秘密を保存する場所。
pub trait SecretStore: Send + Sync {
    /// エントリがなければ `Ok(None)`。
    fn get(&self, key: &str) -> Result<Option<SecretString>, StoreError>;
    fn set(&self, key: &str, value: &SecretString) -> Result<(), StoreError>;
    /// エントリがなくても成功する。
    fn delete(&self, key: &str) -> Result<(), StoreError>;
}

/// サービス名。製品版と開発版で資格情報を分ける。
fn service_name(profile: Profile) -> &'static str {
    match profile {
        Profile::Release => "net.ny4n.nexora",
        Profile::Dev => "net.ny4n.nexora.dev",
    }
}

/// keyring-core のエラーを、秘密を含まない文言にする。
fn map_error(err: keyring_core::Error) -> StoreError {
    use keyring_core::Error;
    match err {
        // 取得した中身を持つ種類は、中身に触れない固定の文言にする。
        Error::BadEncoding(_) => StoreError::new("stored secret is not valid UTF-8"),
        Error::BadDataFormat(..) => StoreError::new("stored secret has an unexpected format"),
        other => StoreError::new(other.to_string()),
    }
}

/// OS の資格情報ストア（Windows 資格情報マネージャー、macOS キーチェーン、Secret Service）。
///
/// グローバルの既定ストア（`set_default_store`）は使わず、自分で持つ `Store` から
/// エントリを作る。プロセス内の他のコードと干渉しないため。
pub struct KeyringStore {
    store: Arc<CredentialStore>,
    service: &'static str,
}

impl KeyringStore {
    pub fn new(profile: Profile) -> Result<Self, StoreError> {
        Ok(Self {
            store: platform_store()?,
            service: service_name(profile),
        })
    }

    fn entry(&self, key: &str) -> Result<Entry, StoreError> {
        self.store
            .build(self.service, key, entry_modifiers().as_ref())
            .map_err(map_error)
    }
}

/// Windows は永続化を Local にする（既定は Enterprise でドメインのローミング対象になる）。
#[cfg(windows)]
fn entry_modifiers() -> Option<HashMap<&'static str, &'static str>> {
    Some(HashMap::from([("persistence", "Local")]))
}

#[cfg(not(windows))]
fn entry_modifiers() -> Option<HashMap<&'static str, &'static str>> {
    None
}

#[cfg(windows)]
fn platform_store() -> Result<Arc<CredentialStore>, StoreError> {
    let store = windows_native_keyring_store::Store::new().map_err(map_error)?;
    Ok(store)
}

#[cfg(target_os = "macos")]
fn platform_store() -> Result<Arc<CredentialStore>, StoreError> {
    // 既定はログインキーチェーン（User）。
    let store = apple_native_keyring_store::keychain::Store::new().map_err(map_error)?;
    Ok(store)
}

#[cfg(target_os = "linux")]
fn platform_store() -> Result<Arc<CredentialStore>, StoreError> {
    // Secret Service に接続できなければここで失敗する。
    let store = zbus_secret_service_keyring_store::Store::new().map_err(map_error)?;
    Ok(store)
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn platform_store() -> Result<Arc<CredentialStore>, StoreError> {
    Err(StoreError::new("no credential store for this platform"))
}

/// Windows の資格情報 1 件に入る blob の上限（`CRED_MAX_CREDENTIAL_BLOB_SIZE`）。テストで使う。
#[cfg(test)]
const WINDOWS_BLOB_LIMIT: usize = 2560;

/// ストアに書くバイト列。UTF-8 のまま渡す。
#[cfg(any(windows, test))]
fn secret_bytes(value: &SecretString) -> &[u8] {
    value.expose_secret().as_bytes()
}

/// ストアから読んだバイト列を UTF-8 として解く。失敗しても中身は文言に含めない。
#[cfg(any(windows, test))]
fn secret_from_bytes(bytes: Vec<u8>) -> Result<SecretString, StoreError> {
    use secrecy::zeroize::Zeroize;
    match String::from_utf8(bytes) {
        Ok(s) => Ok(SecretString::from(s)),
        Err(e) => {
            e.into_bytes().zeroize();
            Err(StoreError::new("stored secret is not valid UTF-8"))
        }
    }
}

impl SecretStore for KeyringStore {
    // Windows では `set_password` が値を UTF-16LE にして保存するため、2,560 バイトの上限に
    // 約 1,280 文字しか入らない。refresh token はこれを超えることがあるので、UTF-8 の
    // バイト列のまま `set_secret` / `get_secret` で読み書きする。
    #[cfg(windows)]
    fn get(&self, key: &str) -> Result<Option<SecretString>, StoreError> {
        match self.entry(key)?.get_secret() {
            Ok(bytes) => secret_from_bytes(bytes).map(Some),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_error(e)),
        }
    }

    #[cfg(windows)]
    fn set(&self, key: &str, value: &SecretString) -> Result<(), StoreError> {
        self.entry(key)?
            .set_secret(secret_bytes(value))
            .map_err(map_error)
    }

    #[cfg(not(windows))]
    fn get(&self, key: &str) -> Result<Option<SecretString>, StoreError> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(Some(SecretString::from(v))),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_error(e)),
        }
    }

    #[cfg(not(windows))]
    fn set(&self, key: &str, value: &SecretString) -> Result<(), StoreError> {
        self.entry(key)?
            .set_password(value.expose_secret())
            .map_err(map_error)
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_error(e)),
        }
    }
}

/// テスト用のメモリ実装。結合テストからも使うため `pub` で置く。
#[doc(hidden)]
#[derive(Default)]
pub struct MemoryStore {
    map: Mutex<HashMap<String, String>>,
    fail_writes: AtomicBool,
}

impl MemoryStore {
    /// `true` の間、`set` と `delete` が失敗する（値は変わらない）。
    pub fn fail_writes(&self, on: bool) {
        self.fail_writes.store(on, Ordering::SeqCst);
    }

    pub fn value(&self, key: &str) -> Option<String> {
        self.map.lock().unwrap().get(key).cloned()
    }

    fn check_writable(&self) -> Result<(), StoreError> {
        if self.fail_writes.load(Ordering::SeqCst) {
            Err(StoreError::new("memory store: write failed"))
        } else {
            Ok(())
        }
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, key: &str) -> Result<Option<SecretString>, StoreError> {
        Ok(self.value(key).map(SecretString::from))
    }

    fn set(&self, key: &str, value: &SecretString) -> Result<(), StoreError> {
        self.check_writable()?;
        self.map
            .lock()
            .unwrap()
            .insert(key.to_string(), value.expose_secret().to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.check_writable()?;
        self.map.lock().unwrap().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(s: &str) -> SecretString {
        SecretString::from(s.to_string())
    }

    #[test]
    fn memory_store_round_trips_and_deletes() {
        let s = MemoryStore::default();
        assert!(s.get("a").unwrap().is_none());
        s.set("a", &secret("v1")).unwrap();
        assert_eq!(s.get("a").unwrap().unwrap().expose_secret(), "v1");
        s.delete("a").unwrap();
        assert!(s.get("a").unwrap().is_none());
    }

    #[test]
    fn memory_store_can_fail_writes_and_keeps_the_old_value() {
        let s = MemoryStore::default();
        s.set("a", &secret("old")).unwrap();
        s.fail_writes(true);
        assert!(s.set("a", &secret("new")).is_err());
        assert!(s.delete("a").is_err());
        assert_eq!(s.value("a").as_deref(), Some("old"));
        s.fail_writes(false);
        s.set("a", &secret("new")).unwrap();
        assert_eq!(s.value("a").as_deref(), Some("new"));
    }

    #[test]
    fn deleting_a_missing_entry_succeeds() {
        MemoryStore::default().delete("none").unwrap();
    }

    #[test]
    fn a_2000_char_token_fits_the_windows_blob_limit() {
        let token = secret(&"a".repeat(2000));
        assert!(secret_bytes(&token).len() <= WINDOWS_BLOB_LIMIT);
    }

    #[test]
    fn stored_bytes_round_trip_as_utf8() {
        let token = secret("M.C5_\u{3042}_token");
        let back = secret_from_bytes(secret_bytes(&token).to_vec()).unwrap();
        assert_eq!(back.expose_secret(), token.expose_secret());
    }

    #[test]
    fn invalid_utf8_gives_a_fixed_message_without_the_bytes() {
        let err = secret_from_bytes(vec![0x41, 0xff, 0xfe]).unwrap_err();
        assert_eq!(err.message, "stored secret is not valid UTF-8");
    }

    #[test]
    fn service_names_match_the_profile() {
        assert_eq!(service_name(Profile::Release), "net.ny4n.nexora");
        assert_eq!(service_name(Profile::Dev), "net.ny4n.nexora.dev");
    }

    #[test]
    #[ignore = "OS の資格情報ストアに触れるため、手動で実行する"]
    fn keyring_store_round_trips() {
        let s = KeyringStore::new(Profile::Dev).unwrap();
        let key = format!(
            "nexora-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        assert!(s.get(&key).unwrap().is_none());
        s.set(&key, &secret("v1")).unwrap();
        s.set(&key, &secret("v2")).unwrap();
        assert_eq!(s.get(&key).unwrap().unwrap().expose_secret(), "v2");
        let long = "a".repeat(2000);
        s.set(&key, &secret(&long)).unwrap();
        assert_eq!(s.get(&key).unwrap().unwrap().expose_secret(), long);
        s.delete(&key).unwrap();
        assert!(s.get(&key).unwrap().is_none());
        s.delete(&key).unwrap();
    }
}
