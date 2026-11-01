//! 元のフォルダを、リンクをたどらずにインスタンスのフォルダへコピーする。

use std::fs;
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

use crate::error::CoreError;
use crate::mrpack::safe_path::ensure_contained;

/// コピーした結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyReport {
    pub files: u64,
    pub bytes: u64,
    /// たどらずに飛ばしたリンク（ジャンクションを含む）の数。
    pub skipped_links: u64,
}

/// シンボリックリンクか、Windows のジャンクションか。読み込み元ではこれだけをたどらない。
/// クラウド同期の未ダウンロードのファイルなど、ほかの再解析ポイントは普通のファイルとして読む。
pub(crate) fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

fn io_err(path: &Path, source: std::io::Error) -> CoreError {
    CoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// 元のフォルダにあるファイル（相対パスと大きさ）と、飛ばすリンクの数。
struct Plan {
    dirs: Vec<PathBuf>,
    files: Vec<(PathBuf, u64)>,
    skipped_links: u64,
}

fn plan(src: &Path, token: &CancellationToken) -> Result<Plan, CoreError> {
    let mut plan = Plan {
        dirs: Vec::new(),
        files: Vec::new(),
        skipped_links: 0,
    };
    if is_link(src) {
        plan.skipped_links = 1;
        return Ok(plan);
    }
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        if token.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let dir = src.join(&rel);
        for entry in fs::read_dir(&dir).map_err(|e| io_err(&dir, e))? {
            let entry = entry.map_err(|e| io_err(&dir, e))?;
            let path = entry.path();
            let rel_path = rel.join(entry.file_name());
            if is_link(&path) {
                plan.skipped_links += 1;
                continue;
            }
            let meta = fs::symlink_metadata(&path).map_err(|e| io_err(&path, e))?;
            if meta.is_dir() {
                plan.dirs.push(rel_path.clone());
                stack.push(rel_path);
            } else if meta.is_file() {
                plan.files.push((rel_path, meta.len()));
            }
        }
    }
    Ok(plan)
}

