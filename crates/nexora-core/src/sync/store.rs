//! 共通設定の保存層。テーブルは `0006_sync.sql`。

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::registry::Category;
use crate::db::Db;
use crate::error::CoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncProfile {
    pub id: String,
    pub name: String,
    pub revision: i64,
    pub categories: Vec<Category>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    pub instance_id: String,
    pub profile_id: String,
    pub excluded: Vec<String>,
    pub overrides: BTreeMap<String, String>,
}

/// 共通設定に合わせるかどうかをユーザーに尋ねる、項目ごとの提案。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub field_id: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SnapshotState {
    Applied,
    Skipped,
    Failed,
}

impl SnapshotState {
    fn as_str(self) -> &'static str {
        match self {
            SnapshotState::Applied => "applied",
            SnapshotState::Skipped => "skipped",
            SnapshotState::Failed => "failed",
        }
    }

    fn parse(text: &str) -> Result<Self, CoreError> {
        match text {
            "applied" => Ok(SnapshotState::Applied),
            "skipped" => Ok(SnapshotState::Skipped),
            "failed" => Ok(SnapshotState::Failed),
            other => Err(corrupt(format!("unknown snapshot state {other}"))),
        }
    }
}

/// インスタンスに最後に適用した内容の記録。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub instance_id: String,
    pub applied_revision: i64,
    pub baseline: BTreeMap<String, String>,
    pub proposals: Vec<Proposal>,
    pub state: SnapshotState,
    /// 最後に記録した時刻（unix ミリ秒）。時刻を持たない古い記録は `None`。
    pub updated_at: Option<i64>,
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn corrupt(message: String) -> CoreError {
    CoreError::Unexpected { message }
}

fn to_json<T: Serialize + ?Sized>(value: &T) -> Result<String, CoreError> {
    serde_json::to_string(value).map_err(|e| corrupt(format!("failed to encode json: {e}")))
}

fn from_json<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T, CoreError> {
    serde_json::from_str(text).map_err(|e| corrupt(format!("stored json is invalid: {e}")))
}

pub fn create_profile(
    db: &Db,
    name: &str,
    categories: &[Category],
) -> Result<SyncProfile, CoreError> {
    let id = uuid::Uuid::new_v4().to_string();
    let json = to_json(categories)?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO sync_profiles (id, name, revision, categories, created_at)
             VALUES (?1, ?2, 0, ?3, ?4)",
            params![id, name, json, now_millis()],
        )?;
        Ok(())
    })?;
    Ok(SyncProfile {
        id,
        name: name.to_string(),
        revision: 0,
        categories: categories.to_vec(),
    })
}

pub fn list_profiles(db: &Db) -> Result<Vec<SyncProfile>, CoreError> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, revision, categories FROM sync_profiles ORDER BY created_at, id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, name, revision, categories)| {
                Ok(SyncProfile {
                    id,
                    name,
                    revision,
                    categories: from_json(&categories)?,
                })
            })
            .collect()
    })
}

pub fn rename_profile(db: &Db, profile_id: &str, name: &str) -> Result<(), CoreError> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE sync_profiles SET name = ?2 WHERE id = ?1",
            params![profile_id, name],
        )?;
        Ok(())
    })
}

pub fn set_categories(db: &Db, profile_id: &str, categories: &[Category]) -> Result<(), CoreError> {
    let json = to_json(categories)?;
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE sync_profiles SET categories = ?2 WHERE id = ?1",
            params![profile_id, json],
        )?;
        Ok(())
    })
}

/// 共通設定を消す。値と参加の行は外部キーで一緒に消える。
pub fn delete_profile(db: &Db, profile_id: &str) -> Result<(), CoreError> {
    db.with_conn(|conn| {
        conn.execute("DELETE FROM sync_profiles WHERE id = ?1", [profile_id])?;
        Ok(())
    })
}

/// 値を書き込み、revision を 1 上げる。
pub fn set_values(db: &Db, profile_id: &str, values: &[(String, String)]) -> Result<(), CoreError> {
    db.with_conn(|conn| {
        let tx = conn.transaction()?;
        let now = now_millis();
        for (field_id, value) in values {
            tx.execute(
                "INSERT INTO sync_values (profile_id, field_id, value, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(profile_id, field_id)
                 DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![profile_id, field_id, value, now],
            )?;
        }
        tx.execute(
            "UPDATE sync_profiles SET revision = revision + 1 WHERE id = ?1",
            [profile_id],
        )?;
        tx.commit()?;
        Ok(())
    })
}

