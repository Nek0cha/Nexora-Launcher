//! ゲームの異常終了の診断。ログとクラッシュレポートから、原因の MOD の候補を返す。

use super::*;
use crate::diagnose::{self, CrashDiagnosis};

impl Launcher {
    /// `launched_at` の起動が異常終了した原因の、MOD の候補を求める。インスタンスは変更しない。
    /// ログやレポートが読めない場合は、候補なしを返す（エラーにしない）。
    pub async fn diagnose_crash(
        &self,
        id: &str,
        launched_at: i64,
    ) -> Result<CrashDiagnosis, CoreError> {
        let inst = instance::get(&self.env.db, id)?;
        let items = self.list_content(id, ContentKind::Mod).await?;
        let game_dir = instance::game_dir(&self.env.paths, &inst);
        let log_dir = self.env.paths.game_logs_dir().join(id);
        blocking(move || Ok(diagnose::diagnose(&items, &game_dir, &log_dir, launched_at))).await
    }
}
