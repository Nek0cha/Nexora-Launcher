//! HTTP の取得と、並列ダウンロード。

pub mod download;
pub mod hash;
mod lock;

use std::path::Path;
use std::time::Duration;

pub use download::{Depth, DownloadItem, fetch_all, needs_download, part_path};
pub(crate) use lock::lock_path;

use crate::error::CoreError;

/// 一時的な失敗に対する再試行の最大回数（最初の試行を含まない）。
pub const MAX_RETRIES: u32 = 3;

#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
    retry_base: Duration,
}

/// 条件付きの GET の結果。
#[derive(Debug)]
pub enum Conditional {
    NotModified,
    Fresh { body: Vec<u8>, etag: Option<String> },
}

impl Http {
    pub fn new() -> Result<Self, CoreError> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Nexora/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| CoreError::Unexpected {
                message: e.to_string(),
            })?;
        Ok(Self {
            client,
            retry_base: Duration::from_millis(500),
        })
    }

    /// 再試行の待ち時間の基準。テストで短くするために使う。
    pub fn with_retry_base(mut self, base: Duration) -> Self {
        self.retry_base = base;
        self
    }

    pub(crate) fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// attempt 回目（0 から）の失敗の後に待つ時間。
    pub(crate) fn backoff(&self, attempt: u32) -> Duration {
        self.retry_base * 2u32.pow(attempt)
    }

    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>, CoreError> {
        match self.get_conditional(url, None).await? {
            Conditional::Fresh { body, .. } => Ok(body),
            Conditional::NotModified => Err(CoreError::Unexpected {
                message: format!("unexpected 304 from {url}"),
            }),
        }
    }

    /// etag を If-None-Match に付けて取得する。一時的な失敗は再試行する。
    pub async fn get_conditional(
        &self,
        url: &str,
        etag: Option<&str>,
    ) -> Result<Conditional, CoreError> {
        let mut attempt = 0;
        loop {
            match self.try_get(url, etag).await {
                Err(err) if is_transient(&err) && attempt < MAX_RETRIES => {
                    tokio::time::sleep(self.backoff(attempt)).await;
                    attempt += 1;
                }
                other => return other,
            }
        }
    }

    async fn try_get(&self, url: &str, etag: Option<&str>) -> Result<Conditional, CoreError> {
        let mut request = self.client.get(url);
        if let Some(tag) = etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, tag);
        }
        let response = request.send().await.map_err(offline)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(Conditional::NotModified);
        }
        if !status.is_success() {
            return Err(CoreError::HttpStatus {
                status: status.as_u16(),
                url: url.to_string(),
            });
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = response.bytes().await.map_err(offline)?.to_vec();
        Ok(Conditional::Fresh { body, etag })
    }
}

/// 通信そのものの失敗は、すべてネットワークの不通として扱う。
pub(crate) fn offline(err: reqwest::Error) -> CoreError {
    CoreError::Offline {
        message: err.to_string(),
    }
}

/// 再試行で回復しうる失敗か。
pub(crate) fn is_transient(err: &CoreError) -> bool {
    match err {
        CoreError::Offline { .. } | CoreError::HashMismatch { .. } => true,
        CoreError::HttpStatus { status, .. } => *status == 429 || *status >= 500,
        _ => false,
    }
}

pub fn io_err(path: &Path, source: std::io::Error) -> CoreError {
    CoreError::Io {
        path: path.display().to_string(),
        source,
    }
}
