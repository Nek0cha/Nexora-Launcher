//! 保存したスキンのライブラリ（`skin_library`）と、現在のスキンを一度だけ取り込む記録（`skin_import_log`）。

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::pixel_hash;
use crate::auth::minecraft::{SkinVariant, validate_skin_png};
use crate::db::Db;
use crate::error::CoreError;

/// 画面に渡す保存済みスキン（画像そのものは `png` で別に取る）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SavedSkin {
    pub id: String,
    pub name: String,
    pub variant: SkinVariant,
    pub sha256: String,
    pub added_at_ms: i64,
}

/// 編集の内容。指定した項目だけを変える。
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SavedSkinPatch {
    pub name: Option<String>,
    pub variant: Option<SkinVariant>,
    pub png: Option<Vec<u8>>,
}

fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidSkin {
        reason: reason.into(),
    }
}

/// 前後の空白を除く。空なら `InvalidSkin { reason: "name" }`。
fn clean_name(name: &str) -> Result<String, CoreError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid("name"));
    }
    Ok(name.to_string())
}

fn parse_variant(s: &str) -> SkinVariant {
    if s == "slim" {
        SkinVariant::Slim
    } else {
        SkinVariant::Classic
    }
}

/// 64×64 の RGBA に展開して、スリム（細い腕）かどうかを見分ける。
/// 腕の右側の列 (54..=55, 20..=31) がすべて透明ならスリム。64×32 は常にクラシック。
/// 展開できない（壊れた）PNG は None。
pub fn detect_variant(png: &[u8]) -> Option<SkinVariant> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = (reader.info().width as usize, reader.info().height as usize);
    if width != 64 || !(height == 64 || height == 32) {
        return None;
    }
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buf).ok()?;
    if height == 32 {
        return Some(SkinVariant::Classic);
    }
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    let src = buf.get(..frame.buffer_size())?;
    // alpha を持たない形式は不透明として扱う
    let alpha = |x: usize, y: usize| -> Option<u8> {
        let px = src.get((y * width + x) * channels..)?;
        Some(match channels {
            2 => *px.get(1)?,
            4 => *px.get(3)?,
            _ => 255,
        })
    };
    for y in 20..=31 {
        for x in 54..=55 {
            if alpha(x, y)? != 0 {
                return Some(SkinVariant::Classic);
            }
        }
    }
    Some(SkinVariant::Slim)
}

const COLUMNS: &str = "id, name, variant, sha256, added_at";

fn row_to_skin(row: &Row<'_>) -> rusqlite::Result<SavedSkin> {
    let variant: String = row.get(2)?;
    Ok(SavedSkin {
        id: row.get(0)?,
        name: row.get(1)?,
        variant: parse_variant(&variant),
        sha256: row.get(3)?,
        added_at_ms: row.get(4)?,
    })
}

fn find_by_sha(conn: &rusqlite::Connection, sha: &str) -> Result<Option<SavedSkin>, CoreError> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT {COLUMNS} FROM skin_library WHERE sha256 = ?1 ORDER BY added_at LIMIT 1"
            ),
            [sha],
            row_to_skin,
        )
        .optional()?)
}

fn insert(
    conn: &rusqlite::Connection,
    name: &str,
    variant: SkinVariant,
    sha: &str,
    png: &[u8],
    now_ms: i64,
) -> Result<SavedSkin, CoreError> {
    let id = Uuid::new_v4().simple().to_string();
    conn.execute(
        "INSERT INTO skin_library (id, name, variant, sha256, png, added_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, name, variant.as_str(), sha, png, now_ms],
    )?;
    Ok(SavedSkin {
        id,
        name: name.to_string(),
        variant,
        sha256: sha.to_string(),
        added_at_ms: now_ms,
    })
}

/// ライブラリに足す。同じ画素の内容がすでにあれば、新しく作らず既存を返す。
/// `variant` が None なら画像から判定し、判定できなければクラシック。
pub fn add(
    db: &Db,
    name: &str,
    variant: Option<SkinVariant>,
    png: &[u8],
    now_ms: i64,
) -> Result<SavedSkin, CoreError> {
    let name = clean_name(name)?;
    validate_skin_png(png)?;
    let variant = variant.unwrap_or_else(|| detect_variant(png).unwrap_or(SkinVariant::Classic));
    let sha = pixel_hash(png);
    db.with_conn(|conn| {
        if let Some(existing) = find_by_sha(conn, &sha)? {
            return Ok(existing);
        }
        insert(conn, &name, variant, &sha, png, now_ms)
    })
}

