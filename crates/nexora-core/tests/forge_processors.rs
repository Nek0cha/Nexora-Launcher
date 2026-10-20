//! Forge の processor の実行。実際の Java の代わりに、引数を記録するだけの「偽の Java」を使う。
//!
//! 偽の Java は `-cp <cp> <Main-Class>` の後ろの引数を `記録先 目印 動作 出力` として読む。
//! 動作は `ok`（出力に書く）、`fail`（標準エラーに書いて終了コード 3）、`failwrite`（出力に書いてから終了コード 4）、`spin`（出力に追記し続ける）、
//! `none`（何もしない）。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nexora_core::install::forge::data::{ExpectedOutput, PlannedProcessor};
use nexora_core::install::forge::run::run_processors;
use nexora_core::net::hash::sha1_hex;
use tokio_util::sync::CancellationToken;
use zip::write::SimpleFileOptions;

#[cfg(windows)]
const FAKE_NAME: &str = "java.cmd";
#[cfg(windows)]
const FAKE_SCRIPT: &str = "@echo off\r\n\
>>\"%~4\" echo %~5\r\n\
if \"%~6\"==\"ok\" (\r\n\
  >\"%~7\" echo good\r\n\
  exit /b 0\r\n\
)\r\n\
if \"%~6\"==\"fail\" (\r\n\
  echo some stdout\r\n\
  echo boom-stderr-end 1>&2\r\n\
  exit /b 3\r\n\
)\r\n\
if \"%~6\"==\"failwrite\" (\r\n\
  >\"%~7\" echo partial\r\n\
  exit /b 4\r\n\
)\r\n\
if \"%~6\"==\"spin\" goto spin\r\n\
exit /b 0\r\n\
:spin\r\n\
>>\"%~7\" echo beat\r\n\
goto spin\r\n";

#[cfg(not(windows))]
const FAKE_NAME: &str = "java";
#[cfg(not(windows))]
const FAKE_SCRIPT: &str = "#!/bin/sh\n\
shift 3\n\
echo \"$2\" >> \"$1\"\n\
case \"$3\" in\n\
  ok) echo good > \"$4\" ;;\n\
  fail) echo some stdout; echo boom-stderr-end >&2; exit 3 ;;\n\
  failwrite) echo partial > \"$4\"; exit 4 ;;\n\
  spin) while :; do echo beat >> \"$4\"; sleep 0.05; done ;;\n\
esac\n\
exit 0\n";

struct Fixture {
    _temp: tempfile::TempDir,
    /// 空白と ASCII 以外の文字を含む作業場所。
    dir: PathBuf,
    java: PathBuf,
    jar: PathBuf,
    record: PathBuf,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("nexora テスト dir");
    std::fs::create_dir_all(&dir).unwrap();
    let java = dir.join(FAKE_NAME);
    std::fs::write(&java, FAKE_SCRIPT).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&java, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let jar = dir.join("processor.jar");
    let file = std::fs::File::create(&jar).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"Manifest-Version: 1.0\r\nMain-Class: fake.Main\r\n\r\n")
        .unwrap();
    zip.finish().unwrap();
    let record = dir.join("record.txt");
    Fixture {
        _temp: temp,
        dir,
        java,
        jar,
        record,
    }
}

impl Fixture {
    fn processor(
        &self,
        marker: &str,
        action: &str,
        out: &Path,
        outputs: Vec<ExpectedOutput>,
    ) -> PlannedProcessor {
        PlannedProcessor {
            name: format!("test:{marker}:1.0"),
            jar: self.jar.clone(),
            classpath: vec![self.dir.join("lib a.jar")],
            args: vec![
                self.record.to_string_lossy().into_owned(),
                marker.into(),
                action.into(),
                out.to_string_lossy().into_owned(),
            ],
            outputs,
        }
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(&self.record)
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    }
}

fn good_sha1() -> String {
    // 偽の Java が書く中身（echo good）の sha1。
    sha1_hex(if cfg!(windows) {
        b"good\r\n"
    } else {
        b"good\n"
    })
}