pub fn values(db: &Db, profile_id: &str) -> Result<BTreeMap<String, String>, CoreError> {
    db.with_conn(|conn| {
        let mut stmt =
            conn.prepare("SELECT field_id, value FROM sync_values WHERE profile_id = ?1")?;
        let rows = stmt
            .query_map([profile_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<BTreeMap<String, String>, _>>()?;
        Ok(rows)
    })
}

/// インスタンスを共通設定に参加させる。同じ共通設定への再参加は何も変えない。
/// 別の共通設定にいれば移り、除外と上書きを空に戻し、前の共通設定の記録（snapshot）を消す。
pub fn join(db: &Db, instance_id: &str, profile_id: &str) -> Result<(), CoreError> {
    db.with_conn(|conn| {
        let tx = conn.transaction()?;
        let current: Option<String> = tx
            .query_row(
                "SELECT profile_id FROM sync_profile_instances WHERE instance_id = ?1",
                [instance_id],
                |row| row.get(0),
            )
            .optional()?;
        if current.as_deref() == Some(profile_id) {
            return Ok(());
        }
        tx.execute(
            "INSERT INTO sync_profile_instances (instance_id, profile_id) VALUES (?1, ?2)
             ON CONFLICT(instance_id) DO UPDATE SET
                 profile_id = excluded.profile_id, excluded_fields = '[]', overrides = '{}'",
            params![instance_id, profile_id],
        )?;
        tx.execute(
            "DELETE FROM sync_snapshots WHERE instance_id = ?1",
            [instance_id],
        )?;
        tx.commit()?;
        Ok(())
    })
}

pub fn leave(db: &Db, instance_id: &str) -> Result<(), CoreError> {
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM sync_profile_instances WHERE instance_id = ?1",
            [instance_id],
        )?;
        Ok(())
    })
}

pub fn membership(db: &Db, instance_id: &str) -> Result<Option<Membership>, CoreError> {
    db.with_conn(|conn| {
        let row = conn
            .query_row(
                "SELECT profile_id, excluded_fields, overrides
                 FROM sync_profile_instances WHERE instance_id = ?1",
                [instance_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(profile_id, excluded, overrides)| {
            Ok(Membership {
                instance_id: instance_id.to_string(),
                profile_id,
                excluded: from_json(&excluded)?,
                overrides: from_json(&overrides)?,
            })
        })
        .transpose()
    })
}

pub fn members(db: &Db, profile_id: &str) -> Result<Vec<String>, CoreError> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT instance_id FROM sync_profile_instances
             WHERE profile_id = ?1 ORDER BY instance_id",
        )?;
        let ids = stmt
            .query_map([profile_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(ids)
    })
}

/// 参加中のインスタンスで、共通設定から外す項目を置き換える。
pub fn set_excluded(db: &Db, instance_id: &str, excluded: &[String]) -> Result<(), CoreError> {
    let json = to_json(excluded)?;
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE sync_profile_instances SET excluded_fields = ?2 WHERE instance_id = ?1",
            params![instance_id, json],
        )?;
        Ok(())
    })
}

/// 参加中のインスタンスで、項目 1 つの上書き値を決める。`None` なら上書きを消す。
pub fn set_override(
    db: &Db,
    instance_id: &str,
    field_id: &str,
    value: Option<&str>,
) -> Result<(), CoreError> {
    let Some(mut current) = membership(db, instance_id)? else {
        return Ok(());
    };
    match value {
        Some(value) => {
            current
                .overrides
                .insert(field_id.to_string(), value.to_string());
        }
        None => {
            current.overrides.remove(field_id);
        }
    }
    let json = to_json(&current.overrides)?;
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE sync_profile_instances SET overrides = ?2 WHERE instance_id = ?1",
            params![instance_id, json],
        )?;
        Ok(())
    })
}

/// 適用の結果を記録する。提案は空に戻る。
pub fn save_snapshot(
    db: &Db,
    instance_id: &str,
    applied_revision: i64,
    baseline: &BTreeMap<String, String>,
    state: SnapshotState,
) -> Result<(), CoreError> {
    let baseline = to_json(baseline)?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO sync_snapshots
                 (instance_id, applied_revision, baseline, proposals, state, updated_at)
             VALUES (?1, ?2, ?3, '[]', ?4, ?5)
             ON CONFLICT(instance_id) DO UPDATE SET
                 applied_revision = excluded.applied_revision,
                 baseline = excluded.baseline,
                 proposals = '[]',
                 state = excluded.state,
                 updated_at = excluded.updated_at",
            params![
                instance_id,
                applied_revision,
                baseline,
                state.as_str(),
                now_millis()
            ],
        )?;
        Ok(())
    })
}

