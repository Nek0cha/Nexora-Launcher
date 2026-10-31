//! インスタンスのスクリーンショット（ゲームの `screenshots` フォルダ）の一覧、削除、取り込み。
//! 名前はフォルダ直下のファイル名だけを受け付け、フォルダの外には触れない。

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use ts_rs::TS;

use crate::error::CoreError;

/// 扱う画像の拡張子（ゲームが保存するのは PNG。取り込みでは JPEG も受ける）
const EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Screenshot {
    /// フォルダの中のファイル名
    pub name: String,
    /// 画像を表示するための絶対パス
    pub path: String,
    /// 更新日時（UNIX ミリ秒）。並び順と日付の表示に使う
    #[ts(type = "number")]
    pub modified_ms: i64,
    #[ts(type = "number")]
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportReport {
    pub added: u32,
    /// 画像ではない、または読めなかったファイルの数
    pub skipped: u32,
}

fn io_err(path: &Path, source: std::io::Error) -> CoreError {
    CoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn is_image_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// フォルダ直下の通常のファイル名か（区切りや `..` を含まない）。
fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0'])
}

/// 新しい順。フォルダが無ければ空。
pub fn list(dir: &Path) -> Result<Vec<Screenshot>, CoreError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_err(dir, e)),
    };
    let mut shots = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| io_err(dir, e))?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_image_name(&name) {
            continue;
        }
        // リンクは辿らない（フォルダの外を見せない）
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let modified_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        shots.push(Screenshot {
            path: entry.path().display().to_string(),
            name,
            modified_ms,
            size: meta.len(),
        });
    }
    shots.sort_by(|a, b| {
        b.modified_ms
            .cmp(&a.modified_ms)
            .then_with(|| b.name.cmp(&a.name))
    });
    Ok(shots)
}

pub fn delete(dir: &Path, name: &str) -> Result<(), CoreError> {
    if !is_plain_name(name) || !is_image_name(name) {
        return Err(CoreError::ScreenshotNotFound);
    }
    let path = dir.join(name);
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_file() => std::fs::remove_file(&path).map_err(|e| io_err(&path, e)),
        Ok(_) => Err(CoreError::ScreenshotNotFound),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(CoreError::ScreenshotNotFound),
        Err(e) => Err(io_err(&path, e)),
    }
}

/// 同じ名前があるときは `name-1.png` のように番号を付けて、上書きしない。
fn free_target(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("screenshot");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("png");
    (1u32..)
        .map(|n| dir.join(format!("{stem}-{n}.{ext}")))
        .find(|p| !p.exists())
        .expect("an unused name exists")
}

/// 画像のファイルをフォルダへ写す。画像でないものと読めないものは数えて飛ばす。
pub fn import(dir: &Path, sources: &[PathBuf]) -> Result<ImportReport, CoreError> {
    std::fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    let mut report = ImportReport {
        added: 0,
        skipped: 0,
    };
    for source in sources {
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| is_image_name(n));
        let usable = std::fs::metadata(source).is_ok_and(|m| m.is_file());
        let Some(name) = name.filter(|_| usable) else {
            report.skipped += 1;
            continue;
        };
        let target = free_target(dir, name);
        match std::fs::copy(source, &target) {
            Ok(_) => report.added += 1,
            Err(e) => {
                tracing::warn!(%e, path = %source.display(), "could not import a screenshot");
                report.skipped += 1;
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

    fn touch(dir: &Path, name: &str, age_secs: u64) {
        let path = dir.join(name);
        std::fs::write(&path, b"x").unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(age_secs))
            .unwrap();
    }

    #[test]
    fn lists_images_newest_first_and_ignores_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "old.png", 100);
        touch(dir.path(), "new.PNG", 10);
        touch(dir.path(), "mid.jpg", 50);
        touch(dir.path(), "notes.txt", 5);
        std::fs::create_dir(dir.path().join("sub.png")).unwrap();
        let names: Vec<_> = list(dir.path())
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["new.PNG", "mid.jpg", "old.png"]);
    }

    #[test]
    fn a_missing_folder_is_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list(&dir.path().join("none")).unwrap().is_empty());
    }

    #[test]
    fn delete_removes_only_an_image_directly_in_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("outside.png");
        std::fs::write(&outside, b"x").unwrap();
        let shots = dir.path().join("shots");
        std::fs::create_dir(&shots).unwrap();
        touch(&shots, "a.png", 1);
        touch(&shots, "keep.txt", 1);

        delete(&shots, "a.png").unwrap();
        assert!(!shots.join("a.png").exists());
        for bad in [
            "../outside.png",
            "..\\outside.png",
            "keep.txt",
            "",
            "missing.png",
        ] {
            assert!(
                matches!(delete(&shots, bad), Err(CoreError::ScreenshotNotFound)),
                "{bad}"
            );
        }
        assert!(outside.exists());
        assert!(shots.join("keep.txt").exists());
    }

    #[test]
    fn import_copies_images_without_overwriting() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let shots = dst.path().join("screenshots");
        touch(src.path(), "a.png", 1);
        touch(src.path(), "b.txt", 1);
        std::fs::write(src.path().join("c.jpg"), b"y").unwrap();
        let sources = vec![
            src.path().join("a.png"),
            src.path().join("b.txt"),
            src.path().join("c.jpg"),
            src.path().join("gone.png"),
        ];

        let first = import(&shots, &sources).unwrap();
        assert_eq!(
            first,
            ImportReport {
                added: 2,
                skipped: 2
            }
        );
        let again = import(&shots, &sources[..1]).unwrap();
        assert_eq!(again.added, 1);
        let mut names: Vec<_> = list(&shots).unwrap().into_iter().map(|s| s.name).collect();
        names.sort();
        assert_eq!(names, ["a-1.png", "a.png", "c.jpg"]);
    }
}