#[tokio::test]
async fn runs_processors_in_order() {
    let f = fixture();
    let out = f.dir.join("out.txt");
    let list = vec![
        f.processor("first", "none", &out, vec![]),
        f.processor("second", "none", &out, vec![]),
        f.processor("third", "none", &out, vec![]),
    ];
    run_processors(&f.java, &list, &CancellationToken::new(), |_, _| {})
        .await
        .unwrap();
    assert_eq!(f.calls(), vec!["first", "second", "third"]);
}

#[tokio::test]
async fn skips_when_outputs_are_satisfied() {
    let f = fixture();
    let out = f.dir.join("out file.jar");
    std::fs::write(&out, b"already").unwrap();
    let list = vec![
        f.processor(
            "skipped",
            "ok",
            &out,
            vec![ExpectedOutput {
                path: out.clone(),
                sha1: Some(sha1_hex(b"already")),
            }],
        ),
        f.processor("ran", "none", &f.dir.join("x"), vec![]),
    ];
    run_processors(&f.java, &list, &CancellationToken::new(), |_, _| {})
        .await
        .unwrap();
    assert_eq!(f.calls(), vec!["ran"]);
    assert_eq!(std::fs::read(&out).unwrap(), b"already");
}

#[tokio::test]
async fn reruns_when_no_outputs_are_declared() {
    let f = fixture();
    let out = f.dir.join("out.txt");
    let list = vec![f.processor("again", "ok", &out, vec![])];
    for _ in 0..2 {
        run_processors(&f.java, &list, &CancellationToken::new(), |_, _| {})
            .await
            .unwrap();
    }
    assert_eq!(f.calls(), vec!["again", "again"]);
}

#[tokio::test]
async fn runs_and_verifies_declared_outputs() {
    let f = fixture();
    let out = f.dir.join("patched.jar");
    let list = vec![f.processor(
        "patch",
        "ok",
        &out,
        vec![ExpectedOutput {
            path: out.clone(),
            sha1: Some(good_sha1().to_uppercase()),
        }],
    )];
    run_processors(&f.java, &list, &CancellationToken::new(), |_, _| {})
        .await
        .unwrap();
    assert!(out.is_file());
    assert_eq!(f.calls(), vec!["patch"]);
}

