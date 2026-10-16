//! `accounts` テーブルの読み書き。トークンは持たない（refresh token は SecretStore だけ）。

#[cfg(debug_assertions)]
use rusqlite::Connection;
use rusqlite::{OptionalExtension, Row, params};
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::db::Db;
use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EntitlementState {
    Verified,
    NotOwned,
    Unknown,
}

impl EntitlementState {
    fn as_str(self) -> &'static str {
        match self {
            EntitlementState::Verified => "verified",
            EntitlementState::NotOwned => "not_owned",
            EntitlementState::Unknown => "unknown",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "verified" => Some(EntitlementState::Verified),
            "not_owned" => Some(EntitlementState::NotOwned),
            "unknown" => Some(EntitlementState::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AccountRow {
    #[ts(type = "string")]
    pub id: Uuid,
    pub username: String,
    pub xuid: Option<String>,
    pub skin_url: Option<String>,
    pub is_active: bool,
    pub added_at: i64,
    pub profile_fetched_at: Option<i64>,
    pub entitlement: EntitlementState,
    pub entitlement_checked_at: Option<i64>,
    pub verified_at: Option<i64>,
    pub needs_login: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRecord {
    pub id: Uuid,
    pub username: String,
    pub xuid: Option<String>,
    pub skin_url: Option<String>,
}

const COLUMNS: &str = "id, username, xuid, skin_url, is_active, added_at, profile_fetched_at, \
                       entitlement_state, entitlement_checked_at, verified_at, needs_login";

/// DB には UUID をハイフンなしの 32 文字で保存する。
fn key(id: Uuid) -> String {
    id.simple().to_string()
}

fn bad_value(col: usize, what: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        col,
        rusqlite::types::Type::Text,
        what.to_string().into(),
    )
}

fn from_row(r: &Row<'_>) -> rusqlite::Result<AccountRow> {
    let id: String = r.get(0)?;
    let state: String = r.get(7)?;
    Ok(AccountRow {
        id: Uuid::parse_str(&id).map_err(|_| bad_value(0, "invalid account id"))?,
        username: r.get(1)?,
        xuid: r.get(2)?,
        skin_url: r.get(3)?,
        is_active: r.get::<_, i64>(4)? != 0,
        added_at: r.get(5)?,
        profile_fetched_at: r.get(6)?,
        entitlement: EntitlementState::parse(&state)
            .ok_or_else(|| bad_value(7, "invalid entitlement state"))?,
        entitlement_checked_at: r.get(8)?,
        verified_at: r.get(9)?,
        needs_login: r.get::<_, i64>(10)? != 0,
    })
}

fn not_found(id: Uuid) -> CoreError {
    CoreError::AccountNotFound {
        account_id: key(id),
    }
}

fn ensure_changed(changed: usize, id: Uuid) -> Result<(), CoreError> {
    if changed == 0 {
        Err(not_found(id))
    } else {
        Ok(())
    }
}

pub fn list(db: &Db) -> Result<Vec<AccountRow>, CoreError> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM accounts ORDER BY added_at ASC, id ASC"
        ))?;
        let rows = stmt
            .query_map([], from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
}

pub fn get(db: &Db, id: Uuid) -> Result<AccountRow, CoreError> {
    db.with_conn(|conn| {
        conn.query_row(
            &format!("SELECT {COLUMNS} FROM accounts WHERE id = ?1"),
            [key(id)],
            from_row,
        )
        .optional()?
        .ok_or_else(|| not_found(id))
    })
}

pub fn active(db: &Db) -> Result<Option<AccountRow>, CoreError> {
    db.with_conn(|conn| {
        Ok(conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM accounts WHERE is_active = 1"),
                [],
                from_row,
            )
            .optional()?)
    })
}

