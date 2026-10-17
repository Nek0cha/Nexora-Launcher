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

/// 一時的な HTTP ステータス（5xx、429）。本文は入れない。
pub(crate) fn transient_status(status: u16) -> CoreError {
    CoreError::Offline {
        message: format!("HTTP {status}"),
    }
}

/// 想定外の HTTP ステータス。本文は入れない。url に秘密を含めないこと。
pub(crate) fn http_status(status: u16, url: &str) -> CoreError {
    CoreError::HttpStatus {
        status,
        url: url.to_string(),
    }
}

/// 読み終えた応答。5xx と 429 は `send` の時点でエラーになっているので、ここには来ない。
pub(crate) struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// 本文を JSON として読む。失敗の中身（本文）はエラーに入れない。
    pub fn json<T: serde::de::DeserializeOwned>(&self, what: &str) -> Result<T, CoreError> {
        serde_json::from_slice(&self.body).map_err(|_| CoreError::Unexpected {
            message: format!("invalid {what} response"),
        })
    }

    /// 2xx なら本文を JSON として読み、そうでなければ HttpStatus にする。
    pub fn json_or_status<T: serde::de::DeserializeOwned>(
        &self,
        what: &str,
        url: &str,
    ) -> Result<T, CoreError> {
        if self.is_success() {
            self.json(what)
        } else {
            Err(http_status(self.status, url))
        }
    }
}

/// JSON を POST するリクエスト。reqwest に json 機能がないので、本文は自分で組む。
pub(crate) fn json_post(
    http: &AuthHttp,
    url: &str,
    body: &serde_json::Value,
) -> reqwest::RequestBuilder {
    http.client()
        .post(url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .body(body.to_string())
}

/// 送って本文まで読む。通信の失敗と 5xx、429 は一時的な失敗（Offline）にする。
pub(crate) async fn send(request: reqwest::RequestBuilder) -> Result<Reply, CoreError> {
    let response = request.send().await.map_err(|e| transient(&e))?;
    let status = response.status().as_u16();
    let body = response.bytes().await.map_err(|e| transient(&e))?;
    if status_is_transient(status) {
        return Err(transient_status(status));
    }
    Ok(Reply {
        status,
        body: body.to_vec(),
    })
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
