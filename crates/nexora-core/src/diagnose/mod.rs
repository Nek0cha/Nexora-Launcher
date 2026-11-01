//! ゲームの異常終了の診断。ログとクラッシュレポートから、原因の MOD の候補を求める。

mod modids;

pub use modids::{jar_defines_class, read_mod_ids};