/// 追加が新しい順。
pub fn list(db: &Db) -> Result<Vec<SavedSkin>, CoreError> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM skin_library ORDER BY added_at DESC, id"
        ))?;
        let rows = stmt.query_map([], row_to_skin)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
}

/// 画像と型を返す。
pub fn png(db: &Db, id: &str) -> Result<(Vec<u8>, SkinVariant), CoreError> {
    db.with_conn(|conn| {
        let row: Option<(Vec<u8>, String)> = conn
            .query_row(
                "SELECT png, variant FROM skin_library WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (png, variant) = row.ok_or_else(|| invalid("not_found"))?;
        Ok((png, parse_variant(&variant)))
    })
}

/// 名前、型、画像のうち指定したものを変える。画像を変えると `sha256` も変わり、`id` は変わらない。
pub fn update(db: &Db, id: &str, patch: SavedSkinPatch) -> Result<SavedSkin, CoreError> {
    let name = patch.name.as_deref().map(clean_name).transpose()?;
    let new_png = match &patch.png {
        Some(bytes) => {
            validate_skin_png(bytes)?;
            Some((bytes.as_slice(), pixel_hash(bytes)))
        }
        None => None,
    };
    db.with_conn(|conn| {
        let tx = conn.transaction()?;
        let exists = tx
            .query_row("SELECT 1 FROM skin_library WHERE id = ?1", [id], |_| Ok(()))
            .optional()?
            .is_some();
        if !exists {
            return Err(invalid("not_found"));
        }
        if let Some(name) = &name {
            tx.execute(
                "UPDATE skin_library SET name = ?2 WHERE id = ?1",
                params![id, name],
            )?;
        }
        if let Some(variant) = patch.variant {
            tx.execute(
                "UPDATE skin_library SET variant = ?2 WHERE id = ?1",
                params![id, variant.as_str()],
            )?;
        }
        if let Some((bytes, sha)) = &new_png {
            tx.execute(
                "UPDATE skin_library SET png = ?2, sha256 = ?3 WHERE id = ?1",
                params![id, bytes, sha],
            )?;
        }
        let skin = tx.query_row(
            &format!("SELECT {COLUMNS} FROM skin_library WHERE id = ?1"),
            [id],
            row_to_skin,
        )?;
        tx.commit()?;
        Ok(skin)
    })
}

/// 削除する。取り込みの記録は残すので、現在のスキンが同じ間は戻ってこない。
pub fn delete(db: &Db, id: &str) -> Result<(), CoreError> {
    db.with_conn(|conn| {
        conn.execute("DELETE FROM skin_library WHERE id = ?1", [id])?;
        Ok(())
    })
}

/// アカウントの現在のスキンを、(アカウント, 画素の内容) ごとに一度だけ取り込む。
/// 取り込んだら `Some`。記録済み、またはすでに同じ画像がライブラリにあれば `None`
/// （後者も記録は残す）。削除したスキンは、記録があるので二度と戻らない。
pub fn import_current(
    db: &Db,
    account_id: Uuid,
    name: &str,
    variant: SkinVariant,
    png: &[u8],
    now_ms: i64,
) -> Result<Option<SavedSkin>, CoreError> {
    let name = clean_name(name)?;
    validate_skin_png(png)?;
    let sha = pixel_hash(png);
    let account = account_id.simple().to_string();
    db.with_conn(|conn| {
        let tx = conn.transaction()?;
        let logged = tx.execute(
            "INSERT OR IGNORE INTO skin_import_log (account_id, sha256) VALUES (?1, ?2)",
            params![account, sha],
        )?;
        if logged == 0 || find_by_sha(&tx, &sha)?.is_some() {
            tx.commit()?;
            return Ok(None);
        }
        let skin = insert(&tx, &name, variant, &sha, png, now_ms)?;
        tx.commit()?;
        Ok(Some(skin))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(w: u32, h: u32, pixels: &[[u8; 4]]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        let data: Vec<u8> = pixels.iter().flatten().copied().collect();
        writer.write_image_data(&data).unwrap();
        writer.finish().unwrap();
        out
    }

    /// 全面が不透明で、`seed` で中身が変わる 64×64。
    fn skin(seed: u8) -> Vec<u8> {
        let px: Vec<[u8; 4]> = (0..64 * 64)
            .map(|i| [(i % 251) as u8, seed, (i % 13) as u8, 255])
            .collect();
        encode(64, 64, &px)
    }

    /// 腕の右側の列 (54..=55, 20..=31) を透明にした 64×64。`opaque_at` の画素だけ不透明に戻す。
    fn arm_skin(opaque_at: Option<(usize, usize)>) -> Vec<u8> {
        let mut px = vec![[10u8, 20, 30, 255]; 64 * 64];
        for y in 20..=31 {
            for x in 54..=55 {
                px[y * 64 + x] = [0, 0, 0, 0];
            }
        }
        if let Some((x, y)) = opaque_at {
            px[y * 64 + x] = [1, 2, 3, 255];
        }
        encode(64, 64, &px)
    }

    /// IHDR だけの PNG（検査は通るが、中身は壊れている）。
    fn ihdr_only(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]);
        v.extend_from_slice(&[0, 0, 0, 0]);
        v
    }

    fn is_invalid(r: Result<SavedSkin, CoreError>, why: &str) -> bool {
        matches!(r, Err(CoreError::InvalidSkin { reason }) if reason == why)
    }

    #[test]
    fn add_validates_and_rejects_bad_png() {
        let db = Db::open_in_memory().unwrap();
        // 壊れた中身でもパニックせず、判定は None、追加はクラシックになる
        let broken = ihdr_only(64, 64);
        validate_skin_png(&broken).unwrap();
        assert_eq!(detect_variant(&broken), None);
        assert_eq!(
            add(&db, "broken", None, &broken, 1).unwrap().variant,
            SkinVariant::Classic
        );
        assert!(add(&db, "x", None, b"<html>no</html>", 1).is_err());
        let mut huge = skin(1);
        huge.resize(64 * 1024 + 1, 0);
        assert!(add(&db, "x", None, &huge, 1).is_err());
        assert!(add(&db, "x", None, &ihdr_only(32, 32), 1).is_err());
        assert!(is_invalid(add(&db, "  ", None, &skin(1), 1), "name"));
    }

    #[test]
    fn detect_variant_reads_the_slim_arm_columns() {
        assert_eq!(detect_variant(&arm_skin(None)), Some(SkinVariant::Slim));
        assert_eq!(
            detect_variant(&arm_skin(Some((55, 31)))),
            Some(SkinVariant::Classic)
        );
        assert_eq!(
            detect_variant(&arm_skin(Some((54, 20)))),
            Some(SkinVariant::Classic)
        );
        // 範囲の外の不透明は関係ない
        assert_eq!(
            detect_variant(&arm_skin(Some((56, 20)))),
            Some(SkinVariant::Slim)
        );
        let old = encode(64, 32, &vec![[0, 0, 0, 0]; 64 * 32]);
        assert_eq!(detect_variant(&old), Some(SkinVariant::Classic));
    }

    #[test]
    fn add_uses_the_given_variant_over_detection_and_dedupes_by_sha256() {
        let db = Db::open_in_memory().unwrap();
        let slim = arm_skin(None);
        // 画像はスリムだが、指定したクラシックを使う
        let a = add(&db, " Alex ", Some(SkinVariant::Classic), &slim, 10).unwrap();
        assert_eq!(a.name, "Alex");
        assert_eq!(a.variant, SkinVariant::Classic);
        // 指定がなければ画像から判定する
        let auto = add(&db, "auto", None, &arm_skin(Some((54, 20))), 11).unwrap();
        assert_eq!(auto.variant, SkinVariant::Classic);
        let third = add(&db, "third", Some(SkinVariant::Slim), &skin(9), 12).unwrap();
        assert_eq!(third.variant, SkinVariant::Slim);
        // 同じ画素の内容は 1 件のまま
        let again = add(&db, "other", Some(SkinVariant::Slim), &slim, 99).unwrap();
        assert_eq!(again, a);
        let listed = list(&db).unwrap();
        // 新しい順
        assert_eq!(
            listed.iter().map(|s| s.added_at_ms).collect::<Vec<_>>(),
            vec![12, 11, 10]
        );
        let (bytes, variant) = png(&db, &a.id).unwrap();
        assert_eq!(bytes, slim);
        assert_eq!(variant, SkinVariant::Classic);
        assert!(png(&db, "nope").is_err());
        // 指定も保存もない場合の判定はスリム
        let db2 = Db::open_in_memory().unwrap();
        assert_eq!(
            add(&db2, "s", None, &slim, 1).unwrap().variant,
            SkinVariant::Slim
        );
    }

    #[test]
    fn update_trims_name_and_rejects_empty_name() {
        let db = Db::open_in_memory().unwrap();
        let s = add(&db, "a", None, &skin(1), 1).unwrap();
        let patch = |n: &str| SavedSkinPatch {
            name: Some(n.into()),
            ..Default::default()
        };
        assert_eq!(update(&db, &s.id, patch("  New  ")).unwrap().name, "New");
        assert!(is_invalid(update(&db, &s.id, patch("   ")), "name"));
        assert_eq!(list(&db).unwrap()[0].name, "New");
        assert!(update(&db, "nope", patch("x")).is_err());
    }

    #[test]
    fn update_can_replace_the_png_and_keeps_the_id() {
        let db = Db::open_in_memory().unwrap();
        let s = add(&db, "a", None, &skin(1), 1).unwrap();
        let new = skin(2);
        let u = update(
            &db,
            &s.id,
            SavedSkinPatch {
                png: Some(new.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(u.id, s.id);
        assert_ne!(u.sha256, s.sha256);
        assert_eq!(u.sha256, pixel_hash(&new));
        assert_eq!(png(&db, &s.id).unwrap().0, new);
        let bad = SavedSkinPatch {
            png: Some(b"junk".to_vec()),
            ..Default::default()
        };
        assert!(update(&db, &s.id, bad).is_err());
    }

    #[test]
    fn update_variant() {
        let db = Db::open_in_memory().unwrap();
        let s = add(&db, "a", None, &skin(1), 1).unwrap();
        let u = update(
            &db,
            &s.id,
            SavedSkinPatch {
                variant: Some(SkinVariant::Slim),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(u.variant, SkinVariant::Slim);
        assert_eq!(png(&db, &s.id).unwrap().1, SkinVariant::Slim);
        assert_eq!(u.name, "a");
    }

    #[test]
    fn delete_removes_the_row() {
        let db = Db::open_in_memory().unwrap();
        let s = add(&db, "a", None, &skin(1), 1).unwrap();
        delete(&db, &s.id).unwrap();
        assert!(list(&db).unwrap().is_empty());
        delete(&db, &s.id).unwrap();
    }

    #[test]
    fn import_current_adds_once_per_account_and_image() {
        let db = Db::open_in_memory().unwrap();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let img = skin(5);
        let first = import_current(&db, a, "Mine", SkinVariant::Slim, &img, 1)
            .unwrap()
            .unwrap();
        assert_eq!(first.variant, SkinVariant::Slim);
        assert!(
            import_current(&db, a, "Mine", SkinVariant::Slim, &img, 2)
                .unwrap()
                .is_none()
        );
        // 削除しても戻らない
        delete(&db, &first.id).unwrap();
        assert!(
            import_current(&db, a, "Mine", SkinVariant::Slim, &img, 3)
                .unwrap()
                .is_none()
        );
        assert!(list(&db).unwrap().is_empty());
        // 別のアカウントは取り込める
        assert!(
            import_current(&db, b, "Mine", SkinVariant::Slim, &img, 4)
                .unwrap()
                .is_some()
        );
        // すでに同じ画像があれば追加せず、記録だけ残す
        let c = Uuid::new_v4();
        let other = skin(6);
        add(&db, "manual", None, &other, 5).unwrap();
        assert!(
            import_current(&db, c, "Cur", SkinVariant::Classic, &other, 6)
                .unwrap()
                .is_none()
        );
        assert_eq!(list(&db).unwrap().len(), 2);
        let logged: i64 = db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM skin_import_log WHERE account_id = ?1",
                    [c.simple().to_string()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(logged, 1);
        // 名前が空なら記録も残さない
        assert!(import_current(&db, c, " ", SkinVariant::Classic, &skin(7), 7).is_err());
        assert!(
            import_current(&db, c, "ok", SkinVariant::Classic, &skin(7), 7)
                .unwrap()
                .is_some()
        );
    }
}
