//! アプリが落ちて残った書きかけのファイル（`<name>.<uuid>.part`）を消す。

use std::path::Path;
use std::time::{Duration, SystemTime};

/// これより新しい `.part` は消さない。同じデータフォルダーを使う別のプロセスが書いている途中かもしれないため。
pub const STALE_AFTER: Duration = Duration::from_secs(10 * 60);

/// `part_path` が付ける名前（`.<32 桁の 16 進>.part` で終わる）か。
fn is_part_name(name: &str) -> bool {
    let Some(rest) = name.strip_suffix(".part") else {
        return false;
    };
    let Some((head, id)) = rest.rsplit_once('.') else {
        return false;
    };
    !head.is_empty() && id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `dir` の下（サブフォルダーを含む）の古い `.part` を消し、消した数を返す。
/// リンク（シンボリックリンク、ジャンクション）はたどらない。読めないフォルダーと消せないファイルは飛ばす。
pub fn sweep_stale_parts(dir: &Path, now: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            removed += sweep_stale_parts(&path, now);
            continue;
        }
        if !meta.is_file() || !entry.file_name().to_str().is_some_and(is_part_name) {
            continue;
        }
        let old = meta
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .is_some_and(|age| age >= STALE_AFTER);
        if old && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::part_path;

    #[test]
    fn recognises_only_part_path_names() {
        let made = part_path(Path::new("a/lib.jar"));
        assert!(is_part_name(made.file_name().unwrap().to_str().unwrap()));
        assert!(!is_part_name("lib.jar"));
        assert!(!is_part_name("notes.part"));
        assert!(!is_part_name("lib.jar.1234.part"));
        assert!(!is_part_name(".0123456789abcdef0123456789abcdef.part"));
    }

    #[test]
    fn removes_old_parts_in_subfolders_and_keeps_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("libraries").join("x");
        std::fs::create_dir_all(&nested).unwrap();
        let old = part_path(&nested.join("a.jar"));
        let fresh = part_path(&dir.path().join("b.jar"));
        let kept = nested.join("a.jar");
        for p in [&old, &fresh, &kept] {
            std::fs::write(p, b"x").unwrap();
        }
        // 古い方だけ、書いてから時間がたったことにする
        let file = std::fs::File::options().write(true).open(&old).unwrap();
        file.set_modified(SystemTime::now() - STALE_AFTER - Duration::from_secs(1))
            .unwrap();
        drop(file);

        assert_eq!(sweep_stale_parts(dir.path(), SystemTime::now()), 1);
        assert!(!old.exists());
        assert!(fresh.exists());
        assert!(kept.exists());
    }

    #[test]
    fn missing_folder_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            sweep_stale_parts(&dir.path().join("none"), SystemTime::now()),
            0
        );
    }
}
