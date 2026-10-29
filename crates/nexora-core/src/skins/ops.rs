//! ライブラリの操作のうち、ファイルや通信をまたぐもの（Tauri 命令から呼ぶ）。
//! DB は同期の呼び出しの中でだけ使い、`await` をまたいで握らない。

use std::io::Read;
use std::path::Path;

use uuid::Uuid;

use super::library::{self, SavedSkin};
use crate::auth::accounts;
use crate::auth::minecraft::SKIN_MAX_BYTES;
use crate::auth::service::AuthService;
use crate::db::Db;
use crate::error::CoreError;

fn io_error(path: &Path, source: std::io::Error) -> CoreError {
    CoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// 上限までしか読まない。`SKIN_MAX_BYTES` を超えて読めたら `too_large`。
fn read_bounded(reader: impl Read) -> Result<Vec<u8>, CoreError> {
    let mut bytes = Vec::new();
    reader
        .take(SKIN_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error(Path::new(""), e))?;
    if bytes.len() > SKIN_MAX_BYTES {
        return Err(CoreError::InvalidSkin {
            reason: "too_large".into(),
        });
    }
    Ok(bytes)
}

/// OS から渡されたパスの PNG を読み、(拡張子を除くファイル名, 中身) を返す。
/// ファイルを一度だけ開き、開いたハンドルの `metadata` で通常のファイルかと大きさを調べる。
/// 読むときも上限で打ち切るので、調べた後に大きくなったり差し替えられたりしても全部は読まない。
/// 画像そのものの検査（署名と寸法）は、ライブラリへ足すときの `validate_skin_png` が行う。
pub fn read_skin_file(path: &Path) -> Result<(String, Vec<u8>), CoreError> {
    // ディレクトリは開けない環境があるので、開く前にも通常のファイルかを見る（本当の判定は下）
    if !std::fs::metadata(path)
        .map_err(|e| io_error(path, e))?
        .is_file()
    {
        return Err(CoreError::InvalidSkin {
            reason: "not_file".into(),
        });
    }
    let file = std::fs::File::open(path).map_err(|e| io_error(path, e))?;
    let meta = file.metadata().map_err(|e| io_error(path, e))?;
    if !meta.is_file() {
        return Err(CoreError::InvalidSkin {
            reason: "not_file".into(),
        });
    }
    if meta.len() > SKIN_MAX_BYTES as u64 {
        return Err(CoreError::InvalidSkin {
            reason: "too_large".into(),
        });
    }
    let bytes = read_bounded(file).map_err(|e| match e {
        CoreError::Io { source, .. } => io_error(path, source),
        other => other,
    })?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok((name, bytes))
}

/// ファイルからライブラリに足す。型は画像から判定する。
pub fn add_from_path(db: &Db, path: &Path, now_ms: i64) -> Result<SavedSkin, CoreError> {
    let (name, bytes) = read_skin_file(path)?;
    library::add(db, &name, None, &bytes, now_ms)
}

/// 保存したスキンをアカウントに適用する。先にライブラリから取り出す（知らない ID は通信しない）。
pub async fn apply_saved(
    db: &Db,
    auth: &AuthService,
    account_id: Uuid,
    skin_id: &str,
) -> Result<(), CoreError> {
    let (png, variant) = blocking({
        let (db, skin_id) = (db.clone(), skin_id.to_string());
        move || library::png(&db, &skin_id)
    })
    .await?;
    auth.change_skin(account_id, variant, png).await
}

/// アカウントの現在のスキンを、まだ取り込んでいなければライブラリに取り込む。
/// スキンがない、または取得に失敗したときは `None` で、記録も残さない（次の機会にやり直せる）。
/// 型は Mojang のプロフィールのものを使い、名前はアカウントのユーザー名にする。
pub async fn import_current_skin(
    db: &Db,
    auth: &AuthService,
    account_id: Uuid,
    now_ms: i64,
) -> Result<Option<SavedSkin>, CoreError> {
    let Some((png, variant)) = auth.current_skin_raw(account_id).await? else {
        return Ok(None);
    };
    let db = db.clone();
    blocking(move || {
        let name = import_name(&accounts::get(&db, account_id)?.username);
        library::import_current(&db, account_id, &name, variant, &png, now_ms)
    })
    .await
}

/// DB の同期処理を実行スレッドから離して動かす（`await` をまたいで `Db` を握らない）。
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, CoreError> + Send + 'static,
) -> Result<T, CoreError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| CoreError::Unexpected {
            message: e.to_string(),
        })?
}

/// 取り込むスキンの名前。ユーザー名が空白だけでも、ライブラリの名前の規則で取り込みが
/// 失敗しない（記録が残らず、毎回失敗し続ける）ように、固定の名前に替える。
fn import_name(username: &str) -> String {
    let name = username.trim();
    if name.is_empty() {
        "Skin".to_string()
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_bounded_stops_at_the_limit_even_if_the_reader_has_more() {
        let exact = vec![1u8; SKIN_MAX_BYTES];
        assert_eq!(
            read_bounded(exact.as_slice()).unwrap().len(),
            SKIN_MAX_BYTES
        );
        // 無限に続く読み手でも、上限を 1 つ超えたところで止めて断る
        let err = read_bounded(std::io::repeat(7)).unwrap_err();
        assert!(matches!(&err, CoreError::InvalidSkin { reason } if reason == "too_large"));
    }

    #[test]
    fn import_name_falls_back_when_the_username_is_blank() {
        assert_eq!(import_name("  Steve "), "Steve");
        assert_eq!(import_name(""), "Skin");
        assert_eq!(import_name(" 	 "), "Skin");
    }
}
