//! インスタンスのゲームのログ（起動ごとに `logs/game/<id>/<起動時刻>.log`）の一覧と読み取り。
//! 名前は `<数字>.log` だけを受け付け、フォルダの外には触れない。

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

use crate::error::CoreError;

/// 1 回に読む末尾の大きさ。デバッグ出力で数十 MB になるログでも、画面が重くならないようにする。
pub const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameLog {
    /// フォルダの中のファイル名（`<起動時刻>.log`）
    pub name: String,
    /// 起動した日時（UNIX ミリ秒）。ファイル名から取る
    #[ts(type = "number")]
    pub launched_at: i64,
    #[ts(type = "number")]
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LogContent {
    pub text: String,
    /// 大きすぎて、先頭を読んでいない
    pub truncated: bool,
}

fn io_err(path: &Path, source: std::io::Error) -> CoreError {
    CoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// `<数字>.log` なら起動時刻を返す。
fn launched_at(name: &str) -> Option<i64> {
    let stem = name.strip_suffix(".log")?;
    if stem.is_empty() || !stem.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    stem.parse().ok()
}

/// 新しい順。フォルダが無ければ空。
pub fn list(dir: &Path) -> Result<Vec<GameLog>, CoreError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_err(dir, e)),
    };
    let mut logs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| io_err(dir, e))?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(launched_at) = launched_at(&name) else {
            continue;
        };
        // リンクは辿らない
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        logs.push(GameLog {
            name,
            launched_at,
            size: meta.len(),
        });
    }
    logs.sort_by_key(|l| std::cmp::Reverse(l.launched_at));
    Ok(logs)
}

/// ログの末尾（最大 `MAX_READ_BYTES`）を文字列で返す。途中から読んだときは、切れた最初の行を捨てる。
/// 文字として読めない部分は置き換える（ゲームは UTF-8 以外で書くことがある）。
pub fn read(dir: &Path, name: &str) -> Result<LogContent, CoreError> {
    if launched_at(name).is_none() {
        return Err(CoreError::LogNotFound);
    }
    let path = dir.join(name);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_file() => meta,
        Ok(_) => return Err(CoreError::LogNotFound),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(CoreError::LogNotFound),
        Err(e) => return Err(io_err(&path, e)),
    };
    let mut file = std::fs::File::open(&path).map_err(|e| io_err(&path, e))?;
    let truncated = meta.len() > MAX_READ_BYTES;
    if truncated {
        file.seek(SeekFrom::Start(meta.len() - MAX_READ_BYTES))
            .map_err(|e| io_err(&path, e))?;
    }
    let mut bytes = Vec::new();
    file.take(MAX_READ_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|e| io_err(&path, e))?;
    if truncated {
        // 行の途中から始まっているので、最初の改行までを捨てる
        let start = bytes.iter().position(|&b| b == b'\n').map_or(0, |i| i + 1);
        bytes.drain(..start);
    }
    Ok(LogContent {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_only_launch_logs_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "100.log",
            "300.log",
            "200.log",
            "latest.log",
            "5.txt",
            ".log",
        ] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        std::fs::create_dir(dir.path().join("400.log")).unwrap();
        let logs = list(dir.path()).unwrap();
        let names: Vec<_> = logs.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["300.log", "200.log", "100.log"]);
        assert_eq!(logs[0].launched_at, 300);
        assert_eq!(logs[0].size, 1);
    }

    #[test]
    fn a_missing_folder_is_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list(&dir.path().join("none")).unwrap().is_empty());
    }

    #[test]
    fn reads_a_small_log_whole() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("1.log"), "a\nb\n").unwrap();
        let content = read(dir.path(), "1.log").unwrap();
        assert_eq!(
            content,
            LogContent {
                text: "a\nb\n".into(),
                truncated: false
            }
        );
    }

    #[test]
    fn a_large_log_keeps_the_end_and_drops_the_cut_line() {
        let dir = tempfile::tempdir().unwrap();
        let line = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde\n";
        let count = (MAX_READ_BYTES as usize / line.len()) + 10;
        std::fs::write(dir.path().join("2.log"), line.repeat(count)).unwrap();
        let content = read(dir.path(), "2.log").unwrap();
        assert!(content.truncated);
        assert!(content.text.len() as u64 <= MAX_READ_BYTES);
        // 切れた行は残らず、残った行はすべて完全
        assert!(content.text.lines().all(|l| l.len() == line.len() - 1));
        assert!(content.text.ends_with('\n'));
    }

    #[test]
    fn invalid_bytes_are_replaced() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("3.log"), [b'o', b'k', 0xff, b'\n']).unwrap();
        assert_eq!(read(dir.path(), "3.log").unwrap().text, "ok\u{fffd}\n");
    }

    #[test]
    fn rejects_names_outside_the_folder_or_not_launch_logs() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        std::fs::create_dir(&logs).unwrap();
        std::fs::write(dir.path().join("9.log"), b"secret").unwrap();
        for bad in ["../9.log", "..\\9.log", "latest.log", "", "7.log"] {
            assert!(
                matches!(read(&logs, bad), Err(CoreError::LogNotFound)),
                "{bad}"
            );
        }
    }
}
