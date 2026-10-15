//! OS ごとのデータの置き場所を求める。

use std::path::{Path, PathBuf};

/// ビルドの種類。開発ビルドは本番とは別のデータの置き場所を使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Release,
    Dev,
}

impl Profile {
    /// debug ビルドなら Dev、release ビルドなら Release。
    pub fn current() -> Self {
        if cfg!(debug_assertions) {
            Profile::Dev
        } else {
            Profile::Release
        }
    }

    pub fn dir_name(self) -> &'static str {
        match (self, cfg!(target_os = "linux")) {
            (Profile::Release, false) => "Nexora",
            (Profile::Dev, false) => "Nexora-dev",
            (Profile::Release, true) => "nexora",
            (Profile::Dev, true) => "nexora-dev",
        }
    }
}

/// データのルートと、その下のフォルダのパス。
#[derive(Debug, Clone)]
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    /// 任意のフォルダをルートにする。テストでは一時フォルダを渡す。
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn db_file(&self) -> PathBuf {
        self.root.join("nexora.db")
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.root.join("instances")
    }

    pub fn shared_dir(&self) -> PathBuf {
        self.root.join("shared")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.root.join("staging")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.root.join("backups")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn all_dirs(&self) -> [PathBuf; 7] {
        [
            self.root.clone(),
            self.instances_dir(),
            self.shared_dir(),
            self.cache_dir(),
            self.staging_dir(),
            self.backups_dir(),
            self.logs_dir(),
        ]
    }

    /// ルートとすべてのサブフォルダを作成する。既にあれば何もしない。
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for dir in self.all_dirs() {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_root_derives_all_subdirectories() {
        let root = std::path::PathBuf::from("/tmp/nexora-test");
        let paths = AppPaths::from_root(&root);
        assert_eq!(paths.root(), root.as_path());
        assert_eq!(paths.db_file(), root.join("nexora.db"));
        assert_eq!(paths.instances_dir(), root.join("instances"));
        assert_eq!(paths.shared_dir(), root.join("shared"));
        assert_eq!(paths.cache_dir(), root.join("cache"));
        assert_eq!(paths.staging_dir(), root.join("staging"));
        assert_eq!(paths.backups_dir(), root.join("backups"));
        assert_eq!(paths.logs_dir(), root.join("logs"));
    }

    #[test]
    fn ensure_dirs_creates_every_subdirectory() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("data"));
        paths.ensure_dirs().unwrap();
        for dir in paths.all_dirs() {
            assert!(dir.is_dir(), "{} was not created", dir.display());
        }
    }

    #[test]
    fn profile_directory_names() {
        let (release, dev) = if cfg!(target_os = "linux") {
            ("nexora", "nexora-dev")
        } else {
            ("Nexora", "Nexora-dev")
        };
        assert_eq!(Profile::Release.dir_name(), release);
        assert_eq!(Profile::Dev.dir_name(), dev);
    }
}
