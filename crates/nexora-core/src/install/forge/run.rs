//! 計画した processor を子プロセスとして実行し、出力を検証する。

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;

use crate::error::CoreError;
use crate::install::forge::data::{ExpectedOutput, PlannedProcessor};
use crate::install::forge::installer::read_entry;
use crate::net::hash::sha1_file;

const MANIFEST: &str = "META-INF/MANIFEST.MF";

/// Windows でコンソールの窓を出さないためのフラグ（`launch::process` と同じ）。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// jar の `META-INF/MANIFEST.MF` の `Main-Class`。継続行（先頭が空白の行）をつなぐ。無ければ `InvalidMetadata`。
pub fn manifest_main_class(jar: &Path) -> Result<String, CoreError> {
    let bytes = read_entry(jar, MANIFEST)?;
    let text = String::from_utf8_lossy(&bytes);
    // メインの節（最初の空行まで）だけを見る。各エントリの節の Main-Class は使わない。
    let mut logical: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            break;
        }
        if let Some(rest) = line.strip_prefix(' ') {
            if let Some(last) = logical.last_mut() {
                last.push_str(rest);
            }
            continue;
        }
        logical.push(line.to_string());
    }
    logical
        .iter()
        .filter_map(|l| l.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("Main-Class"))
        .map(|(_, value)| value.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| CoreError::InvalidMetadata {
            message: format!("{}: {MANIFEST} has no Main-Class", jar.display()),
        })
}

/// `(java, [-cp, <jar と classpath を OS の区切りでつなぐ>, <Main-Class>, 引数...])`。
pub fn processor_command(
    java: &Path,
    p: &PlannedProcessor,
    main_class: &str,
) -> (PathBuf, Vec<String>) {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let classpath = std::iter::once(&p.jar)
        .chain(p.classpath.iter())
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(sep);
    let mut args = vec!["-cp".to_string(), classpath, main_class.to_string()];
    args.extend(p.args.iter().cloned());
    (java.to_path_buf(), args)
}

/// 出力が 1 つ以上宣言されていて、そのすべてが存在し、sha1 が宣言されているものは一致する。
/// 宣言された出力が無い processor は常に `false`。
pub fn outputs_satisfied(p: &PlannedProcessor) -> bool {
    !p.outputs.is_empty()
        && p.outputs
            .iter()
            .all(|o| output_matches(o) == OutputState::Ok)
}

#[derive(Debug, PartialEq, Eq)]
enum OutputState {
    Ok,
    Missing,
    Mismatch,
}

/// 1 つの出力の状態。sha1 は大文字と小文字を区別しない。
fn output_matches(o: &ExpectedOutput) -> OutputState {
    if !o.path.is_file() {
        return OutputState::Missing;
    }
    let Some(expected) = &o.sha1 else {
        return OutputState::Ok;
    };
    match sha1_file(&o.path) {
        Ok(actual) if actual.eq_ignore_ascii_case(expected.trim()) => OutputState::Ok,
        _ => OutputState::Mismatch,
    }
}

/// 宣言の順に実行する。省略できるものは飛ばす。実行後に出力を検証し、合わなければその出力を消して `HashMismatch`。
/// `progress(完了した数, 全体の数)` を各 processor の前後に呼ぶ。
pub async fn run_processors(
    java: &Path,
    list: &[PlannedProcessor],
    token: &CancellationToken,
    progress: impl Fn(usize, usize),
) -> Result<(), CoreError> {
    let total = list.len();
    for (index, p) in list.iter().enumerate() {
        if token.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        progress(index, total);
        let check = p.clone();
        let satisfied = tokio::task::spawn_blocking(move || outputs_satisfied(&check))
            .await
            .unwrap_or(false);
        if satisfied {
            tracing::debug!(processor = %p.name, "processor outputs are already in place, skipping");
        } else {
            run_one(java, p, token).await?;
            verify_outputs(p).await?;
        }
        progress(index + 1, total);
    }
    Ok(())
}

/// ログの末尾として残す文字数。
const LOG_TAIL_CHARS: usize = 2000;

/// 標準出力と標準エラーを届いた順に集め、末尾だけを保持する。
#[derive(Default)]
struct LogTail {
    buf: String,
}

impl LogTail {
    fn push(&mut self, line: &str) {
        self.buf.push_str(line);
        // 毎回は切り詰めず、十分に大きくなったときだけ末尾に縮める。
        if self.buf.len() > LOG_TAIL_CHARS * 16 {
            self.buf = last_chars(&self.buf, LOG_TAIL_CHARS).to_string();
        }
    }
}

/// 末尾の `n` 文字（文字の境界で切る）。
fn last_chars(s: &str, n: usize) -> &str {
    match s.char_indices().rev().nth(n.saturating_sub(1)) {
        Some((i, _)) if n > 0 => &s[i..],
        _ if n == 0 => "",
        _ => s,
    }
}