/// なければ追加し、あれば名前、XUID、スキン、取得時刻、所有権を更新する。`needs_login` は 0 に戻す。
/// 新しく追加した行は使用中にしない（切り替えは `set_active`）。
pub fn save_profile(
    db: &Db,
    p: &ProfileRecord,
    entitlement: EntitlementState,
    now: i64,
) -> Result<(), CoreError> {
    let verified_at = (entitlement == EntitlementState::Verified).then_some(now);
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO accounts (id, username, xuid, skin_url, is_active, added_at, \
                 profile_fetched_at, entitlement_state, entitlement_checked_at, verified_at, needs_login)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5, ?6, ?5, ?7, 0)
             ON CONFLICT(id) DO UPDATE SET
                 username = excluded.username,
                 xuid = excluded.xuid,
                 skin_url = excluded.skin_url,
                 profile_fetched_at = excluded.profile_fetched_at,
                 entitlement_state = excluded.entitlement_state,
                 entitlement_checked_at = excluded.entitlement_checked_at,
                 verified_at = COALESCE(excluded.verified_at, accounts.verified_at),
                 needs_login = 0",
            params![
                key(p.id),
                p.username,
                p.xuid,
                p.skin_url,
                now,
                entitlement.as_str(),
                verified_at
            ],
        )?;
        Ok(())
    })
}

/// 所有権の確認結果を記録する。`verified_at` は `Verified` のときだけ更新する。
pub fn record_entitlement(
    db: &Db,
    id: Uuid,
    e: EntitlementState,
    now: i64,
) -> Result<(), CoreError> {
    let verified_at = (e == EntitlementState::Verified).then_some(now);
    db.with_conn(|conn| {
        let changed = conn.execute(
            "UPDATE accounts SET entitlement_state = ?2, entitlement_checked_at = ?3,
                 verified_at = COALESCE(?4, verified_at)
             WHERE id = ?1",
            params![key(id), e.as_str(), now, verified_at],
        )?;
        ensure_changed(changed, id)
    })
}

/// 使用中のアカウントを切り替える。開発ビルドでは `dev.offline_active` も偽にする。
pub fn set_active(db: &Db, id: Uuid) -> Result<(), CoreError> {
    db.with_conn(|conn| {
        let tx = conn.transaction()?;
        let exists = tx
            .query_row(
                "SELECT 1 FROM accounts WHERE id = ?1",
                [key(id)],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Err(not_found(id));
        }
        tx.execute("UPDATE accounts SET is_active = 0 WHERE is_active = 1", [])?;
        tx.execute("UPDATE accounts SET is_active = 1 WHERE id = ?1", [key(id)])?;
        #[cfg(debug_assertions)]
        clear_dev_offline_active(&tx)?;
        tx.commit()?;
        Ok(())
    })
}

#[cfg(debug_assertions)]
fn clear_dev_offline_active(conn: &Connection) -> Result<(), CoreError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, 'false', ?2)
         ON CONFLICT(key) DO UPDATE SET value = 'false', updated_at = excluded.updated_at",
        params![crate::launch::session::DEV_OFFLINE_ACTIVE_KEY, now],
    )?;
    Ok(())
}

pub fn set_needs_login(db: &Db, id: Uuid, value: bool) -> Result<(), CoreError> {
    db.with_conn(|conn| {
        let changed = conn.execute(
            "UPDATE accounts SET needs_login = ?2 WHERE id = ?1",
            params![key(id), value as i64],
        )?;
        ensure_changed(changed, id)
    })
}

