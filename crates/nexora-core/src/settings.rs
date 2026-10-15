//! アプリ全体の設定。app_settings テーブルに JSON の文字列として保存する。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::Db;
use crate::error::CoreError;

const KEY_THEME: &str = "theme";
const KEY_LANGUAGE: &str = "language";
const KEY_DOWNLOADS: &str = "max_concurrent_downloads";
const DOWNLOADS_RANGE: std::ops::RangeInclusive<u8> = 1..=16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Theme {
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Language {
    Ja,
    En,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Settings {
    pub theme: Theme,
    pub language: Option<Language>,
    pub max_concurrent_downloads: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            language: None,
            max_concurrent_downloads: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "key", content = "value", rename_all = "camelCase")]
#[ts(export)]
pub enum SettingChange {
    Theme(Theme),
    Language(Language),
    MaxConcurrentDownloads(u8),
}

pub fn load(db: &Db) -> Result<Settings, CoreError> {
    let rows: Vec<(String, String)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT key, value FROM app_settings")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;

    let mut settings = Settings::default();
    for (key, raw) in rows {
        match key.as_str() {
            KEY_THEME => match serde_json::from_str(&raw) {
                Ok(theme) => settings.theme = theme,
                Err(_) => tracing::warn!(key, "ignoring invalid stored setting"),
            },
            KEY_LANGUAGE => match serde_json::from_str(&raw) {
                Ok(language) => settings.language = Some(language),
                Err(_) => tracing::warn!(key, "ignoring invalid stored setting"),
            },
            KEY_DOWNLOADS => match serde_json::from_str::<u8>(&raw) {
                Ok(n) if DOWNLOADS_RANGE.contains(&n) => settings.max_concurrent_downloads = n,
                _ => tracing::warn!(key, "ignoring invalid stored setting"),
            },
            _ => {}
        }
    }
    Ok(settings)
}

pub fn apply(db: &Db, change: SettingChange, now_ms: i64) -> Result<Settings, CoreError> {
    let (key, value) = match change {
        SettingChange::Theme(theme) => (KEY_THEME, serde_json::to_string(&theme)),
        SettingChange::Language(language) => (KEY_LANGUAGE, serde_json::to_string(&language)),
        SettingChange::MaxConcurrentDownloads(n) => {
            if !DOWNLOADS_RANGE.contains(&n) {
                return Err(CoreError::InvalidSetting {
                    key: "maxConcurrentDownloads".into(),
                });
            }
            (KEY_DOWNLOADS, serde_json::to_string(&n))
        }
    };
    let value = value.map_err(|e| CoreError::Unexpected {
        message: e.to_string(),
    })?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![key, value, now_ms],
        )?;
        Ok(())
    })?;
    load(db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn defaults_when_empty() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(load(&db).unwrap(), Settings::default());
        assert_eq!(Settings::default().theme, Theme::System);
        assert_eq!(Settings::default().language, None);
        assert_eq!(Settings::default().max_concurrent_downloads, 8);
    }

    #[test]
    fn apply_round_trips() {
        let db = Db::open_in_memory().unwrap();
        apply(&db, SettingChange::Theme(Theme::Dark), 1).unwrap();
        apply(&db, SettingChange::Language(Language::En), 2).unwrap();
        let after = apply(&db, SettingChange::MaxConcurrentDownloads(4), 3).unwrap();
        assert_eq!(after.theme, Theme::Dark);
        assert_eq!(after.language, Some(Language::En));
        assert_eq!(after.max_concurrent_downloads, 4);
        assert_eq!(load(&db).unwrap(), after);
    }

    #[test]
    fn rejects_out_of_range_downloads() {
        let db = Db::open_in_memory().unwrap();
        for bad in [0u8, 17] {
            let err = apply(&db, SettingChange::MaxConcurrentDownloads(bad), 1).unwrap_err();
            assert_eq!(err.code(), "settings.invalid_value");
        }
    }

    #[test]
    fn invalid_stored_values_fall_back_to_defaults() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute_batch(
                "INSERT INTO app_settings VALUES ('theme', '\"purple\"', 1);
                 INSERT INTO app_settings VALUES ('max_concurrent_downloads', '999', 1);
                 INSERT INTO app_settings VALUES ('language', 'not json', 1);",
            )?;
            Ok(())
        })
        .unwrap();
        assert_eq!(load(&db).unwrap(), Settings::default());
    }

    #[test]
    fn change_json_shape() {
        let change: SettingChange =
            serde_json::from_str(r#"{"key":"maxConcurrentDownloads","value":6}"#).unwrap();
        assert_eq!(change, SettingChange::MaxConcurrentDownloads(6));
        let change: SettingChange =
            serde_json::from_str(r#"{"key":"theme","value":"light"}"#).unwrap();
        assert_eq!(change, SettingChange::Theme(Theme::Light));
    }
}
