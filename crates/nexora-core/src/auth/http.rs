//! 認証用の HTTP クライアント。リダイレクトは追わない。

use std::time::Duration;

use crate::error::CoreError;

#[derive(Clone)]
pub struct AuthHttp {
    client: reqwest::Client,
}

impl AuthHttp {
    pub fn new() -> Result<Self, CoreError> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Nexora/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| CoreError::Unexpected {
                message: e.to_string(),
            })?;
        Ok(Self { client })
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }
}

/// 通信そのものの失敗は、一時的な失敗（オフライン）として扱う。
/// URL は message に含めない。
pub fn transient(err: &reqwest::Error) -> CoreError {
    let message = if err.is_timeout() {
        "request timed out".to_string()
    } else if err.is_connect() {
        "connection failed".to_string()
    } else {
        err.to_string()
    };
    CoreError::Offline { message }
}

/// 5xx と 429 は一時的な失敗。
pub fn status_is_transient(status: u16) -> bool {
    status == 429 || status >= 500
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_statuses() {
        assert!(status_is_transient(429));
        assert!(status_is_transient(500));
        assert!(status_is_transient(503));
        assert!(!status_is_transient(400));
        assert!(!status_is_transient(401));
        assert!(!status_is_transient(302));
    }
}