/// パイプを行ごとに読み切る。行は UTF-8 として読めない部分を置き換える。
async fn drain<R>(reader: R, tail: Arc<Mutex<LogTail>>, processor: String, stream: &'static str)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let text = String::from_utf8_lossy(&line);
                // パスにユーザー名が入りうるため、info 以上には出さない。
                tracing::debug!(processor = %processor, stream, "{}", text.trim_end());
                if let Ok(mut tail) = tail.lock() {
                    tail.push(&text);
                }
            }
        }
    }
}

fn tail_text(tail: &Mutex<LogTail>, note: Option<&str>) -> String {
    let mut text = tail.lock().map(|t| t.buf.clone()).unwrap_or_default();
    if let Some(note) = note {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(note);
    }
    last_chars(&text, LOG_TAIL_CHARS).to_string()
}

/// 1 つの processor を子プロセスとして実行し、終了を待つ。
async fn run_one(
    java: &Path,
    p: &PlannedProcessor,
    token: &CancellationToken,
) -> Result<(), CoreError> {
    let main_class = manifest_main_class(&p.jar)?;
    let (program, args) = processor_command(java, p, &main_class);
    tracing::debug!(processor = %p.name, ?program, ?args, "running processor");

    // シェルを挟まず、引数は 1 つずつ渡す。
    let mut cmd = tokio::process::Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd.spawn().map_err(|e| CoreError::ProcessorFailed {
        processor: p.name.clone(),
        code: None,
        log_tail: format!("could not start Java ({}): {e}", program.display()),
    })?;

    let result = supervise(&mut child, p, token).await;
    if result.is_err() {
        // 起動した後に失敗したとき（キャンセル、0 以外の終了、待てなかった、出力が足りない）は、
        // 途中まで書かれた出力を残さない。実行したのは出力が揃っていなかったからなので、消しても失うものはない。
        remove_declared_outputs(p);
    }
    result
}

/// 宣言された出力だけを消す（できる範囲で）。それ以外のファイルには触らない。
fn remove_declared_outputs(p: &PlannedProcessor) {
    for o in &p.outputs {
        match std::fs::remove_file(&o.path) {
            Ok(()) => {
                tracing::debug!(processor = %p.name, path = %o.path.display(), "removed a partial processor output");
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::debug!(processor = %p.name, path = %o.path.display(), error = %e, "could not remove a processor output");
            }
        }
    }
}

/// 起動した子プロセスの出力を読み、終了（またはキャンセル）を待つ。
async fn supervise(
    child: &mut tokio::process::Child,
    p: &PlannedProcessor,
    token: &CancellationToken,
) -> Result<(), CoreError> {
    let tail = Arc::new(Mutex::new(LogTail::default()));
    // 片方だけを読むと、もう片方のパイプが満ちて子が止まるため、両方を同時に読む。
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        readers.push(tokio::spawn(drain(
            out,
            tail.clone(),
            p.name.clone(),
            "stdout",
        )));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(tokio::spawn(drain(
            err,
            tail.clone(),
            p.name.clone(),
            "stderr",
        )));
    }

    let status = tokio::select! {
        status = child.wait() => status,
        _ = token.cancelled() => {
            // 直接の子（java）を止めて待つ。孫のプロセスまでは追わない。
            let _ = child.kill().await;
            for r in &readers {
                r.abort();
            }
            return Err(CoreError::Cancelled);
        }
    };
    // 終了後はパイプが閉じるまで読み切る。孫のプロセスがパイプを持ち続けても、キャンセルでは抜けられる。
    for i in 0..readers.len() {
        tokio::select! {
            _ = &mut readers[i] => {}
            _ = token.cancelled() => {
                for r in &readers[i..] {
                    r.abort();
                }
                return Err(CoreError::Cancelled);
            }
        }
    }

    let status = status.map_err(|e| CoreError::ProcessorFailed {
        processor: p.name.clone(),
        code: None,
        log_tail: tail_text(&tail, Some(&format!("failed to wait for the process: {e}"))),
    })?;
    if !status.success() {
        return Err(CoreError::ProcessorFailed {
            processor: p.name.clone(),
            code: status.code(),
            log_tail: tail_text(&tail, None),
        });
    }
    // 終了コードが 0 でも、宣言した出力が無ければ失敗とする（ハッシュの不一致とは区別する）。
    let missing: Vec<&ExpectedOutput> = p.outputs.iter().filter(|o| !o.path.is_file()).collect();
    if let Some(first) = missing.first() {
        return Err(CoreError::ProcessorFailed {
            processor: p.name.clone(),
            code: status.code(),
            log_tail: tail_text(
                &tail,
                Some(&format!(
                    "expected output was not created: {}",
                    first.path.display()
                )),
            ),
        });
    }
    Ok(())
}