pub fn snapshot(db: &Db, instance_id: &str) -> Result<Option<Snapshot>, CoreError> {
    db.with_conn(|conn| {
        let row = conn
            .query_row(
                "SELECT applied_revision, baseline, proposals, state, updated_at
                 FROM sync_snapshots WHERE instance_id = ?1",
                [instance_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                    ))
                },
            )
            .optional()?;
        row.map(
            |(applied_revision, baseline, proposals, state, updated_at)| {
                Ok(Snapshot {
                    instance_id: instance_id.to_string(),
                    applied_revision,
                    baseline: from_json(&baseline)?,
                    proposals: from_json(&proposals)?,
                    state: SnapshotState::parse(&state)?,
                    updated_at,
                })
            },
        )
        .transpose()
    })
}

/// 記録済みの提案を置き換える。記録（snapshot）が無ければ何もしない。
pub fn set_proposals(db: &Db, instance_id: &str, proposals: &[Proposal]) -> Result<(), CoreError> {
    let json = to_json(proposals)?;
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE sync_snapshots SET proposals = ?2 WHERE instance_id = ?1",
            params![instance_id, json],
        )?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db_with_instances(ids: &[&str]) -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|conn| {
            for id in ids {
                conn.execute(
                    "INSERT INTO instances (id, name, dir_name, mc_version, state, created_at)
                     VALUES (?1, ?1, ?1, '1.21', 'ready', 0)",
                    [id],
                )?;
            }
            Ok(())
        })
        .unwrap();
        db
    }

    fn count(db: &Db, table: &str) -> i64 {
        db.with_conn(|conn| {
            Ok(conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?)
        })
        .unwrap()
    }

    #[test]
    fn create_and_list_profiles() {
        let db = db_with_instances(&[]);
        let a = create_profile(&db, "A", &[Category::Keys, Category::Sound]).unwrap();
        let b = create_profile(&db, "B", &[]).unwrap();
        assert_eq!(a.revision, 0);
        let listed = list_profiles(&db).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.contains(&a) && listed.contains(&b));
        rename_profile(&db, &a.id, "A2").unwrap();
        set_categories(&db, &a.id, &[Category::Chat]).unwrap();
        let got = list_profiles(&db)
            .unwrap()
            .into_iter()
            .find(|p| p.id == a.id)
            .unwrap();
        assert_eq!(got.name, "A2");
        assert_eq!(got.categories, vec![Category::Chat]);
    }

    #[test]
    fn set_values_bumps_revision() {
        let db = db_with_instances(&[]);
        let p = create_profile(&db, "A", &[Category::Keys]).unwrap();
        set_values(&db, &p.id, &[("fov".into(), "70".into())]).unwrap();
        set_values(
            &db,
            &p.id,
            &[("fov".into(), "90".into()), ("gamma".into(), "1".into())],
        )
        .unwrap();
        let got = &list_profiles(&db).unwrap()[0];
        assert_eq!(got.revision, 2);
        let v = values(&db, &p.id).unwrap();
        assert_eq!(v.get("fov").map(String::as_str), Some("90"));
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn join_is_exclusive_per_instance() {
        let db = db_with_instances(&["i1"]);
        let a = create_profile(&db, "A", &[]).unwrap();
        let b = create_profile(&db, "B", &[]).unwrap();
        join(&db, "i1", &a.id).unwrap();
        set_excluded(&db, "i1", &["fov".to_string()]).unwrap();
        set_override(&db, "i1", "gamma", Some("2")).unwrap();
        let m = membership(&db, "i1").unwrap().unwrap();
        assert_eq!(m.excluded, vec!["fov".to_string()]);
        assert_eq!(m.overrides.get("gamma").map(String::as_str), Some("2"));
        set_override(&db, "i1", "gamma", None).unwrap();
        assert!(membership(&db, "i1").unwrap().unwrap().overrides.is_empty());

        join(&db, "i1", &b.id).unwrap();
        assert_eq!(count(&db, "sync_profile_instances"), 1);
        assert_eq!(membership(&db, "i1").unwrap().unwrap().profile_id, b.id);
        assert!(members(&db, &a.id).unwrap().is_empty());
        assert_eq!(members(&db, &b.id).unwrap(), vec!["i1".to_string()]);

        leave(&db, "i1").unwrap();
        assert!(membership(&db, "i1").unwrap().is_none());
    }

    #[test]
    fn snapshot_round_trip() {
        let db = db_with_instances(&["i1"]);
        assert!(snapshot(&db, "i1").unwrap().is_none());
        let baseline = BTreeMap::from([("fov".to_string(), "70".to_string())]);
        save_snapshot(&db, "i1", 3, &baseline, SnapshotState::Applied).unwrap();
        let proposals = vec![Proposal {
            field_id: "gamma".into(),
            value: "2".into(),
        }];
        set_proposals(&db, "i1", &proposals).unwrap();
        let s = snapshot(&db, "i1").unwrap().unwrap();
        assert_eq!(s.applied_revision, 3);
        assert_eq!(s.baseline, baseline);
        assert_eq!(s.proposals, proposals);
        assert_eq!(s.state, SnapshotState::Applied);
        save_snapshot(&db, "i1", 4, &baseline, SnapshotState::Failed).unwrap();
        let s = snapshot(&db, "i1").unwrap().unwrap();
        assert_eq!(s.state, SnapshotState::Failed);
        assert!(s.proposals.is_empty());
    }

    #[test]
    fn snapshot_records_the_save_time() {
        let db = db_with_instances(&["i1"]);
        let before = now_millis();
        save_snapshot(&db, "i1", 1, &BTreeMap::new(), SnapshotState::Applied).unwrap();
        let at = snapshot(&db, "i1").unwrap().unwrap().updated_at.unwrap();
        assert!(at >= before && at <= now_millis());
        // 時刻を持たない古い記録は None として読める
        db.with_conn(|conn| {
            conn.execute("UPDATE sync_snapshots SET updated_at = NULL", [])?;
            Ok(())
        })
        .unwrap();
        assert_eq!(snapshot(&db, "i1").unwrap().unwrap().updated_at, None);
    }

    #[test]
    fn deleting_an_instance_removes_membership_and_snapshot() {
        let db = db_with_instances(&["i1", "i2"]);
        let p = create_profile(&db, "A", &[]).unwrap();
        for id in ["i1", "i2"] {
            join(&db, id, &p.id).unwrap();
            save_snapshot(&db, id, 1, &BTreeMap::new(), SnapshotState::Skipped).unwrap();
        }
        db.with_conn(|conn| {
            conn.execute("DELETE FROM instances WHERE id = 'i1'", [])?;
            Ok(())
        })
        .unwrap();
        assert!(membership(&db, "i1").unwrap().is_none());
        assert!(snapshot(&db, "i1").unwrap().is_none());
        assert!(membership(&db, "i2").unwrap().is_some());
        assert!(snapshot(&db, "i2").unwrap().is_some());
    }

    #[test]
    fn deleting_a_profile_removes_its_memberships_and_values() {
        let db = db_with_instances(&["i1"]);
        let p = create_profile(&db, "A", &[]).unwrap();
        let other = create_profile(&db, "B", &[]).unwrap();
        set_values(&db, &p.id, &[("fov".into(), "70".into())]).unwrap();
        set_values(&db, &other.id, &[("fov".into(), "80".into())]).unwrap();
        join(&db, "i1", &p.id).unwrap();
        delete_profile(&db, &p.id).unwrap();
        assert!(membership(&db, "i1").unwrap().is_none());
        assert!(values(&db, &p.id).unwrap().is_empty());
        assert_eq!(count(&db, "sync_values"), 1);
        assert_eq!(list_profiles(&db).unwrap().len(), 1);
    }

    #[test]
    fn rejoining_the_same_profile_keeps_state() {
        let db = db_with_instances(&["i1"]);
        let p = create_profile(&db, "A", &[]).unwrap();
        join(&db, "i1", &p.id).unwrap();
        set_excluded(&db, "i1", &["fov".to_string()]).unwrap();
        set_override(&db, "i1", "gamma", Some("2")).unwrap();
        save_snapshot(&db, "i1", 1, &BTreeMap::new(), SnapshotState::Applied).unwrap();
        join(&db, "i1", &p.id).unwrap();
        let m = membership(&db, "i1").unwrap().unwrap();
        assert_eq!(m.excluded, vec!["fov".to_string()]);
        assert_eq!(m.overrides.get("gamma").map(String::as_str), Some("2"));
        assert!(snapshot(&db, "i1").unwrap().is_some());
    }

    #[test]
    fn moving_to_another_profile_resets_state_and_drops_snapshot() {
        let db = db_with_instances(&["i1"]);
        let a = create_profile(&db, "A", &[]).unwrap();
        let b = create_profile(&db, "B", &[]).unwrap();
        join(&db, "i1", &a.id).unwrap();
        set_excluded(&db, "i1", &["fov".to_string()]).unwrap();
        set_override(&db, "i1", "gamma", Some("2")).unwrap();
        save_snapshot(&db, "i1", 1, &BTreeMap::new(), SnapshotState::Applied).unwrap();
        join(&db, "i1", &b.id).unwrap();
        let m = membership(&db, "i1").unwrap().unwrap();
        assert_eq!(m.profile_id, b.id);
        assert!(m.excluded.is_empty() && m.overrides.is_empty());
        assert!(snapshot(&db, "i1").unwrap().is_none());
    }
}
