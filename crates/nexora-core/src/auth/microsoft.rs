//! Microsoft identity platform のトークンとデバイスコード。

use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::AuthUrls;
use super::http::{AuthHttp, status_is_transient, transient};
use crate::error::CoreError;

const SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// `slow_down` のたびに足す間隔。
const SLOW_DOWN_STEP: Duration = Duration::from_secs(5);
/// 応答に `interval` がないときの既定値。
const DEFAULT_INTERVAL_SECS: u64 = 5;

/// Microsoft のトークン。
#[derive(Debug)]
pub struct MsTokens {
    pub access: SecretString,
    /// 応答に新しい refresh token がなければ None（今のものを使い続ける）。
    pub refresh: Option<SecretString>,
    pub expires_at_ms: i64,
}

#[derive(Deserialize)]
struct TokenBody {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: Option<String>,
}

/// デバイスコードの要求の結果。
#[derive(Debug)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    device_code: SecretString,
    interval: Duration,
    expires_at_ms: i64,
}

#[derive(Deserialize)]
struct DeviceBody {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: i64,
    interval: Option<u64>,
}

/// `slow_down` の後の間隔。
fn slowed(interval: Duration) -> Duration {
    interval + SLOW_DOWN_STEP
}

/// トークンのエンドポイントの応答。
enum Reply {
    Ok(TokenBody),
    /// 400 系の `error` の値。
    Error(String),
}

/// フォームを POST して、応答を分類する。本文はエラーに入れない。
async fn post_form(
    http: &AuthHttp,
    url: &str,
    form: &[(&str, &str)],
) -> Result<(u16, Vec<u8>), CoreError> {
    let response = http
        .client()
        .post(url)
        .form(form)
        .send()
        .await
        .map_err(|e| transient(&e))?;
    let status = response.status().as_u16();
    let body = response.bytes().await.map_err(|e| transient(&e))?;
    Ok((status, body.to_vec()))
}

fn http_status(status: u16, url: &str) -> CoreError {
    CoreError::HttpStatus {
        status,
        url: url.to_string(),
    }
}

/// `error` の値を CoreError にする。None は未分類。
fn classify_error(error: &str) -> Option<CoreError> {
    match error {
        "invalid_grant" | "interaction_required" | "consent_required" => {
            Some(CoreError::LoginRequired {
                account_id: String::new(),
            })
        }
        "authorization_declined" | "access_denied" => Some(CoreError::AuthDenied),
        "expired_token" => Some(CoreError::AuthTimeout),
        _ => None,
    }
}

fn read_reply(status: u16, body: &[u8], url: &str) -> Result<Reply, CoreError> {
    if status_is_transient(status) {
        return Err(CoreError::Offline {
            message: format!("HTTP {status}"),
        });
    }
    if (200..300).contains(&status) {
        return serde_json::from_slice::<TokenBody>(body)
            .map(Reply::Ok)
            .map_err(|_| CoreError::Unexpected {
                message: "invalid token response".into(),
            });
    }
    match serde_json::from_slice::<ErrorBody>(body)
        .ok()
        .and_then(|b| b.error)
    {
        Some(error) => Ok(Reply::Error(error)),
        None => Err(http_status(status, url)),
    }
}

fn into_tokens(body: TokenBody, now_ms: i64) -> MsTokens {
    MsTokens {
        access: SecretString::from(body.access_token),
        refresh: body.refresh_token.map(SecretString::from),
        expires_at_ms: now_ms + body.expires_in * 1000,
    }
}

/// 分類できなかったエラーは HttpStatus にする（error の値は入れない）。
fn fail(error: &str, status: u16, url: &str) -> CoreError {
    classify_error(error).unwrap_or_else(|| http_status(status, url))
}

async fn token_request(
    http: &AuthHttp,
    urls: &AuthUrls,
    form: &[(&str, &str)],
    now_ms: i64,
) -> Result<MsTokens, CoreError> {
    let (status, body) = post_form(http, &urls.token, form).await?;
    match read_reply(status, &body, &urls.token)? {
        Reply::Ok(b) => Ok(into_tokens(b, now_ms)),
        Reply::Error(e) => Err(fail(&e, status, &urls.token)),
    }
}

