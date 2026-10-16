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

    pub fn versions_dir(&self) -> PathBuf {
        self.shared_dir().join("versions")
    }

    pub fn libraries_dir(&self) -> PathBuf {
        self.shared_dir().join("libraries")
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.shared_dir().join("assets")
    }

    pub fn runtimes_dir(&self) -> PathBuf {
        self.shared_dir().join("runtimes")
    }

    /// 削除しても再取得できるメタデータ（manifest など）。
    pub fn meta_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("meta")
    }

    /// 起動のたびに展開し直す natives。
    pub fn natives_dir(&self) -> PathBuf {
        self.cache_dir().join("natives")
    }

    /// 起動用の一時ファイル（@argfile）。
    pub fn launch_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("launch")
    }

    /// インスタンスの起動に使う @argfile。
    pub fn argfile(&self, instance_id: &str) -> PathBuf {
        self.launch_cache_dir().join(format!("{instance_id}.args"))
    }

    /// ゲームの出力の書き出し先。
    pub fn game_logs_dir(&self) -> PathBuf {
        self.logs_dir().join("game")
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

use crate::error::CoreError;

/// 開発ビルドでデータフォルダを指定する環境変数。
pub const DATA_DIR_ENV: &str = "NEXORA_DATA_DIR";

impl AppPaths {
    /// OS の基本のデータフォルダに、プロファイルごとのフォルダ名を付け足す。
    /// Linux では dirs が $XDG_DATA_HOME を優先する。
    pub fn resolve(profile: Profile) -> Result<Self, CoreError> {
        let base = dirs::data_dir().ok_or(CoreError::DataDirUnavailable)?;
        Ok(Self::from_root(base.join(profile.dir_name())))
    }

    /// 開発ビルドで override_dir が空でなければ、それをルートにする。それ以外は resolve と同じ。
    pub fn resolve_with(
        profile: Profile,
        override_dir: Option<std::ffi::OsString>,
    ) -> Result<Self, CoreError> {
        if profile == Profile::Dev
            && let Some(dir) = override_dir.filter(|d| !d.is_empty())
        {
            return Ok(Self::from_root(PathBuf::from(dir)));
        }
        Self::resolve(profile)
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

    #[test]
    fn shared_and_cache_subdirectories() {
        let root = std::path::PathBuf::from("/tmp/nexora-test");
        let paths = AppPaths::from_root(&root);
        assert_eq!(paths.versions_dir(), root.join("shared").join("versions"));
        assert_eq!(paths.libraries_dir(), root.join("shared").join("libraries"));
        assert_eq!(paths.assets_dir(), root.join("shared").join("assets"));
        assert_eq!(paths.runtimes_dir(), root.join("shared").join("runtimes"));
        assert_eq!(paths.meta_cache_dir(), root.join("cache").join("meta"));
        assert_eq!(paths.natives_dir(), root.join("cache").join("natives"));
        assert_eq!(paths.launch_cache_dir(), root.join("cache").join("launch"));
        assert_eq!(
            paths.argfile("abc"),
            root.join("cache").join("launch").join("abc.args")
        );
        assert_eq!(paths.game_logs_dir(), root.join("logs").join("game"));
    }

    #[test]
    fn dev_profile_honours_override() {
        let paths = AppPaths::resolve_with(Profile::Dev, Some("/tmp/日本語/data".into())).unwrap();
        assert_eq!(paths.root(), std::path::Path::new("/tmp/日本語/data"));
    }

    #[test]
    fn empty_override_is_ignored() {
        let with_empty = AppPaths::resolve_with(Profile::Dev, Some("".into())).unwrap();
        let default = AppPaths::resolve(Profile::Dev).unwrap();
        assert_eq!(with_empty.root(), default.root());
    }

    #[test]
    fn release_profile_ignores_override() {
        let paths =
            AppPaths::resolve_with(Profile::Release, Some("/tmp/elsewhere".into())).unwrap();
        assert_eq!(
            paths.root(),
            AppPaths::resolve(Profile::Release).unwrap().root()
        );
    }
}
