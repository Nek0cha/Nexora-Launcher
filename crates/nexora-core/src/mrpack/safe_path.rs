//! パックの中のパスの検査。ZIP のエントリー名と索引の `path` は、どちらも信用しない。

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use crate::error::CoreError;

fn unsafe_path(path: &str) -> CoreError {
    CoreError::MrpackUnsafePath {
        path: path.to_string(),
    }
}

/// Windows が予約しているデバイス名（拡張子の有無、大文字小文字を問わない）。
fn is_reserved_name(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            upper
                .strip_prefix(prefix)
                .is_some_and(|n| matches!(n.as_bytes(), [b'1'..=b'9']))
        })
}

fn is_unsafe_component(component: &str) -> bool {
    component.is_empty()
        || component == "."
        || component == ".."
        || component.ends_with('.')
        || component.ends_with(' ')
        || component
            .chars()
            .any(|c| c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*'))
        || is_reserved_name(component)
}

/// 相対パスとして安全かを調べ、`/` 区切りに直した `PathBuf` を返す。
/// `/` と `\` のどちらも区切りとして扱い、OS の解釈には頼らない。
pub fn check_rel_path(path: &str) -> Result<PathBuf, CoreError> {
    if path.is_empty() {
        return Err(unsafe_path(path));
    }
    let mut out = PathBuf::new();
    for component in path.split(['/', '\\']) {
        if is_unsafe_component(component) {
            return Err(unsafe_path(path));
        }
        out.push(component);
    }
    Ok(out)
}

/// 取り込むパスを 1 つずつ通し、大文字と小文字だけが違うパスの衝突を見つける。
#[derive(Debug, Default)]
pub struct PathGuard {
    seen: HashSet<String>,
}

impl PathGuard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn admit(&mut self, path: &str) -> Result<PathBuf, CoreError> {
        let rel = check_rel_path(path)?;
        let key = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect::<Vec<_>>()
            .join("/");
        if !self.seen.insert(key) {
            return Err(unsafe_path(path));
        }
        Ok(rel)
    }
}

/// すでにあるパスがシンボリックリンクか、Windows の再解析ポイントか。
fn is_link(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}

/// `dest` が `root` の下に収まるかを、成分ごとに調べる（文字列の前方一致は使わない）。
/// すでにある親ディレクトリがリンクなら、外へ出られるので拒否する。
pub fn ensure_contained(root: &Path, dest: &Path) -> Result<(), CoreError> {
    let refused = || unsafe_path(&dest.display().to_string());
    let rest = dest.strip_prefix(root).map_err(|_| refused())?;
    let mut current = root.to_path_buf();
    for component in rest.components() {
        match component {
            Component::Normal(name) => current.push(name),
            _ => return Err(refused()),
        }
        if is_link(&current) {
            return Err(refused());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn rejects_unsafe_paths() {
        let cases = [
            "",
            "/etc/passwd",
            "\\server\\share\\a",
            "C:\\a",
            "C:a",
            "../a",
            "a/../../b",
            "a\\..\\b",
            "./a",
            "a/b:stream",
            "con",
            "mods/NUL.jar",
            "mods/com1.txt",
            "mods/a. ",
            "mods/a.",
            "mods/a ",
            "a//b",
            "mods/",
            "mods/a\0b",
            "mods/a<b>.jar",
            "mods/a?.jar",
        ];
        for case in cases {
            let err = check_rel_path(case).unwrap_err();
            assert_eq!(err.code(), "mrpack.unsafe_path", "{case:?}");
        }
    }

    #[test]
    fn accepts_normal_paths() {
        for (case, expected) in [
            ("mods/sodium.jar", "mods/sodium.jar"),
            ("config/a b/日本語.toml", "config/a b/日本語.toml"),
            ("shaderpacks/x.zip", "shaderpacks/x.zip"),
            ("mods\\a.jar", "mods/a.jar"),
            ("console.txt", "console.txt"),
        ] {
            let p = check_rel_path(case).unwrap();
            let shown: Vec<_> = p
                .components()
                .map(|c| c.as_os_str().to_str().unwrap())
                .collect();
            assert_eq!(shown.join("/"), expected, "{case}");
        }
    }

    #[test]
    fn guard_detects_case_collisions() {
        let mut guard = PathGuard::new();
        guard.admit("mods/A.jar").unwrap();
        assert_eq!(
            guard.admit("mods/a.jar").unwrap_err().code(),
            "mrpack.unsafe_path"
        );
        assert_eq!(
            guard.admit("mods/A.jar").unwrap_err().code(),
            "mrpack.unsafe_path"
        );
        guard.admit("mods/b.jar").unwrap();
    }

    #[test]
    fn containment_is_by_component_not_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("inst");
        std::fs::create_dir_all(&root).unwrap();
        ensure_contained(&root, &root.join("mods").join("a.jar")).unwrap();
        let sibling = tmp.path().join("inst-evil").join("a");
        assert_eq!(
            ensure_contained(&root, &sibling).unwrap_err().code(),
            "mrpack.unsafe_path"
        );
        let escaped = root.join("..").join("x");
        assert_eq!(
            ensure_contained(&root, &escaped).unwrap_err().code(),
            "mrpack.unsafe_path"
        );
    }

    #[cfg(unix)]
    #[test]
    fn containment_rejects_a_symlinked_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("inst");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("mods")).unwrap();
        let err = ensure_contained(&root, &root.join("mods").join("a.jar")).unwrap_err();
        assert_eq!(err.code(), "mrpack.unsafe_path");
    }

    #[test]
    fn containment_allows_not_yet_created_parents() {
        let tmp = tempfile::tempdir().unwrap();
        let root: &Path = tmp.path();
        ensure_contained(root, &root.join("config").join("sub").join("a.toml")).unwrap();
    }
}
