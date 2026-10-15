//! ログの初期化。ファイルは logs/nexora.YYYY-MM-DD.log に日ごとに書き、7 日分だけ残す。
//! パスが決まる前の失敗は eprintln で出す（subscriber は一度しか設定できないため、
//! パスが決まってから標準出力とファイルの両方をまとめて設定する）。

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

/// 戻り値の guard はアプリの終了まで保持する（Tauri の state に入れる）。
pub struct LogGuard(#[allow(dead_code)] WorkerGuard);

pub fn init(logs_dir: &Path) -> Result<LogGuard, String> {
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("nexora")
        .filename_suffix("log")
        .max_log_files(7)
        .build(logs_dir)
        .map_err(|e| e.to_string())?;
    let (file_writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_env("NEXORA_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let file_layer = fmt::layer().with_writer(file_writer).with_ansi(false);
    let stdout_layer = cfg!(debug_assertions).then(fmt::layer);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .try_init()
        .map_err(|e| e.to_string())?;
    Ok(LogGuard(guard))
}