#[tokio::test]
async fn fails_with_log_tail_on_nonzero_exit() {
    let f = fixture();
    let out = f.dir.join("out.txt");
    let list = vec![
        f.processor("broken", "fail", &out, vec![]),
        f.processor("never", "none", &out, vec![]),
    ];
    let err = run_processors(&f.java, &list, &CancellationToken::new(), |_, _| {})
        .await
        .unwrap_err();
    assert_eq!(err.code(), "install.processor_failed");
    match err {
        nexora_core::error::CoreError::ProcessorFailed {
            processor,
            code,
            log_tail,
        } => {
            assert_eq!(processor, "test:broken:1.0");
            assert_eq!(code, Some(3));
            assert!(log_tail.contains("boom-stderr-end"), "{log_tail}");
            assert!(log_tail.contains("some stdout"), "{log_tail}");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(f.calls(), vec!["broken"]);
}

#[tokio::test]
async fn missing_output_fails_the_processor() {
    let f = fixture();
    let out = f.dir.join("never-written.jar");
    let list = vec![f.processor(
        "lazy",
        "none",
        &out,
        vec![ExpectedOutput {
            path: out.clone(),
            sha1: None,
        }],
    )];
    let err = run_processors(&f.java, &list, &CancellationToken::new(), |_, _| {})
        .await
        .unwrap_err();
    assert_eq!(err.code(), "install.processor_failed");
    let payload = err.to_payload();
    let tail = format!("{:?}", payload.params.get("logTail"));
    assert!(tail.contains("never-written.jar"), "{tail}");
}

#[tokio::test]
async fn output_hash_mismatch_removes_the_output() {
    let f = fixture();
    let out = f.dir.join("patched.jar");
    let other = f.dir.join("keep.txt");
    std::fs::write(&other, b"keep").unwrap();
    let list = vec![f.processor(
        "patch",
        "ok",
        &out,
        vec![ExpectedOutput {
            path: out.clone(),
            sha1: Some(sha1_hex(b"something else")),
        }],
    )];
    let err = run_processors(&f.java, &list, &CancellationToken::new(), |_, _| {})
        .await
        .unwrap_err();
    assert_eq!(err.code(), "net.hash_mismatch");
    assert!(!out.exists());
    // 宣言された出力の外は消さない。
    assert!(other.is_file());
    assert!(f.jar.is_file());
}

#[tokio::test]
async fn cancel_stops_the_child_and_returns_cancelled() {
    let f = fixture();
    let beat = f.dir.join("beat.txt");
    let list = vec![f.processor("spin", "spin", &beat, vec![])];
    let token = CancellationToken::new();
    let java = f.java.clone();
    let run_token = token.clone();
    let handle =
        tokio::spawn(async move { run_processors(&java, &list, &run_token, |_, _| {}).await });
    // 子プロセスが動き出すのを待つ。
    for _ in 0..200 {
        if beat.is_file() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(beat.is_file(), "the fake java never started");
    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(10), handle)
        .await
        .expect("run_processors did not return after cancel")
        .unwrap();
    assert_eq!(result.unwrap_err().code(), "task.cancelled");
    // 子プロセスが止まっていれば、追記はもう増えない。
    tokio::time::sleep(Duration::from_millis(200)).await;
    let before = std::fs::metadata(&beat).unwrap().len();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let after = std::fs::metadata(&beat).unwrap().len();
    assert_eq!(before, after, "the child kept running after cancel");
}

#[tokio::test]
async fn progress_reports_each_step() {
    let f = fixture();
    let out = f.dir.join("out.txt");
    std::fs::write(&out, b"done").unwrap();
    let list = vec![
        f.processor("one", "none", &out, vec![]),
        f.processor(
            "skipped",
            "none",
            &out,
            vec![ExpectedOutput {
                path: out.clone(),
                sha1: None,
            }],
        ),
    ];
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    run_processors(
        &f.java,
        &list,
        &CancellationToken::new(),
        move |done, total| {
            sink.lock().unwrap().push((done, total));
        },
    )
    .await
    .unwrap();
    assert_eq!(*seen.lock().unwrap(), vec![(0, 2), (1, 2), (1, 2), (2, 2)]);
}

#[tokio::test]
async fn cancel_removes_declared_outputs_so_the_next_run_starts_over() {
    let f = fixture();
    let beat = f.dir.join("beat.txt");
    let declared = vec![ExpectedOutput {
        path: beat.clone(),
        sha1: None,
    }];
    let list = vec![f.processor("spin", "spin", &beat, declared.clone())];
    let token = CancellationToken::new();
    let java = f.java.clone();
    let run_token = token.clone();
    let handle =
        tokio::spawn(async move { run_processors(&java, &list, &run_token, |_, _| {}).await });
    for _ in 0..200 {
        if beat.is_file() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(beat.is_file(), "the fake java never started");
    token.cancel();
    let result = tokio::time::timeout(Duration::from_secs(10), handle)
        .await
        .expect("run_processors did not return after cancel")
        .unwrap();
    assert_eq!(result.unwrap_err().code(), "task.cancelled");
    // 途中まで書かれた出力は残さない。
    assert!(!beat.exists(), "a partial output was left behind");
    assert!(f.jar.is_file());
    assert!(f.java.is_file());

    // 次の実行では省略されずにもう一度動く（ok で書いて終わる）。
    let again = vec![f.processor("spin", "ok", &beat, declared)];
    run_processors(&f.java, &again, &CancellationToken::new(), |_, _| {})
        .await
        .unwrap();
    assert_eq!(f.calls(), vec!["spin", "spin"]);
    assert!(beat.is_file());
}

#[tokio::test]
async fn nonzero_exit_removes_declared_outputs_only() {
    let f = fixture();
    let out = f.dir.join("partial.jar");
    let other = f.dir.join("keep.txt");
    std::fs::write(&other, b"keep").unwrap();
    let list = vec![f.processor(
        "partial",
        "failwrite",
        &out,
        vec![ExpectedOutput {
            path: out.clone(),
            sha1: None,
        }],
    )];
    let err = run_processors(&f.java, &list, &CancellationToken::new(), |_, _| {})
        .await
        .unwrap_err();
    assert_eq!(err.code(), "install.processor_failed");
    assert!(!out.exists(), "a partial output was left behind");
    assert!(other.is_file());
    assert!(f.jar.is_file());
    assert!(f.record.is_file());
}
