//! Minecraft Services（ログイン、所有権、プロフィール、スキン）。

use std::path::PathBuf;

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::AuthUrls;
use super::accounts::EntitlementState;
use super::http::{AuthHttp, http_status, json_post, send};
use super::xbox::XstsToken;
use crate::error::CoreError;
use crate::meta::write_atomic;
use crate::net::{Http, hash::hex};
use crate::paths::AppPaths;

/// Java 版の項目の名前（spec 4.4）。
const JAVA_ITEMS: [&str; 2] = ["product_minecraft", "game_minecraft"];
const SKIN_HOST: &str = "textures.minecraft.net";
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Minecraft のアクセストークン。
#[derive(Debug)]
pub struct McToken {
    pub access: SecretString,
    pub expires_at_ms: i64,
}

#[derive(Deserialize)]
struct LoginBody {
    access_token: String,
    expires_in: i64,
}

/// XSTS のトークンから identity token（`XBL3.0 x=<uhs>;<token>`）を作る。
pub fn identity_token(x: &XstsToken) -> SecretString {
    SecretString::from(format!("XBL3.0 x={};{}", x.uhs, x.token.expose_secret()))
}

/// XSTS のトークンを Minecraft のアクセストークンに換える。
pub async fn login_with_xbox(
    http: &AuthHttp,
    urls: &AuthUrls,
    x: &XstsToken,
    now_ms: i64,
) -> Result<McToken, CoreError> {
    let body = json!({ "identityToken": identity_token(x).expose_secret() });
    let reply = send(json_post(http, &urls.mc_login, &body)).await?;
    if reply.status == 403 {
        // 本文は分類のためだけに読む。エラーには入れない。
        let not_approved =
            String::from_utf8_lossy(&reply.body).contains("Invalid app registration");
        return Err(if not_approved {
            CoreError::AppNotApproved
        } else {
            http_status(403, &urls.mc_login)
        });
    }
    let b: LoginBody = reply.json_or_status("Minecraft login", &urls.mc_login)?;
    Ok(McToken {
        access: SecretString::from(b.access_token),
        expires_at_ms: now_ms.saturating_add(b.expires_in.max(0).saturating_mul(1000)),
    })
}

#[derive(Deserialize)]
struct Items {
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    name: String,
}

fn has_java(items: &[String]) -> bool {
    items.iter().any(|n| JAVA_ITEMS.contains(&n.as_str()))
}

/// 2 つの一覧から所有権を判断する。None は問い合わせの失敗。
/// 空でない一覧だけでは所有の証拠にしない。
pub fn judge_entitlement(store: Option<&[String]>, license: Option<&[String]>) -> EntitlementState {
    if store.is_some_and(has_java) || license.is_some_and(has_java) {
        EntitlementState::Verified
    } else if store.is_none() || license.is_none() {
        EntitlementState::Unknown
    } else {
        EntitlementState::NotOwned
    }
}

/// 項目の名前の一覧を取る。失敗は None。
async fn item_names(http: &AuthHttp, url: &str, mc: &McToken) -> Option<Vec<String>> {
    let request = http
        .client()
        .get(url)
        .header("Accept", "application/json")
        .bearer_auth(mc.access.expose_secret());
    let reply = send(request).await.ok()?;
    if !reply.is_success() {
        return None;
    }
    let items: Items = reply.json("entitlement").ok()?;
    Some(items.items.into_iter().map(|i| i.name).collect())
}

/// 所有権を確認する。`mcstore` と `license` を並行に問い合わせ、失敗は Unknown 側に倒す。
pub async fn entitlement(http: &AuthHttp, urls: &AuthUrls, mc: &McToken) -> EntitlementState {
    let sep = if urls.mc_license.contains('?') {
        '&'
    } else {
        '?'
    };
    let license_url = format!("{}{sep}requestId={}", urls.mc_license, Uuid::new_v4());
    let (store, license) = tokio::join!(
        item_names(http, &urls.mc_store, mc),
        item_names(http, &license_url, mc),
    );
    judge_entitlement(store.as_deref(), license.as_deref())
}