/// 実行後の出力の sha1 を確かめる。合わない出力は消して `HashMismatch`。宣言された出力の外は触らない。
async fn verify_outputs(p: &PlannedProcessor) -> Result<(), CoreError> {
    let outputs = p.outputs.clone();
    let mismatched = tokio::task::spawn_blocking(move || {
        outputs
            .into_iter()
            .filter(|o| output_matches(o) != OutputState::Ok)
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| CoreError::Unexpected {
        message: e.to_string(),
    })?;
    let Some(first) = mismatched.first() else {
        return Ok(());
    };
    for o in &mismatched {
        tracing::debug!(processor = %p.name, path = %o.path.display(), "processor output did not match its sha1");
        let _ = std::fs::remove_file(&o.path);
    }
    Err(CoreError::HashMismatch {
        path: first.path.display().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::hash::sha1_hex;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make_jar(path: &Path, manifest: &str) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    fn processor(outputs: Vec<ExpectedOutput>) -> PlannedProcessor {
        PlannedProcessor {
            name: "net.minecraftforge:binarypatcher:1.0".into(),
            jar: PathBuf::from("a.jar"),
            classpath: vec![],
            args: vec![],
            outputs,
        }
    }

    #[test]
    fn manifest_main_class_reads_main_class() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("p.jar");
        make_jar(
            &jar,
            "Manifest-Version: 1.0\r\nmain-class:  net.minecraftforge.Main  \r\n\r\nName: x\r\nMain-Class: wrong\r\n",
        );
        assert_eq!(
            manifest_main_class(&jar).unwrap(),
            "net.minecraftforge.Main"
        );
    }

    #[test]
    fn manifest_main_class_joins_continuation_lines() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("p.jar");
        make_jar(
            &jar,
            "Manifest-Version: 1.0\nMain-Class: net.minecraftforge.installert\n ools.ConsoleTool\nClass-Path: a.jar\n",
        );
        assert_eq!(
            manifest_main_class(&jar).unwrap(),
            "net.minecraftforge.installertools.ConsoleTool"
        );
    }

    #[test]
    fn manifest_without_main_class_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("p.jar");
        make_jar(
            &jar,
            "Manifest-Version: 1.0\r\n\r\nName: a\r\nMain-Class: x\r\n",
        );
        let err = manifest_main_class(&jar).unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
        // マニフェストそのものが無い jar も同じ。
        let bare = dir.path().join("bare.jar");
        let file = std::fs::File::create(&bare).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("a.txt", SimpleFileOptions::default())
            .unwrap();
        zip.finish().unwrap();
        assert_eq!(
            manifest_main_class(&bare).unwrap_err().code(),
            "meta.invalid"
        );
    }

    #[test]
    fn command_uses_os_separator_and_keeps_args() {
        let p = PlannedProcessor {
            name: "n".into(),
            jar: PathBuf::from("p.jar"),
            classpath: vec![PathBuf::from("a.jar"), PathBuf::from("b.jar")],
            args: vec!["--input".into(), "C:/my folder/client.jar".into()],
            outputs: vec![],
        };
        let (program, args) = processor_command(Path::new("java"), &p, "x.Main");
        assert_eq!(program, PathBuf::from("java"));
        let sep = if cfg!(windows) { ";" } else { ":" };
        assert_eq!(
            args,
            vec![
                "-cp".to_string(),
                format!("p.jar{sep}a.jar{sep}b.jar"),
                "x.Main".into(),
                "--input".into(),
                "C:/my folder/client.jar".into(),
            ]
        );
    }

    #[test]
    fn log_tail_keeps_the_last_chars_on_char_boundaries() {
        assert_eq!(last_chars("あいうえお", 2), "えお");
        assert_eq!(last_chars("ab", 5), "ab");
        assert_eq!(last_chars("ab", 0), "");
        let mut tail = LogTail::default();
        for _ in 0..LOG_TAIL_CHARS * 20 {
            tail.push("é");
        }
        tail.push("end");
        let text = tail_text(&Mutex::new(tail), None);
        assert_eq!(text.chars().count(), LOG_TAIL_CHARS);
        assert!(text.ends_with("end"));
    }

    #[test]
    fn outputs_satisfied_requires_declared_outputs() {
        assert!(!outputs_satisfied(&processor(vec![])));
    }

    #[test]
    fn outputs_satisfied_checks_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.jar");
        let missing = processor(vec![ExpectedOutput {
            path: out.clone(),
            sha1: None,
        }]);
        assert!(!outputs_satisfied(&missing));
        std::fs::write(&out, b"hello").unwrap();
        // sha1 が None なら存在だけを見る。
        assert!(outputs_satisfied(&missing));
        let good = processor(vec![ExpectedOutput {
            path: out.clone(),
            sha1: Some(sha1_hex(b"hello").to_uppercase()),
        }]);
        assert!(outputs_satisfied(&good));
        let bad = processor(vec![ExpectedOutput {
            path: out,
            sha1: Some(sha1_hex(b"other")),
        }]);
        assert!(!outputs_satisfied(&bad));
    }
}