/// アカウントを削除する。使用中だった場合は、残りのうち最後に追加したものを使用中にする。
pub fn remove(db: &Db, id: Uuid) -> Result<(), CoreError> {
    db.with_conn(|conn| {
        let tx = conn.transaction()?;
        let was_active: Option<i64> = tx
            .query_row(
                "SELECT is_active FROM accounts WHERE id = ?1",
                [key(id)],
                |r| r.get(0),
            )
            .optional()?;
        let Some(was_active) = was_active else {
            return Err(not_found(id));
        };
        tx.execute("DELETE FROM accounts WHERE id = ?1", [key(id)])?;
        if was_active != 0 {
            tx.execute(
                "UPDATE accounts SET is_active = 1
                 WHERE id = (SELECT id FROM accounts ORDER BY added_at DESC, id DESC LIMIT 1)",
                [],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(n: u128, name: &str) -> ProfileRecord {
        ProfileRecord {
            id: Uuid::from_u128(n),
            username: name.into(),
            xuid: Some(format!("xuid{n}")),
            skin_url: Some(format!("https://example.invalid/{name}.png")),
        }
    }

    fn add(db: &Db, n: u128, name: &str, at: i64) -> Uuid {
        let p = profile(n, name);
        save_profile(db, &p, EntitlementState::Verified, at).unwrap();
        p.id
    }

    #[test]
    fn save_profile_adds_then_updates() {
        let db = Db::open_in_memory().unwrap();
        let id = add(&db, 1, "Steve", 1);
        let mut p = profile(1, "Alex");
        p.skin_url = Some("https://example.invalid/new.png".into());
        save_profile(&db, &p, EntitlementState::Verified, 5).unwrap();
        let rows = list(&db).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].username, "Alex");
        assert_eq!(
            rows[0].skin_url.as_deref(),
            Some("https://example.invalid/new.png")
        );
        assert_eq!(rows[0].added_at, 1);
        assert_eq!(rows[0].profile_fetched_at, Some(5));
        assert_eq!(rows[0].verified_at, Some(5));
    }

    #[test]
    fn exactly_one_active() {
        let db = Db::open_in_memory().unwrap();
        let a = add(&db, 1, "A", 1);
        let b = add(&db, 2, "B", 2);
        set_active(&db, a).unwrap();
        set_active(&db, b).unwrap();
        assert_eq!(active(&db).unwrap().unwrap().id, b);
        assert!(!get(&db, a).unwrap().is_active);
        assert_eq!(list(&db).unwrap().iter().filter(|r| r.is_active).count(), 1);
    }

    #[test]
    fn removing_the_active_account_activates_the_newest_remaining() {
        let db = Db::open_in_memory().unwrap();
        add(&db, 1, "A", 1);
        let b = add(&db, 2, "B", 2);
        let c = add(&db, 3, "C", 3);
        set_active(&db, c).unwrap();
        remove(&db, c).unwrap();
        assert_eq!(active(&db).unwrap().unwrap().id, b);
        assert_eq!(list(&db).unwrap().len(), 2);
    }

    #[test]
    fn removing_an_inactive_account_keeps_the_active_one() {
        let db = Db::open_in_memory().unwrap();
        let a = add(&db, 1, "A", 1);
        let b = add(&db, 2, "B", 2);
        set_active(&db, a).unwrap();
        remove(&db, b).unwrap();
        assert_eq!(active(&db).unwrap().unwrap().id, a);
    }

    #[test]
    fn removing_the_last_account_leaves_none_active() {
        let db = Db::open_in_memory().unwrap();
        let a = add(&db, 1, "A", 1);
        set_active(&db, a).unwrap();
        remove(&db, a).unwrap();
        assert!(active(&db).unwrap().is_none());
        assert!(list(&db).unwrap().is_empty());
    }

    #[test]
    fn verified_at_is_kept_when_later_checks_are_unknown() {
        let db = Db::open_in_memory().unwrap();
        let id = add(&db, 1, "A", 1);
        record_entitlement(&db, id, EntitlementState::Verified, 10).unwrap();
        record_entitlement(&db, id, EntitlementState::Unknown, 20).unwrap();
        let row = get(&db, id).unwrap();
        assert_eq!(row.entitlement, EntitlementState::Unknown);
        assert_eq!(row.entitlement_checked_at, Some(20));
        assert_eq!(row.verified_at, Some(10));
    }

    #[test]
    fn save_profile_clears_needs_login() {
        let db = Db::open_in_memory().unwrap();
        let id = add(&db, 1, "A", 1);
        set_needs_login(&db, id, true).unwrap();
        assert!(get(&db, id).unwrap().needs_login);
        save_profile(&db, &profile(1, "A"), EntitlementState::Unknown, 2).unwrap();
        assert!(!get(&db, id).unwrap().needs_login);
    }

    #[test]
    fn id_is_stored_without_hyphens() {
        let db = Db::open_in_memory().unwrap();
        let id = add(&db, 0xabcdef, "A", 1);
        let raw: String = db
            .with_conn(|c| Ok(c.query_row("SELECT id FROM accounts", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(raw, id.simple().to_string());
        assert_eq!(raw.len(), 32);
    }

    #[test]
    fn unknown_id_is_account_not_found() {
        let db = Db::open_in_memory().unwrap();
        let id = Uuid::from_u128(9);
        for err in [
            get(&db, id).unwrap_err(),
            set_active(&db, id).unwrap_err(),
            remove(&db, id).unwrap_err(),
            set_needs_login(&db, id, true).unwrap_err(),
            record_entitlement(&db, id, EntitlementState::Verified, 1).unwrap_err(),
        ] {
            assert_eq!(err.code(), "auth.account_not_found");
        }
    }
}
