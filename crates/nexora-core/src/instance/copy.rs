//! インスタンスの複製で使う、ディレクトリの再帰的なコピー。

use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

use crate::error::CoreError;

fn io_error(path: &Path, source: std::io::Error) -> CoreError {
    CoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn check_cancel(cancel: &CancellationToken) -> Result<(), CoreError> {
    if cancel.is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    Ok(())
}

/// `src` の中身を、新しく作る `dst` へ再帰的にコピーする。
///
/// `dst` がすでにあればエラーにする（既存のディレクトリに混ぜて書かないため）。
/// 進捗は `(コピーしたファイル数, ファイルの総数)` で通知する。
/// シンボリックリンク（Windows のジャンクションを含む）はたどらず、リンクとしても作らない。
/// 失敗や取り消しのとき、作りかけの `dst` は残る。後始末は呼び出し側で行う。
pub fn copy_dir(
    src: &Path,
    dst: &Path,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), CoreError> {
    std::fs::create_dir(dst).map_err(|e| io_error(dst, e))?;
    let (dirs, files) = collect(src, cancel)?;
    let total = files.len() as u64;
    progress(0, total);
    for rel in &dirs {
        check_cancel(cancel)?;
        let target = dst.join(rel);
        std::fs::create_dir(&target).map_err(|e| io_error(&target, e))?;
    }
    for (done, rel) in files.iter().enumerate() {
        check_cancel(cancel)?;
        let from = src.join(rel);
        std::fs::copy(&from, dst.join(rel)).map_err(|e| io_error(&from, e))?;
        progress(done as u64 + 1, total);
    }
    Ok(())
}

/// `src` の下のディレクトリとファイルを、`src` からの相対パスで集める。
/// ディレクトリは親が子より先に並ぶ。
fn collect(
    src: &Path,
    cancel: &CancellationToken,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>), CoreError> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(rel) = pending.pop() {
        check_cancel(cancel)?;
        let dir = src.join(&rel);
        for entry in std::fs::read_dir(&dir).map_err(|e| io_error(&dir, e))? {
            let entry = entry.map_err(|e| io_error(&dir, e))?;
            let path = entry.path();
            // DirEntry::file_type はリンクをたどらない
            let kind = entry.file_type().map_err(|e| io_error(&path, e))?;
            let child = rel.join(entry.file_name());
            if kind.is_symlink() {
                tracing::warn!(path = %path.display(), "skipping a symbolic link while copying");
            } else if kind.is_dir() {
                dirs.push(child.clone());
                pending.push(child);
            } else if kind.is_file() {
                files.push(child);
            } else {
                tracing::warn!(path = %path.display(), "skipping a special file while copying");
            }
        }
    }
    Ok((dirs, files))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn source() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("src");
        write(&src.join("options.txt"), "a");
        write(&src.join("saves/world/level.dat"), "b");
        write(&src.join("saves/world/region/r.0.0.mca"), "c");
        std::fs::create_dir_all(src.join("empty")).unwrap();
        let dst = temp.path().join("dst");
        (temp, src, dst)
    }

    #[test]
    fn copies_nested_files_and_reports_progress() {
        let (_t, src, dst) = source();
        let mut calls = Vec::new();
        copy_dir(&src, &dst, &CancellationToken::new(), &mut |c, t| {
            calls.push((c, t))
        })
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(dst.join("options.txt")).unwrap(),
            "a"
        );
        assert_eq!(
            std::fs::read_to_string(dst.join("saves/world/level.dat")).unwrap(),
            "b"
        );
        assert_eq!(
            std::fs::read_to_string(dst.join("saves/world/region/r.0.0.mca")).unwrap(),
            "c"
        );
        assert!(dst.join("empty").is_dir());
        assert_eq!(calls, vec![(0, 3), (1, 3), (2, 3), (3, 3)]);
        // 元は変わらない
        assert_eq!(
            std::fs::read_to_string(src.join("options.txt")).unwrap(),
            "a"
        );
    }

    #[test]
    fn cancel_stops_copy() {
        let (_t, src, dst) = source();
        let token = CancellationToken::new();
        let mut copied = 0;
        let err = copy_dir(&src, &dst, &token, &mut |c, _| {
            copied = c;
            if c == 1 {
                token.cancel();
            }
        })
        .unwrap_err();
        assert_eq!(err.code(), "task.cancelled");
        assert_eq!(copied, 1);
    }

    #[test]
    fn cancel_before_start_copies_nothing() {
        let (_t, src, dst) = source();
        let token = CancellationToken::new();
        token.cancel();
        let err = copy_dir(&src, &dst, &token, &mut |_, _| {}).unwrap_err();
        assert_eq!(err.code(), "task.cancelled");
        assert!(!dst.join("options.txt").exists());
    }

    #[test]
    fn existing_destination_is_refused() {
        let (_t, src, dst) = source();
        write(&dst.join("mine.txt"), "keep");
        let err = copy_dir(&src, &dst, &CancellationToken::new(), &mut |_, _| {}).unwrap_err();
        assert_eq!(err.code(), "io.failed");
        // 既存のディレクトリには書き込まない
        assert!(!dst.join("options.txt").exists());
        assert_eq!(
            std::fs::read_to_string(dst.join("mine.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn missing_source_fails() {
        let temp = tempfile::tempdir().unwrap();
        let err = copy_dir(
            &temp.path().join("nope"),
            &temp.path().join("dst"),
            &CancellationToken::new(),
            &mut |_, _| {},
        )
        .unwrap_err();
        assert_eq!(err.code(), "io.failed");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_is_skipped() {
        let (temp, src, dst) = source();
        let outside = temp.path().join("outside");
        write(&outside.join("secret.txt"), "s");
        std::os::unix::fs::symlink(outside.join("secret.txt"), src.join("file_link")).unwrap();
        std::os::unix::fs::symlink(&outside, src.join("dir_link")).unwrap();
        let mut calls = Vec::new();
        copy_dir(&src, &dst, &CancellationToken::new(), &mut |c, t| {
            calls.push((c, t))
        })
        .unwrap();
        // リンクはたどらず、リンクとしても作らない
        assert!(std::fs::symlink_metadata(dst.join("file_link")).is_err());
        assert!(std::fs::symlink_metadata(dst.join("dir_link")).is_err());
        assert_eq!(calls.last(), Some(&(3, 3)));
    }
}
