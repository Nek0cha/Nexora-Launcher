//! HTTP の取得と、並列ダウンロード。

pub mod download;
pub mod hash;
mod lock;

use std::path::Path;
use std::time::Duration;

pub use download::{
    Depth, DownloadItem, FetchPhase, fetch_all, fetch_all_with, needs_download, part_path,
};
pub(crate) use lock::lock_path;

use crate::error::CoreError;

/// 一時的な失敗に対する再試行の最大回数（最初の試行を含まない）。
pub const MAX_RETRIES: u32 = 3;

/// Modrinth の API は、連絡先の分かる User-Agent を求めている。
pub const USER_AGENT: &str = concat!(
    "Nexora/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/Nek0cha/Nexora-Launcher)"
);

/// 429 のときに `X-Ratelimit-Reset` がなければ待つ秒数。
const RATE_LIMIT_DEFAULT_SECS: u64 = 5;
/// 429 のときに待つ秒数の上限。
const RATE_LIMIT_MAX_SECS: u64 = 60;

#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
    retry_base: Duration,
}

/// 取得してよい URL の方針。ホスト名の完全一致で許可する。
#[derive(Debug, Clone)]
pub struct HostPolicy {
    hosts: Vec<String>,
    strict: bool,
}

impl HostPolicy {
    /// HTTPS、許可したホスト名、既定のポート、認証情報なし、のすべてを求める。
    pub fn new(hosts: &[&str]) -> Self {
        Self {
            hosts: hosts.iter().map(|h| h.to_string()).collect(),
            strict: true,
        }
    }

    /// テストで 127.0.0.1 のモックを使うための方針。HTTP と任意のポートを許す。製品では使わない。
    #[doc(hidden)]
    pub fn lenient_for_tests(hosts: &[&str]) -> Self {
        Self {
            strict: false,
            ..Self::new(hosts)
        }
    }

    fn allows(&self, url: &reqwest::Url) -> bool {
        let Some(host) = url.host_str() else {
            return false;
        };
        let host_ok = self.hosts.iter().any(|h| h.eq_ignore_ascii_case(host));
        if !self.strict {
            return host_ok && matches!(url.scheme(), "http" | "https");
        }
        // IP アドレスのホストは、名前の一覧に載らないので、ここで自然に外れる
        host_ok
            && url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
    }

    pub fn check(&self, url: &str) -> Result<(), CoreError> {
        match reqwest::Url::parse(url) {
            Ok(parsed) if self.allows(&parsed) => Ok(()),
            _ => Err(CoreError::InvalidMetadata {
                message: format!("download host is not allowed: {url}"),
            }),
        }
    }
}

/// 条件付きの GET の結果。
#[derive(Debug)]
pub enum Conditional {
    NotModified,
    Fresh { body: Vec<u8>, etag: Option<String> },
}

impl Http {
    pub fn new() -> Result<Self, CoreError> {
        Self::from_builder(Self::client_builder())
    }

    /// 許可したホストにしかリダイレクトしない Http。リダイレクトは `max_redirects` 回まで。
    /// 許可外の飛び先や回数の超過では追わずに止まり、その 3xx の応答が `HttpStatus` の失敗になる。
    pub fn restricted(policy: &HostPolicy, max_redirects: usize) -> Result<Self, CoreError> {
        let policy = policy.clone();
        let redirect = reqwest::redirect::Policy::custom(move |attempt| {
            // `previous` は、この飛び先に着くまでに通った URL（最初の URL を含む）
            if attempt.previous().len() > max_redirects || !policy.allows(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        });
        Self::from_builder(Self::client_builder().redirect(redirect))
    }

    fn client_builder() -> reqwest::ClientBuilder {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
    }

    fn from_builder(builder: reqwest::ClientBuilder) -> Result<Self, CoreError> {
        let client = builder.build().map_err(|e| CoreError::Unexpected {
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

    /// JSON を POST して本文を返す。一時的な失敗は `get_conditional` と同じく再試行する。
    /// 429 のときだけは `X-Ratelimit-Reset` の秒数待って 1 回再試行する。
    pub async fn post_json(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> Result<Vec<u8>, CoreError> {
        let payload = serde_json::to_vec(body).map_err(|e| CoreError::Unexpected {
            message: e.to_string(),
        })?;
        self.send_rate_limited(url, || {
            self.client
                .post(url)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(payload.clone())
        })
        .await
    }

    /// GET して本文を返す。429 の扱いは `post_json` と同じ。
    pub(crate) async fn get_rate_limited(&self, url: &str) -> Result<Vec<u8>, CoreError> {
        self.send_rate_limited(url, || self.client.get(url)).await
    }

    async fn send_rate_limited(
        &self,
        url: &str,
        build: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<Vec<u8>, CoreError> {
        let mut attempt = 0;
        let mut waited_for_limit = false;
        loop {
            let (err, reset) = match self.try_send(build(), url).await {
                Ok(body) => return Ok(body),
                Err(failure) => failure,
            };
            let limited = matches!(err, CoreError::HttpStatus { status: 429, .. });
            if limited && waited_for_limit {
                // 待った後の 429 は、通常の再試行に回さずそのまま返す
                return Err(err);
            } else if limited {
                waited_for_limit = true;
                let secs = reset
                    .unwrap_or(RATE_LIMIT_DEFAULT_SECS)
                    .min(RATE_LIMIT_MAX_SECS);
                tokio::time::sleep(Duration::from_secs(secs)).await;
            } else if is_transient(&err) && attempt < MAX_RETRIES {
                tokio::time::sleep(self.backoff(attempt)).await;
                attempt += 1;
            } else {
                return Err(err);
            }
        }
    }

    /// 1 回送る。失敗には、`X-Ratelimit-Reset`（秒）があれば添える。
    async fn try_send(
        &self,
        request: reqwest::RequestBuilder,
        url: &str,
    ) -> Result<Vec<u8>, (CoreError, Option<u64>)> {
        let response = request.send().await.map_err(|e| (offline(e), None))?;
        let status = response.status();
        if !status.is_success() {
            let reset = response
                .headers()
                .get("x-ratelimit-reset")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            let err = CoreError::HttpStatus {
                status: status.as_u16(),
                url: url.to_string(),
            };
            return Err((err, reset));
        }
        let body = response.bytes().await.map_err(|e| (offline(e), None))?;
        Ok(body.to_vec())
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

#[cfg(test)]
mod tests {
    use super::HostPolicy;

    #[test]
    fn host_policy_allows_only_exact_https_hosts() {
        let policy = HostPolicy::new(&["cdn.modrinth.com", "github.com"]);
        for url in [
            "https://cdn.modrinth.com/data/a/b.jar",
            "https://CDN.MODRINTH.COM/a",
            "https://github.com/a/b/releases/download/v1/a.jar",
            "https://cdn.modrinth.com:443/a",
        ] {
            assert!(policy.check(url).is_ok(), "{url}");
        }
        for url in [
            "http://cdn.modrinth.com/a",
            "https://cdn.modrinth.com.evil.invalid/a",
            "https://evil.invalid/cdn.modrinth.com",
            "https://user:pw@cdn.modrinth.com/a",
            "https://cdn.modrinth.com:8443/a",
            "https://127.0.0.1/a",
            "https://[::1]/a",
            "https://10.0.0.1/a",
            "ftp://cdn.modrinth.com/a",
            "not a url",
        ] {
            let err = policy.check(url).unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "{url}");
        }
    }
}
