//! 設定の同期（共通設定）。保存層、登録表、options.txt の読み書きなど。

pub mod options;
pub mod registry;
pub mod store;
pub mod version;

use crate::db::Db;
use crate::paths::AppPaths;

/// 同期の処理が使う入口。DB とアプリのパスを借りる。
pub struct SyncCtx<'a> {
    pub db: &'a Db,
    pub paths: &'a AppPaths,
}
