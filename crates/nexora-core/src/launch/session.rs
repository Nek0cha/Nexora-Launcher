//! 起動用セッション。開発ビルドだけ、オフラインアカウントを作れる。

use secrecy::SecretString;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserType {
    Msa,
    Offline,
}

impl UserType {
    /// 起動引数の ${user_type} の値。
    pub fn arg_value(self) -> &'static str {
        match self {
            UserType::Msa => "msa",
            UserType::Offline => "legacy",
        }
    }
}

/// 起動処理に渡す認証の結果。フェーズ 3 では Microsoft 認証から同じ型を作る。
#[derive(Debug, Clone)]
pub struct LaunchSession {
    pub uuid: Uuid,
    pub username: String,
    pub access_token: SecretString,
    pub xuid: Option<String>,
    pub user_type: UserType,
}

#[cfg(debug_assertions)]
mod dev {
    use md5::{Digest, Md5};
    use rusqlite::{OptionalExtension, params};
    use secrecy::SecretString;
    use uuid::Uuid;

    use super::{LaunchSession, UserType};
    use crate::db::Db;
    use crate::error::CoreError;

    pub const DEV_USERNAME_KEY: &str = "dev.offline_username";
    pub const DEFAULT_DEV_USERNAME: &str = "Player";

    /// Vanilla のサーバーのオフラインモードと同じく、"OfflinePlayer:<名前>" の MD5 から v3 の UUID を作る。
    pub fn offline_uuid(name: &str) -> Uuid {
        let digest = Md5::digest(format!("OfflinePlayer:{name}").as_bytes());
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest);
        uuid::Builder::from_md5_bytes(bytes).into_uuid()
    }

    pub fn offline_session(username: &str) -> LaunchSession {
        LaunchSession {
            uuid: offline_uuid(username),
            username: username.to_string(),
            access_token: SecretString::from("0".to_string()),
            xuid: None,
            user_type: UserType::Offline,
        }
    }

    fn valid(name: &str) -> bool {
        (1..=16).contains(&name.len())
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    pub fn load_dev_username(db: &Db) -> Result<String, CoreError> {
        let raw: Option<String> = db.with_conn(|conn| {
            Ok(conn
                .query_row(
                    "SELECT value FROM app_settings WHERE key = ?1",
                    [DEV_USERNAME_KEY],
                    |r| r.get(0),
                )
                .optional()?)
        })?;
        Ok(raw
            .and_then(|r| serde_json::from_str::<String>(&r).ok())
            .filter(|n| valid(n))
            .unwrap_or_else(|| DEFAULT_DEV_USERNAME.to_string()))
    }

    pub fn save_dev_username(db: &Db, name: &str, now_ms: i64) -> Result<String, CoreError> {
        if !valid(name) {
            return Err(CoreError::InvalidSetting {
                key: "devOfflineUsername".into(),
            });
        }
        let value = serde_json::to_string(name).map_err(|e| CoreError::Unexpected {
            message: e.to_string(),
        })?;
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![DEV_USERNAME_KEY, value, now_ms],
            )?;
            Ok(())
        })?;
        Ok(name.to_string())
    }
}

#[cfg(debug_assertions)]
pub use dev::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use secrecy::ExposeSecret;

    #[test]
    fn offline_uuid_matches_vanilla_servers() {
        // Java の UUID.nameUUIDFromBytes("OfflinePlayer:Notch") と同じ値
        assert_eq!(
            offline_uuid("Notch").to_string(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
        assert_eq!(offline_uuid("Player").get_version_num(), 3);
        assert_eq!(offline_uuid("Player"), offline_uuid("Player"));
        assert_ne!(offline_uuid("Player"), offline_uuid("player"));
    }

    #[test]
    fn offline_session_shape() {
        let s = offline_session("Steve");
        assert_eq!(s.username, "Steve");
        assert_eq!(s.uuid, offline_uuid("Steve"));
        assert_eq!(s.access_token.expose_secret(), "0");
        assert_eq!(s.user_type, UserType::Offline);
        assert_eq!(s.user_type.arg_value(), "legacy");
        assert_eq!(s.xuid, None);
    }

    #[test]
    fn dev_username_defaults_and_round_trips() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(load_dev_username(&db).unwrap(), "Player");
        assert_eq!(save_dev_username(&db, "Alex_01", 1).unwrap(), "Alex_01");
        assert_eq!(load_dev_username(&db).unwrap(), "Alex_01");
    }

    #[test]
    fn dev_username_is_validated() {
        let db = Db::open_in_memory().unwrap();
        for bad in ["", "has space", "日本語", "x".repeat(17).as_str()] {
            let err = save_dev_username(&db, bad, 1).unwrap_err();
            assert_eq!(err.code(), "settings.invalid_value", "{bad}");
        }
    }
}