/// Minecraft のプロフィール。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub skin_url: Option<String>,
}

#[derive(Deserialize)]
struct ProfileBody {
    id: String,
    name: String,
    #[serde(default)]
    skins: Vec<SkinBody>,
}

#[derive(Deserialize)]
struct SkinBody {
    state: Option<String>,
    url: Option<String>,
}

/// `textures.minecraft.net` の https だけを受け付ける。
fn trusted_skin_url(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| u.scheme() == "https" && u.host_str() == Some(SKIN_HOST))
}

/// プロフィールを読む。プロフィールがなければ NoProfile。
pub async fn profile(http: &AuthHttp, urls: &AuthUrls, mc: &McToken) -> Result<Profile, CoreError> {
    let request = http
        .client()
        .get(&urls.mc_profile)
        .header("Accept", "application/json")
        .bearer_auth(mc.access.expose_secret());
    let reply = send(request).await?;
    if reply.status == 404 {
        return Err(CoreError::NoProfile);
    }
    let b: ProfileBody = reply.json_or_status("Minecraft profile", &urls.mc_profile)?;
    let id = Uuid::parse_str(&b.id).map_err(|_| CoreError::Unexpected {
        message: "invalid Minecraft profile id".into(),
    })?;
    let skin_url = b
        .skins
        .into_iter()
        .find(|s| s.state.as_deref() == Some("ACTIVE"))
        .and_then(|s| s.url)
        .filter(|u| trusted_skin_url(u));
    Ok(Profile {
        id,
        name: b.name,
        skin_url,
    })
}

/// スキンの画像を取得して `cache/skins/<URL の SHA-256>.png` に保存する。あればそれを返す。
pub async fn cached_skin(paths: &AppPaths, http: &Http, url: &str) -> Result<PathBuf, CoreError> {
    let file = paths
        .skins_dir()
        .join(format!("{}.png", hex(&Sha256::digest(url.as_bytes()))));
    if file.is_file() {
        return Ok(file);
    }
    let bytes = http.get_bytes(url).await?;
    if !bytes.starts_with(PNG_MAGIC) {
        return Err(CoreError::Unexpected {
            message: "skin is not a PNG image".into(),
        });
    }
    write_atomic(&file, &bytes)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn java_item_in_either_list_is_verified() {
        let java = names(&["product_minecraft"]);
        let game = names(&["game_minecraft"]);
        assert_eq!(
            judge_entitlement(Some(&java), Some(&[])),
            EntitlementState::Verified
        );
        assert_eq!(
            judge_entitlement(Some(&[]), Some(&game)),
            EntitlementState::Verified
        );
    }

    #[test]
    fn other_items_only_are_not_owned() {
        let a = names(&["product_minecraft_bedrock"]);
        let b = names(&["game_minecraft_bedrock"]);
        assert_eq!(
            judge_entitlement(Some(&a), Some(&b)),
            EntitlementState::NotOwned
        );
    }

    #[test]
    fn both_empty_is_not_owned() {
        assert_eq!(
            judge_entitlement(Some(&[]), Some(&[])),
            EntitlementState::NotOwned
        );
    }

    #[test]
    fn a_failed_query_without_java_items_is_unknown() {
        assert_eq!(
            judge_entitlement(None, Some(&[])),
            EntitlementState::Unknown
        );
        assert_eq!(
            judge_entitlement(Some(&[]), None),
            EntitlementState::Unknown
        );
        assert_eq!(judge_entitlement(None, None), EntitlementState::Unknown);
    }

    #[test]
    fn a_failed_query_with_java_items_in_the_other_is_verified() {
        let java = names(&["game_minecraft"]);
        assert_eq!(
            judge_entitlement(None, Some(&java)),
            EntitlementState::Verified
        );
        assert_eq!(
            judge_entitlement(Some(&java), None),
            EntitlementState::Verified
        );
    }
}
