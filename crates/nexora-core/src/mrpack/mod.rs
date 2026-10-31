//! Modrinth のモッドパック（`.mrpack`）のインポートとエクスポート。

pub mod archive;
pub mod manifest;
pub mod safe_path;

/// 読み込みの安全性のための上限。パックは信用しない入力として扱う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// `modrinth.index.json` の大きさ。
    pub manifest_bytes: u64,
    /// ZIP のエントリー数。
    pub entries: usize,
    /// 展開後の合計の大きさ。
    pub total_uncompressed: u64,
    /// 1 エントリーの展開後の大きさ ÷ 圧縮後の大きさ。
    pub ratio: u64,
    /// ダウンロードする 1 ファイルの大きさ。
    pub file_bytes: u64,
}

impl Limits {
    pub const DEFAULT: Limits = Limits {
        manifest_bytes: 5 * 1024 * 1024,
        entries: 50_000,
        total_uncompressed: 4 * 1024 * 1024 * 1024,
        ratio: 200,
        file_bytes: 512 * 1024 * 1024,
    };
}
