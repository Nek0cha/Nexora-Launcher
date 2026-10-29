//! ライブラリの操作のうち、ファイルや通信をまたぐもの（Tauri 命令から呼ぶ）。
//! DB は同期の呼び出しの中でだけ使い、`await` をまたいで握らない。

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

/// OS から渡されたパスの PNG を読み、(拡張子を除くファイル名, 中身) を返す。
/// 通常のファイルだけを受け、読む前に大きさを調べて `SKIN_MAX_BYTES` を超えるものは読まない。
/// 画像そのものの検査（署名と寸法）は、ライブラリへ足すときの `validate_skin_png` が行う。
pub fn read_skin_file(path: &Path) -> Result<(String, Vec<u8>), CoreError> {
    let meta = std::fs::metadata(path).map_err(|e| io_error(path, e))?;
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
    let bytes = std::fs::read(path).map_err(|e| io_error(path, e))?;
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
    let (png, variant) = library::png(db, skin_id)?;
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
    let name = accounts::get(db, account_id)?.username;
    library::import_current(db, account_id, &name, variant, &png, now_ms)
}