/// 認可コードをトークンに交換する。
pub async fn exchange_code(
    http: &AuthHttp,
    urls: &AuthUrls,
    client_id: &str,
    code: &SecretString,
    verifier: &SecretString,
    redirect_uri: &str,
    now_ms: i64,
) -> Result<MsTokens, CoreError> {
    token_request(
        http,
        urls,
        &[
            ("client_id", client_id),
            ("grant_type", "authorization_code"),
            ("code", code.expose_secret()),
            ("code_verifier", verifier.expose_secret()),
            ("redirect_uri", redirect_uri),
            ("scope", SCOPE),
        ],
        now_ms,
    )
    .await
}

/// refresh token からトークンを取り直す。
pub async fn refresh(
    http: &AuthHttp,
    urls: &AuthUrls,
    client_id: &str,
    refresh: &SecretString,
    now_ms: i64,
) -> Result<MsTokens, CoreError> {
    token_request(
        http,
        urls,
        &[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.expose_secret()),
            ("scope", SCOPE),
        ],
        now_ms,
    )
    .await
}

/// デバイスコードを要求する。
pub async fn request_device_code(
    http: &AuthHttp,
    urls: &AuthUrls,
    client_id: &str,
    now_ms: i64,
) -> Result<DeviceCode, CoreError> {
    let (status, body) = post_form(
        http,
        &urls.device_code,
        &[("client_id", client_id), ("scope", SCOPE)],
    )
    .await?;
    if status_is_transient(status) {
        return Err(CoreError::Offline {
            message: format!("HTTP {status}"),
        });
    }
    if !(200..300).contains(&status) {
        return Err(http_status(status, &urls.device_code));
    }
    let b: DeviceBody = serde_json::from_slice(&body).map_err(|_| CoreError::Unexpected {
        message: "invalid device code response".into(),
    })?;
    Ok(DeviceCode {
        user_code: b.user_code,
        verification_uri: b.verification_uri,
        device_code: SecretString::from(b.device_code),
        interval: Duration::from_secs(b.interval.unwrap_or(DEFAULT_INTERVAL_SECS)),
        expires_at_ms: now_ms + b.expires_in * 1000,
    })
}

/// ユーザーの承認まで、応答の `interval` を守ってトークンのエンドポイントに問い合わせる。
pub async fn poll_device_code(
    http: &AuthHttp,
    urls: &AuthUrls,
    client_id: &str,
    code: &DeviceCode,
    cancel: &CancellationToken,
    now: impl Fn() -> i64,
) -> Result<MsTokens, CoreError> {
    let mut interval = code.interval;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Err(CoreError::Cancelled),
            _ = tokio::time::sleep(interval) => {}
        }
        if now() >= code.expires_at_ms {
            return Err(CoreError::AuthTimeout);
        }
        let form = [
            ("client_id", client_id),
            ("grant_type", DEVICE_GRANT),
            ("device_code", code.device_code.expose_secret()),
        ];
        let (status, body) = tokio::select! {
            _ = cancel.cancelled() => return Err(CoreError::Cancelled),
            r = post_form(http, &urls.token, &form) => r?,
        };
        match read_reply(status, &body, &urls.token)? {
            Reply::Ok(b) => return Ok(into_tokens(b, now())),
            Reply::Error(e) => match e.as_str() {
                "authorization_pending" => {}
                "slow_down" => interval = slowed(interval),
                other => return Err(fail(other, status, &urls.token)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 時計を止めると reqwest のタイムアウトも進んでしまうため、間隔の計算だけを確かめる。
    #[test]
    fn slow_down_adds_five_seconds() {
        assert_eq!(slowed(Duration::ZERO), Duration::from_secs(5));
        assert_eq!(slowed(Duration::from_secs(5)), Duration::from_secs(10));
    }
}
