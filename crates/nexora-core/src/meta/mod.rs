//! ゲームとローダーのメタデータ。

pub mod loader;
pub mod mojang;

use std::path::Path;

use serde::de::DeserializeOwned;

use crate::error::CoreError;
use crate::net::{Conditional, Http, io_err, part_path};

pub fn parse_json<T: DeserializeOwned>(bytes: &[u8], what: &str) -> Result<T, CoreError> {
    serde_json::from_slice(bytes).map_err(|e| CoreError::InvalidMetadata {
        message: format!("{what}: {e}"),
    })
}

/// `.part` に書いてからリネームする。`.part` の名前は呼ぶたびに一意なので、
/// 同じ path に同時に書いても互いの書きかけを壊さない（最後のリネームが残る）。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|s| io_err(parent, s))?;
    }
    let part = part_path(path);
    let result = std::fs::write(&part, bytes)
        .map_err(|s| io_err(&part, s))
        .and_then(|()| std::fs::rename(&part, path).map_err(|s| io_err(path, s)));
    if result.is_err() {
        // 自分の `.part` だけを消す
        let _ = std::fs::remove_file(&part);
    }
    result
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, CoreError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_err(path, source)),
    }
}

/// ETag 付きでキャッシュする取得。通信できないときや 5xx のときは、キャッシュがあればそれを返す。
/// 取得した本文は、呼び出し側が parse に成功してから使う前提で、ここでは中身を検証しない。
pub async fn fetch_cached(
    http: &Http,
    cache_dir: &Path,
    name: &str,
    url: &str,
) -> Result<Vec<u8>, CoreError> {
    let body_path = cache_dir.join(name);
    let etag_path = cache_dir.join(format!("{name}.etag"));
    let cached = read_optional(&body_path)?;
    let etag = match &cached {
        Some(_) => read_optional(&etag_path)?.and_then(|b| String::from_utf8(b).ok()),
        None => None,
    };
    match http.get_conditional(url, etag.as_deref()).await {
        Ok(Conditional::Fresh { body, etag }) => {
            write_atomic(&body_path, &body)?;
            match etag {
                Some(tag) => write_atomic(&etag_path, tag.as_bytes())?,
                None => {
                    let _ = std::fs::remove_file(&etag_path);
                }
            }
            Ok(body)
        }
        Ok(Conditional::NotModified) => cached.ok_or_else(|| CoreError::Unexpected {
            message: format!("304 without a cached copy of {name}"),
        }),
        Err(err) => {
            let recoverable = matches!(
                err,
                CoreError::Offline { .. } | CoreError::HttpStatus { status: 500.., .. }
            );
            match cached {
                Some(body) if recoverable => {
                    tracing::warn!(name, code = err.code(), "using cached metadata");
                    Ok(body)
                }
                _ => Err(err),
            }
        }
    }
}
