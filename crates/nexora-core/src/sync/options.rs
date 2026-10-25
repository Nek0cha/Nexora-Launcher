//! options.txt の読み書き。管理対象の行だけを書き換え、他の行は元のバイト列のまま残す。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::CoreError;

/// 残すバックアップの件数。
const BACKUP_KEEP: usize = 10;

/// 1 行。`body` は改行を除いた本文、`eol` はその行の改行（最終行が無改行なら空）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    body: String,
    eol: &'static str,
}

impl Line {
    /// 最初の `:` で分けたキーと値。コロンが無い行は `None`。
    fn split(&self) -> Option<(&str, &str)> {
        self.body.split_once(':')
    }
}

/// options.txt を行の列として保持する。行ごとの改行も保持するので、
/// 触れない行は `render` で元のまま戻る（LF、CRLF、混在、最終行の改行なしを含む）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsFile {
    lines: Vec<Line>,
}

impl OptionsFile {
    pub fn parse(text: &str) -> OptionsFile {
        let mut lines = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            match rest.find('\n') {
                Some(i) => {
                    let content = &rest[..i];
                    let (body, eol) = match content.strip_suffix('\r') {
                        Some(b) => (b, "\r\n"),
                        None => (content, "\n"),
                    };
                    lines.push(Line {
                        body: body.to_string(),
                        eol,
                    });
                    rest = &rest[i + 1..];
                }
                None => {
                    lines.push(Line {
                        body: rest.to_string(),
                        eol: "",
                    });
                    rest = "";
                }
            }
        }
        OptionsFile { lines }
    }

    /// キーの値。同じキーが複数あるときは、ゲームの読み方に合わせて最後の行を返す。
    pub fn get(&self, key: &str) -> Option<&str> {
        self.lines
            .iter()
            .rev()
            .filter_map(Line::split)
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v)
    }

    /// `key:value` の組を並び順どおりに返す。同じキーが複数あれば、`get` と同じく最後の行だけ。
    pub fn entries(&self) -> Vec<(&str, &str)> {
        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<(&str, &str)> = self
            .lines
            .iter()
            .rev()
            .filter_map(Line::split)
            .filter(|(k, _)| seen.insert(*k))
            .collect();
        out.reverse();
        out
    }

    /// `version:` の行（DataVersion）。
    pub fn version(&self) -> Option<i64> {
        self.get("version")?.trim().parse().ok()
    }

    /// 既存の `key:value` の行だけを書き換える。無いキーは足さない。
    /// 同じキーの行が複数あれば、すべて同じ値にする。改行を含む値は行を壊すので無視する。
    pub fn set_all(&mut self, updates: &[(&str, String)]) {
        for (key, value) in updates {
            if key.is_empty() || value.contains(['\n', '\r']) {
                continue;
            }
            for line in &mut self.lines {
                if let Some((k, _)) = line.split()
                    && k == *key
                {
                    line.body = format!("{key}:{value}");
                }
            }
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            out.push_str(&line.body);
            out.push_str(line.eol);
        }
        out
    }
}

fn io_err(path: &Path, source: std::io::Error) -> CoreError {
    CoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// `options-<数字>.txt` の数字部分。
fn backup_stamp(name: &str) -> Option<u128> {
    let digits = name.strip_prefix("options-")?.strip_suffix(".txt")?;
    // `+5` のような符号付きは受け付けない（数字だけの名前に限る）
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// 古いバックアップを消して `BACKUP_KEEP` 件にする。`keep` は今作ったもので、時計が戻っても消さない。
/// 失敗は警告のログだけで、書き込みは止めない。
fn prune_backups(dir: &Path, keep: &Path) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("バックアップの一覧を読めなかった: {}: {e}", dir.display());
            return;
        }
    };
    let mut found: Vec<(u128, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let stamp = backup_stamp(e.file_name().to_str()?)?;
            Some((stamp, e.path()))
        })
        .filter(|(_, p)| p != keep)
        .collect();
    found.sort();
    // 今作った 1 件を数えて、残りの枠は BACKUP_KEEP - 1 件
    let excess = found.len().saturating_sub(BACKUP_KEEP - 1);
    for (_, path) in found.into_iter().take(excess) {
        if let Err(e) = fs::remove_file(&path) {
            tracing::warn!("古いバックアップを消せなかった: {}: {e}", path.display());
        }
    }
}

