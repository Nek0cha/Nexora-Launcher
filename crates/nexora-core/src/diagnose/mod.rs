//! ゲームの異常終了の診断。ログとクラッシュレポートから、原因の MOD の候補を求める。

mod clues;
mod modids;

use serde::Serialize;
use ts_rs::TS;

pub use clues::{extract_clues, stack_classes};
pub use modids::{jar_defines_class, read_mod_ids};

/// 原因の見つかった理由。強いものほど前に並ぶ（`Ord` の順）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CrashReason {
    MissingDependency,
    IncompatibleVersion,
    ModLoadingError,
    MixinFailure,
    SuspectedInReport,
    StackTrace,
}

/// ログやレポートの 1 つの手がかり。`mod_id` は原因として疑う MOD。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clue {
    pub mod_id: String,
    pub reason: CrashReason,
    /// 根拠として見せる行（短く切ってある）
    pub evidence: String,
    /// `MissingDependency` / `IncompatibleVersion` で、足りない側（または合わない側）の MOD ID
    pub missing: Option<String>,
}
