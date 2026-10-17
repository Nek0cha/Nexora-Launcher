//! Xbox Live のユーザートークンと XSTS。

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::json;

use super::AuthUrls;
use super::http::{AuthHttp, http_status, json_post, send};
use crate::error::CoreError;
use crate::install::now_ms;

const XSTS_RELYING_PARTY: &str = "rp://api.minecraftservices.com/";
/// `NotAfter` を読めないときに仮定する有効時間。
const FALLBACK_LIFETIME_MS: i64 = 60 * 60 * 1000;

/// Xbox Live のユーザートークン。
#[derive(Debug)]
pub struct UserToken {
    pub token: SecretString,
    pub expires_at_ms: i64,
}

/// Minecraft 向けの XSTS トークン。
#[derive(Debug)]
pub struct XstsToken {
    pub token: SecretString,
    pub uhs: String,
    pub xuid: Option<String>,
    pub expires_at_ms: i64,
}

#[derive(Deserialize)]
struct TokenBody {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "NotAfter")]
    not_after: Option<String>,
    #[serde(rename = "DisplayClaims")]
    display_claims: Option<Claims>,
}

#[derive(Deserialize)]
struct Claims {
    xui: Vec<Xui>,
}

#[derive(Deserialize)]
struct Xui {
    uhs: Option<String>,
    xid: Option<String>,
}

#[derive(Deserialize)]
struct ErrBody {
    #[serde(rename = "XErr")]
    xerr: Option<u64>,
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// RFC 3339 の日時をエポックからのミリ秒にする。小数秒は切り捨てる。読めなければ None。
fn parse_rfc3339_ms(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> {
        let part = s.get(from..from + len)?;
        if part.bytes().all(|c| c.is_ascii_digit()) {
            part.parse().ok()
        } else {
            None
        }
    };
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    if b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (y, mo, d) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (h, mi, sec) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return None;
        }
    }
    let offset_min = match *b.get(i)? {
        b'Z' | b'z' if i + 1 == b.len() => 0,
        sign @ (b'+' | b'-') if i + 6 == b.len() && b[i + 3] == b':' => {
            let (oh, om) = (num(i + 1, 2)?, num(i + 4, 2)?);
            if oh > 23 || om > 59 {
                return None;
            }
            let v = oh * 60 + om;
            if sign == b'+' { v } else { -v }
        }
        _ => return None,
    };
    let secs = days_from_civil(y, mo, d)
        .saturating_mul(86_400)
        .saturating_add(h * 3600 + mi * 60 + sec)
        .saturating_sub(offset_min * 60);
    Some(secs.saturating_mul(1000))
}

fn expiry(not_after: Option<&str>) -> i64 {
    match not_after.and_then(parse_rfc3339_ms) {
        Some(ms) => ms,
        None => {
            tracing::warn!("could not parse Xbox NotAfter; assuming one hour");
            now_ms().saturating_add(FALLBACK_LIFETIME_MS)
        }
    }
}

/// JSON を POST して、応答を読む。本文はエラーに入れない。
async fn post_json(
    http: &AuthHttp,
    url: &str,
    body: &serde_json::Value,
) -> Result<TokenBody, CoreError> {
    let reply = send(json_post(http, url, body).header("x-xbl-contract-version", "1")).await?;
    if reply.is_success() {
        return reply.json("Xbox");
    }
    match serde_json::from_slice::<ErrBody>(&reply.body)
        .ok()
        .and_then(|b| b.xerr)
    {
        Some(xerr) => Err(CoreError::AuthXbox { xerr }),
        None => Err(http_status(reply.status, url)),
    }
}

/// Microsoft のアクセストークンを Xbox Live のユーザートークンに換える。
pub async fn user_token(
    http: &AuthHttp,
    urls: &AuthUrls,
    ms_access: &SecretString,
) -> Result<UserToken, CoreError> {
    let body = json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={}", ms_access.expose_secret()),
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    });
    let reply = post_json(http, &urls.xbox_user, &body).await?;
    Ok(UserToken {
        token: SecretString::from(reply.token),
        expires_at_ms: expiry(reply.not_after.as_deref()),
    })
}

/// ユーザートークンを Minecraft 向けの XSTS トークンに換える。
pub async fn xsts(
    http: &AuthHttp,
    urls: &AuthUrls,
    user: &UserToken,
) -> Result<XstsToken, CoreError> {
    let body = json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [user.token.expose_secret()],
        },
        "RelyingParty": XSTS_RELYING_PARTY,
        "TokenType": "JWT",
    });
    let reply = post_json(http, &urls.xsts, &body).await?;
    let no_hash = || CoreError::Unexpected {
        message: "XSTS response has no user hash".into(),
    };
    let xui = reply
        .display_claims
        .and_then(|c| c.xui.into_iter().next())
        .ok_or_else(no_hash)?;
    let uhs = xui.uhs.ok_or_else(no_hash)?;
    Ok(XstsToken {
        token: SecretString::from(reply.token),
        uhs,
        xuid: xui.xid,
        expires_at_ms: expiry(reply.not_after.as_deref()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339() {
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_rfc3339_ms("2030-01-01T00:00:00.9999999Z"),
            Some(1_893_456_000_000)
        );
        assert_eq!(
            parse_rfc3339_ms("2030-01-01T09:00:00+09:00"),
            Some(1_893_456_000_000)
        );
        assert_eq!(
            parse_rfc3339_ms("2029-12-31T19:00:00-05:00"),
            Some(1_893_456_000_000)
        );
        assert_eq!(
            parse_rfc3339_ms("2024-02-29T12:00:00Z"),
            Some(1_709_208_000_000)
        );
    }

    #[test]
    fn rejects_garbage_without_panicking() {
        for s in [
            "",
            "garbage",
            "2030-13-01T00:00:00Z",
            "2030-01-01T00:00:00",
            "２０３０-01-01T00:00:00Z",
            "9999-99-99T99:99:99+99:99",
            "2030-01-01T00:00:00.Z",
        ] {
            assert_eq!(parse_rfc3339_ms(s), None, "{s}");
        }
        assert!(parse_rfc3339_ms("9999-12-31T23:59:59Z").is_some());
    }
}
