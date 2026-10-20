//! instance_contents の読み書き。

use rusqlite::{Row as SqlRow, params};

use super::{ContentItem, ContentKind, InstalledAs, Provider, Side};
use crate::db::Db;
use crate::error::CoreError;

/// DB の 1 行。画面向けの `ContentItem` に、走査と照合で使う列を足したもの。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub item: ContentItem,
    pub mtime_ms: i64,
    pub sha512: String,
    pub identified_at: Option<i64>,
    pub local_icon_key: Option<String>,
}

const COLUMNS: &str = "kind, rel_path, enabled, sha1, sha512, size, mtime_ms, provider,
    project_id, version_id, pinned, installed_as, name, version_number, description,
    icon_url, local_icon_key, client_side, server_side, categories, dependencies, identified_at";

/// JSON の文字列配列。壊れていれば空配列として読む。
fn parse_list(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

fn read_row(r: &SqlRow<'_>) -> rusqlite::Result<Row> {
    let rel_path: String = r.get(1)?;
    let file_name = rel_path.rsplit('/').next().unwrap_or(&rel_path).to_string();
    let local_icon_key: Option<String> = r.get(16)?;
    let identified_at: Option<i64> = r.get(21)?;
    let side = |idx| -> rusqlite::Result<Option<Side>> {
        Ok(r.get::<_, Option<String>>(idx)?
            .as_deref()
            .and_then(Side::parse))
    };
    Ok(Row {
        item: ContentItem {
            kind: ContentKind::parse(&r.get::<_, String>(0)?).unwrap_or(ContentKind::Mod),
            rel_path,
            file_name,
            enabled: r.get::<_, i64>(2)? != 0,
            sha1: r.get(3)?,
            size: r.get::<_, i64>(5)?.max(0) as u64,
            provider: Provider::parse(&r.get::<_, String>(7)?),
            project_id: r.get(8)?,
            version_id: r.get(9)?,
            pinned: r.get::<_, i64>(10)? != 0,
            installed_as: InstalledAs::parse(&r.get::<_, String>(11)?),
            name: r.get(12)?,
            version_number: r.get(13)?,
            description: r.get(14)?,
            icon_url: r.get(15)?,
            has_local_icon: local_icon_key.is_some(),
            client_side: side(17)?,
            server_side: side(18)?,
            categories: parse_list(&r.get::<_, String>(19)?),
            dependencies: parse_list(&r.get::<_, String>(20)?),
            identified: identified_at.is_some(),
        },
        mtime_ms: r.get(6)?,
        sha512: r.get(4)?,
        identified_at,
        local_icon_key,
    })
}

pub(crate) fn list(
    db: &Db,
    instance_id: &str,
    kind: ContentKind,
) -> Result<Vec<ContentItem>, CoreError> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM instance_contents
             WHERE instance_id = ?1 AND kind = ?2 ORDER BY rel_path"
        ))?;
        let rows = stmt.query_map(params![instance_id, kind.as_str()], read_row)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?.item);
        }
        Ok(items)
    })
}

pub(crate) fn upsert(db: &Db, instance_id: &str, row: &Row) -> Result<(), CoreError> {
    let item = &row.item;
    let categories = serde_json::to_string(&item.categories).unwrap_or_else(|_| "[]".into());
    let dependencies = serde_json::to_string(&item.dependencies).unwrap_or_else(|_| "[]".into());
    db.with_conn(|conn| {
        conn.execute(
            &format!(
                "INSERT OR REPLACE INTO instance_contents (instance_id, {COLUMNS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                         ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)"
            ),
            params![
                instance_id,
                item.kind.as_str(),
                item.rel_path,
                item.enabled,
                item.sha1,
                row.sha512,
                item.size as i64,
                row.mtime_ms,
                item.provider.as_str(),
                item.project_id,
                item.version_id,
                item.pinned,
                item.installed_as.as_str(),
                item.name,
                item.version_number,
                item.description,
                item.icon_url,
                row.local_icon_key,
                item.client_side.map(Side::as_str),
                item.server_side.map(Side::as_str),
                categories,
                dependencies,
                row.identified_at,
            ],
        )?;
        Ok(())
    })
}

pub(crate) fn delete(db: &Db, instance_id: &str, rel_path: &str) -> Result<(), CoreError> {
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM instance_contents WHERE instance_id = ?1 AND rel_path = ?2",
            params![instance_id, rel_path],
        )?;
        Ok(())
    })
}