/// 1 つのファイルを `dest` へコピーする。
fn copy_one(
    src: &Path,
    dest: &Path,
    token: &CancellationToken,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<CopyReport, CoreError> {
    if token.is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    let bytes = fs::copy(src, dest).map_err(|e| io_err(src, e))?;
    progress(bytes, bytes);
    Ok(CopyReport {
        files: 1,
        bytes,
        skipped_links: 0,
    })
}

/// `src` の中身を `dest` へコピーする。`dest` は `dest_root` の下でなければならない。
/// リンクはたどらず数えるだけ。`progress(done, total)` はバイト数で呼ぶ。
pub fn copy_tree(
    src: &Path,
    dest_root: &Path,
    dest: &Path,
    token: &CancellationToken,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<CopyReport, CoreError> {
    ensure_contained(dest_root, dest)?;
    if !is_link(src) && fs::metadata(src).is_ok_and(|m| m.is_file()) {
        return copy_one(src, dest, token, progress);
    }
    let plan = plan(src, token)?;
    let total: u64 = plan.files.iter().map(|(_, len)| len).sum();
    fs::create_dir_all(dest).map_err(|e| io_err(dest, e))?;
    for rel in &plan.dirs {
        let path = dest.join(rel);
        fs::create_dir_all(&path).map_err(|e| io_err(&path, e))?;
    }
    let mut report = CopyReport {
        files: 0,
        bytes: 0,
        skipped_links: plan.skipped_links,
    };
    progress(0, total);
    for (rel, _) in &plan.files {
        if token.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let (from, to) = (src.join(rel), dest.join(rel));
        let copied = fs::copy(&from, &to).map_err(|e| io_err(&from, e))?;
        report.files += 1;
        report.bytes += copied;
        progress(report.bytes, total);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tokio_util::sync::CancellationToken;

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    /// 元の外を指すリンク（Windows ではジャンクション）を作る。作れなければ `false`。
    fn make_link(link: &Path, target: &Path) -> bool {
        #[cfg(windows)]
        {
            std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
    }

    fn run(
        src: &Path,
        root: &Path,
        dest: &Path,
        token: &CancellationToken,
    ) -> Result<CopyReport, CoreError> {
        copy_tree(src, root, dest, token, &mut |_, _| {})
    }

    #[test]
    fn copies_nested_files_and_leaves_the_source_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        write(&src.join("a.jar"), b"aaa");
        write(&src.join("world/region/r.mca"), b"rrrr");
        let root = tmp.path().join("inst");
        let dest = root.join("saves");
        let report = run(&src, &root, &dest, &CancellationToken::new()).unwrap();
        assert_eq!(fs::read(dest.join("a.jar")).unwrap(), b"aaa");
        assert_eq!(fs::read(dest.join("world/region/r.mca")).unwrap(), b"rrrr");
        assert_eq!(
            (report.files, report.bytes, report.skipped_links),
            (2, 7, 0)
        );
        assert_eq!(fs::read(src.join("a.jar")).unwrap(), b"aaa");
    }

    #[test]
    fn copies_a_single_file_to_the_destination_path() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("options.txt");
        write(&src, b"fov:90");
        let root = tmp.path().join("inst");
        let dest = root.join("options.txt");
        let report = run(&src, &root, &dest, &CancellationToken::new()).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"fov:90");
        assert_eq!((report.files, report.bytes), (1, 6));
        assert_eq!(fs::read(&src).unwrap(), b"fov:90");
    }

    #[test]
    fn a_single_file_cannot_land_outside_the_instance() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("servers.dat");
        write(&src, b"x");
        let root = tmp.path().join("inst");
        let err = run(
            &src,
            &root,
            &tmp.path().join("elsewhere.dat"),
            &CancellationToken::new(),
        )
        .unwrap_err();
        assert_eq!(err.code(), "mrpack.unsafe_path");
    }

    #[test]
    fn reports_progress_up_to_the_total() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        write(&src.join("a"), &[0u8; 10]);
        write(&src.join("b"), &[0u8; 20]);
        let root = tmp.path().join("inst");
        let mut last = (0, 0);
        copy_tree(
            &src,
            &root,
            &root.join("mods"),
            &CancellationToken::new(),
            &mut |d, t| last = (d, t),
        )
        .unwrap();
        assert_eq!(last, (30, 30));
    }

    #[test]
    fn refuses_a_destination_outside_the_instance() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        write(&src.join("a"), b"x");
        let root = tmp.path().join("inst");
        let outside = tmp.path().join("elsewhere");
        let err = run(&src, &root, &outside, &CancellationToken::new()).unwrap_err();
        assert_eq!(err.code(), "mrpack.unsafe_path");
        assert!(!outside.exists());
    }

    #[test]
    fn skips_links_without_following_them() {
        let tmp = tempfile::tempdir().unwrap();
        let secret = tmp.path().join("secret");
        write(&secret.join("pw.txt"), b"secret");
        let src = tmp.path().join("src");
        write(&src.join("keep.txt"), b"k");
        if !make_link(&src.join("jump"), &secret) {
            return;
        }
        let root = tmp.path().join("inst");
        let dest = root.join("saves");
        let report = run(&src, &root, &dest, &CancellationToken::new()).unwrap();
        assert_eq!(report.skipped_links, 1);
        assert!(dest.join("keep.txt").exists());
        assert!(!dest.join("jump").exists());
        assert!(secret.join("pw.txt").exists());
    }

    #[test]
    fn a_cancelled_token_stops_the_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        write(&src.join("a"), b"x");
        let token = CancellationToken::new();
        token.cancel();
        let root = tmp.path().join("inst");
        let err = run(&src, &root, &root.join("mods"), &token).unwrap_err();
        assert_eq!(err.code(), "task.cancelled");
    }

    #[test]
    fn an_unreadable_source_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("inst");
        let err = run(
            &tmp.path().join("missing"),
            &root,
            &root.join("mods"),
            &CancellationToken::new(),
        )
        .unwrap_err();
        assert_eq!(err.code(), "io.failed");
    }
}
