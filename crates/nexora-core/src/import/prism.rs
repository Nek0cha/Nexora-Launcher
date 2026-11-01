//! Prism Launcher（MultiMC 系を含む）のインスタンスを読む。実装は未着手。

use std::path::PathBuf;

use super::{ImportSource, SourceInstance};
use crate::error::CoreError;

pub struct PrismLauncher {
    #[allow(dead_code)]
    root: PathBuf,
}

impl PrismLauncher {
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// 既定の場所にデータがあれば、そのフォルダ。
    pub fn locate() -> Option<PathBuf> {
        None
    }
}

impl ImportSource for PrismLauncher {
    fn list(&self) -> Result<Vec<SourceInstance>, CoreError> {
        Err(CoreError::ImportUnavailable {
            reason: "not implemented yet".into(),
        })
    }

    fn source_path(&self, _id: &str, _part: &str) -> Result<Option<PathBuf>, CoreError> {
        Err(CoreError::ImportUnavailable {
            reason: "not implemented yet".into(),
        })
    }
}