/// 有効、無効の切り替えなどでファイル名が変わったときに、行のキーと `enabled` を合わせる。
pub(crate) fn rename(
    db: &Db,
    instance_id: &str,
    old_rel: &str,
    new_rel: &str,
    enabled: bool,
) -> Result<(), CoreError> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE instance_contents SET rel_path = ?3, enabled = ?4
             WHERE instance_id = ?1 AND rel_path = ?2",
            params![instance_id, old_rel, new_rel, enabled],
        )?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db_with_instance() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO instances (id, name, dir_name, mc_version, state, created_at)
                 VALUES ('i1', 'n', 'd', '1.21', 'ready', 0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        db
    }

    fn row(rel_path: &str) -> Row {
        Row {
            item: ContentItem {
                kind: ContentKind::Mod,
                rel_path: rel_path.to_string(),
                file_name: rel_path.rsplit('/').next().unwrap().to_string(),
                enabled: true,
                size: 10,
                sha1: "a".repeat(40),
                provider: Provider::Modrinth,
                project_id: Some("AANobbMI".into()),
                version_id: Some("v1".into()),
                pinned: true,
                installed_as: InstalledAs::Dependency,
                name: Some("Sodium".into()),
                version_number: Some("0.5.0".into()),
                description: Some("d".into()),
                icon_url: Some("https://example.com/i.png".into()),
                has_local_icon: true,
                client_side: Some(Side::Required),
                server_side: Some(Side::Unsupported),
                categories: vec!["optimization".into()],
                dependencies: vec!["P7dR8mSH".into()],
                identified: true,
            },
            mtime_ms: 123,
            sha512: "b".repeat(128),
            identified_at: Some(456),
            local_icon_key: Some("k".into()),
        }
    }

    #[test]
    fn upserted_row_is_listed() {
        let db = db_with_instance();
        let r = row("mods/sodium.jar");
        upsert(&db, "i1", &r).unwrap();
        assert_eq!(list(&db, "i1", ContentKind::Mod).unwrap(), vec![r.item]);
        assert!(list(&db, "i1", ContentKind::Shader).unwrap().is_empty());
    }

    #[test]
    fn list_is_sorted_by_rel_path() {
        let db = db_with_instance();
        upsert(&db, "i1", &row("mods/b.jar")).unwrap();
        upsert(&db, "i1", &row("mods/a.jar")).unwrap();
        let paths: Vec<_> = list(&db, "i1", ContentKind::Mod)
            .unwrap()
            .into_iter()
            .map(|i| i.rel_path)
            .collect();
        assert_eq!(paths, ["mods/a.jar", "mods/b.jar"]);
    }

    #[test]
    fn second_upsert_overwrites() {
        let db = db_with_instance();
        let mut r = row("mods/sodium.jar");
        upsert(&db, "i1", &r).unwrap();
        r.item.size = 99;
        r.item.pinned = false;
        upsert(&db, "i1", &r).unwrap();
        let items = list(&db, "i1", ContentKind::Mod).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].size, 99);
        assert!(!items[0].pinned);
    }

    #[test]
    fn delete_removes_the_row() {
        let db = db_with_instance();
        upsert(&db, "i1", &row("mods/sodium.jar")).unwrap();
        delete(&db, "i1", "mods/sodium.jar").unwrap();
        assert!(list(&db, "i1", ContentKind::Mod).unwrap().is_empty());
    }

    #[test]
    fn rename_changes_path_and_enabled() {
        let db = db_with_instance();
        upsert(&db, "i1", &row("mods/sodium.jar")).unwrap();
        rename(
            &db,
            "i1",
            "mods/sodium.jar",
            "mods/sodium.jar.disabled",
            false,
        )
        .unwrap();
        let items = list(&db, "i1", ContentKind::Mod).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].rel_path, "mods/sodium.jar.disabled");
        assert!(!items[0].enabled);
    }

    #[test]
    fn deleting_the_instance_cascades() {
        let db = db_with_instance();
        upsert(&db, "i1", &row("mods/sodium.jar")).unwrap();
        db.with_conn(|conn| {
            conn.execute("DELETE FROM instances WHERE id = 'i1'", [])?;
            let n: i64 =
                conn.query_row("SELECT count(*) FROM instance_contents", [], |r| r.get(0))?;
            assert_eq!(n, 0);
            Ok(())
        })
        .unwrap();
    }
}