/// バックアップ → `<path>.tmp` → rename の順で書き換える。
/// `path` が無いときはバックアップを作らず、そのまま新規に書く。
/// 古い `.tmp` が残っていても上書きする。失敗しても `path` 本体は書きかけにならない。
pub fn write_atomic(
    path: &Path,
    backups_dir: &Path,
    instance_id: &str,
    text: &str,
) -> Result<(), CoreError> {
    if path.is_file() {
        let dir = backups_dir.join("sync").join(instance_id);
        fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
        // 同じミリ秒の衝突は 1 ずつ進めて避ける
        let mut stamp = unix_ms();
        let backup = loop {
            let candidate = dir.join(format!("options-{stamp}.txt"));
            if !candidate.exists() {
                break candidate;
            }
            stamp += 1;
        };
        fs::copy(path, &backup).map_err(|e| io_err(&backup, e))?;
        prune_backups(&dir, &backup);
    }

    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp = PathBuf::from(tmp_name);
    // 書いた内容をディスクへ出してから rename する（電源断で空の options.txt にならないように）
    let write_tmp = || -> std::io::Result<()> {
        use std::io::Write;
        let mut file = fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()
    };
    if let Err(e) = write_tmp() {
        let _ = fs::remove_file(&tmp);
        return Err(io_err(&tmp, e));
    }
    // Windows でも std::fs::rename は既存の宛先を置き換える
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(io_err(path, e));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODERN: &str = include_str!("../../tests/fixtures/options/modern.txt");
    const LEGACY: &str = include_str!("../../tests/fixtures/options/legacy.txt");
    const CRLF: &str = include_str!("../../tests/fixtures/options/crlf.txt");
    const NO_EOL: &str = include_str!("../../tests/fixtures/options/no_trailing_newline.txt");

    #[test]
    fn fixtures_keep_their_line_endings() {
        assert!(CRLF.contains("\r\n"));
        assert!(!MODERN.contains('\r'));
        assert!(!NO_EOL.ends_with('\n'));
    }

    #[test]
    fn parse_render_roundtrips_byte_for_byte() {
        let mixed = "a:1\r\nb:2\nno colon\n\n\r\nc:3\r";
        for text in [MODERN, LEGACY, CRLF, NO_EOL, "", "\n", "x", mixed] {
            assert_eq!(OptionsFile::parse(text).render(), text);
        }
    }

    #[test]
    fn set_all_changes_only_the_named_lines() {
        let mut f = OptionsFile::parse(MODERN);
        f.set_all(&[
            ("renderDistance", "20".into()),
            ("key_key.forward", "key.keyboard.up".into()),
        ]);
        let want = MODERN
            .replace("renderDistance:12", "renderDistance:20")
            .replace(
                "key_key.forward:key.keyboard.w",
                "key_key.forward:key.keyboard.up",
            );
        assert_eq!(f.render(), want);
        // 値に含まれるコロンは最初の 1 つだけで分ける
        assert_eq!(f.get("mod_unknown_key"), Some("a:b:c"));
        assert_eq!(f.get("lastServer"), Some("play.example.com:25565"));
    }

    #[test]
    fn duplicate_keys_all_updated_and_get_returns_last() {
        let mut f = OptionsFile::parse(MODERN);
        assert_eq!(f.get("fov"), Some("0.7"));
        f.set_all(&[("fov", "0.9".into())]);
        assert_eq!(f.render().matches("fov:0.9").count(), 2);
    }

    #[test]
    fn set_all_ignores_keys_not_in_the_file() {
        let mut f = OptionsFile::parse(MODERN);
        f.set_all(&[("maxFps", "60".into()), ("garbage", "1".into())]);
        assert_eq!(f.render(), MODERN);
    }

    #[test]
    fn set_all_rejects_values_that_would_break_lines() {
        let mut f = OptionsFile::parse(MODERN);
        f.set_all(&[("fov", "1\nlastServer:evil".into()), ("fov", "1\r".into())]);
        assert_eq!(f.render(), MODERN);
    }

    #[test]
    fn crlf_is_preserved() {
        let mut f = OptionsFile::parse(CRLF);
        f.set_all(&[("maxFps", "30".into()), ("fov", "1.0".into())]);
        let out = f.render();
        assert_eq!(
            out,
            CRLF.replace("maxFps:120", "maxFps:30")
                .replace("fov:0.5", "fov:1.0")
        );
        assert_eq!(out.matches('\n').count(), out.matches("\r\n").count());
    }

    #[test]
    fn missing_trailing_newline_is_preserved() {
        let mut f = OptionsFile::parse(NO_EOL);
        f.set_all(&[("enableVsync", "false".into()), ("maxFps", "60".into())]);
        let out = f.render();
        assert!(out.ends_with("enableVsync:false"));
        assert_eq!(
            out,
            NO_EOL
                .replace("enableVsync:true", "enableVsync:false")
                .replace("maxFps:120", "maxFps:60")
        );
    }

    #[test]
    fn version_line_is_read() {
        assert_eq!(OptionsFile::parse(MODERN).version(), Some(3465));
        assert_eq!(OptionsFile::parse(CRLF).version(), Some(3465));
        assert_eq!(OptionsFile::parse(LEGACY).version(), None);
        assert_eq!(OptionsFile::parse("version:abc\n").version(), None);
    }

    #[test]
    fn backup_stamp_accepts_only_ascii_digits() {
        assert_eq!(backup_stamp("options-123.txt"), Some(123));
        for bad in [
            "options-+5.txt",
            "options-.txt",
            "options--5.txt",
            "options-1a.txt",
            "options-１.txt",
            "x-1.txt",
        ] {
            assert_eq!(backup_stamp(bad), None, "{bad}");
        }
    }

    #[test]
    fn prune_keeps_newest_ten_and_just_created_and_ignores_odd_names() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for i in 1..=12u32 {
            fs::write(dir.join(format!("options-{i}.txt")), "").unwrap();
        }
        fs::write(dir.join("options-+5.txt"), "").unwrap();
        // 今作ったものが最小の番号（時計が戻った場合）でも消さない
        let keep = dir.join("options-0.txt");
        fs::write(&keep, "").unwrap();
        prune_backups(dir, &keep);
        assert!(keep.exists());
        assert!(dir.join("options-+5.txt").exists());
        let numbered = (1..=12u32)
            .filter(|i| dir.join(format!("options-{i}.txt")).exists())
            .count();
        assert_eq!(numbered, 9);
        assert!(dir.join("options-12.txt").exists() && !dir.join("options-3.txt").exists());
    }

    #[test]
    fn prune_on_missing_dir_does_not_panic() {
        let tmp = tempfile::tempdir().unwrap();
        prune_backups(
            &tmp.path().join("nope"),
            &tmp.path().join("nope/options-1.txt"),
        );
    }

    fn backups(dir: &Path, id: &str) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir.join("sync").join(id))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn write_atomic_keeps_ten_backups() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("options.txt");
        let backups_dir = tmp.path().join("backups");
        fs::write(&path, "gen:0\n").unwrap();
        for i in 1..=12 {
            write_atomic(&path, &backups_dir, "inst", &format!("gen:{i}\n")).unwrap();
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), "gen:12\n");
        let names = backups(&backups_dir, "inst");
        assert_eq!(names.len(), 10);
        // 残るのは最新 10 件（直前の内容 gen:2 から gen:11）
        let mut gens: Vec<i32> = names
            .iter()
            .map(|n| {
                let c = fs::read_to_string(backups_dir.join("sync/inst").join(n)).unwrap();
                c[4..c.len() - 1].parse().unwrap()
            })
            .collect();
        gens.sort();
        assert_eq!(gens, (2..=11).collect::<Vec<_>>());
        assert!(!tmp.path().join("options.txt.tmp").exists());
    }

    #[test]
    fn write_atomic_leaves_original_when_tmp_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("options.txt");
        let backups_dir = tmp.path().join("backups");
        fs::write(&path, "old:1\n").unwrap();
        fs::write(tmp.path().join("options.txt.tmp"), "stale half-written").unwrap();
        write_atomic(&path, &backups_dir, "inst", "new:2\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new:2\n");
        assert!(!tmp.path().join("options.txt.tmp").exists());
        let names = backups(&backups_dir, "inst");
        assert_eq!(names.len(), 1);
        assert_eq!(
            fs::read_to_string(backups_dir.join("sync/inst").join(&names[0])).unwrap(),
            "old:1\n"
        );
    }

    #[test]
    fn write_atomic_creates_missing_file_without_backup() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("options.txt");
        write_atomic(&path, &tmp.path().join("backups"), "inst", "a:1\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "a:1\n");
        assert!(!tmp.path().join("backups").exists());
    }

    #[test]
    fn write_atomic_reports_io_error_with_path() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("no_such_dir").join("options.txt");
        let err = write_atomic(&path, &tmp.path().join("b"), "i", "a:1\n").unwrap_err();
        assert!(matches!(err, CoreError::Io { .. }));
    }
}
