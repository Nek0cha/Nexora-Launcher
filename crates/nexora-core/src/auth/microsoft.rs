//! Microsoft identity platform のトークンとデバイスコード。

use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::AuthUrls;
use super::http::{AuthHttp, Reply, http_status, send};
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
enum TokenReply {
    Ok(TokenBody),
    /// 400 系の `error` の値。
    Error(String),
}

/// フォームを POST して、応答を読む。本文はエラーに入れない。
async fn post_form(http: &AuthHttp, url: &str, form: &[(&str, &str)]) -> Result<Reply, CoreError> {
    send(http.client().post(url).form(form)).await
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

fn read_reply(reply: &Reply, url: &str) -> Result<TokenReply, CoreError> {
    if reply.is_success() {
        return reply.json("token").map(TokenReply::Ok);
    }
    match serde_json::from_slice::<ErrorBody>(&reply.body)
        .ok()
        .and_then(|b| b.error)
    {
        Some(error) => Ok(TokenReply::Error(error)),
        None => Err(http_status(reply.status, url)),
    }
}

/// 応答の `expires_in`（秒）から期限を求める。極端な値でも溢れないようにする。
fn expires_at(now_ms: i64, expires_in: i64) -> i64 {
    now_ms.saturating_add(expires_in.max(0).saturating_mul(1000))
}

fn into_tokens(body: TokenBody, now_ms: i64) -> MsTokens {
    MsTokens {
        access: SecretString::from(body.access_token),
        refresh: body.refresh_token.map(SecretString::from),
        expires_at_ms: expires_at(now_ms, body.expires_in),
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
    let reply = post_form(http, &urls.token, form).await?;
    match read_reply(&reply, &urls.token)? {
        TokenReply::Ok(b) => Ok(into_tokens(b, now_ms)),
        TokenReply::Error(e) => Err(fail(&e, reply.status, &urls.token)),
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
    let reply = post_form(
        http,
        &urls.device_code,
        &[("client_id", client_id), ("scope", SCOPE)],
    )
    .await?;
    let b: DeviceBody = reply.json_or_status("device code", &urls.device_code)?;
    Ok(DeviceCode {
        user_code: b.user_code,
        verification_uri: b.verification_uri,
        device_code: SecretString::from(b.device_code),
        interval: Duration::from_secs(b.interval.unwrap_or(DEFAULT_INTERVAL_SECS)),
        expires_at_ms: expires_at(now_ms, b.expires_in),
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
        let reply = tokio::select! {
            _ = cancel.cancelled() => return Err(CoreError::Cancelled),
            r = post_form(http, &urls.token, &form) => r?,
        };
        match read_reply(&reply, &urls.token)? {
            TokenReply::Ok(b) => return Ok(into_tokens(b, now())),
            TokenReply::Error(e) => match e.as_str() {
                "authorization_pending" => {}
                "slow_down" => interval = slowed(interval),
                other => return Err(fail(other, reply.status, &urls.token)),
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

    #[test]
    fn expires_at_saturates_and_ignores_negative_values() {
        assert_eq!(expires_at(1_000, 60), 61_000);
        assert_eq!(expires_at(1_000, i64::MAX), i64::MAX);
        assert_eq!(expires_at(i64::MAX - 1, 60), i64::MAX);
        assert_eq!(expires_at(1_000, -5), 1_000);
    }
}
