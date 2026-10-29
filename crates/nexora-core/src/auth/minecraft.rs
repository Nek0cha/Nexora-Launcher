//! Minecraft Services（ログイン、所有権、プロフィール、スキン）。

use std::path::PathBuf;

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use ts_rs::TS;
use uuid::Uuid;

use super::AuthUrls;
use super::accounts::EntitlementState;
use super::http::{
    AuthHttp, http_status, json_post, send, send_any_status, status_is_transient, transient,
    transient_status,
};
use super::xbox::XstsToken;
use crate::error::CoreError;
use crate::meta::write_atomic;
use crate::net::hash::hex;
use crate::paths::AppPaths;

/// Java 版の項目の名前（spec 4.4）。
const JAVA_ITEMS: [&str; 2] = ["product_minecraft", "game_minecraft"];
const SKIN_HOST: &str = "textures.minecraft.net";
/// スキンの画像の上限。
const MAX_SKIN_BYTES: usize = 1024 * 1024;
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
/// 変更に使うスキンの画像の上限。
pub const SKIN_MAX_BYTES: usize = 64 * 1024;
/// 受け付けるスキンの寸法（幅、高さ）。
const SKIN_SIZES: [(u32, u32); 2] = [(64, 64), (64, 32)];

/// Minecraft のアクセストークン。
#[derive(Debug, Clone)]
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

/// スキンのモデル。値は Minecraft Services の API と同じ（`classic`、`slim`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SkinVariant {
    Classic,
    Slim,
}

impl SkinVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            SkinVariant::Classic => "classic",
            SkinVariant::Slim => "slim",
        }
    }

    /// プロフィールの `variant`（`CLASSIC`、`SLIM`）を読む。大文字と小文字は区別しない。
    fn parse(s: &str) -> Option<SkinVariant> {
        if s.eq_ignore_ascii_case("classic") {
            Some(SkinVariant::Classic)
        } else if s.eq_ignore_ascii_case("slim") {
            Some(SkinVariant::Slim)
        } else {
            None
        }
    }
}

/// 所有しているマント。`url` は取得に使う https の URL。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Cape {
    pub id: String,
    pub alias: String,
    pub url: String,
    pub active: bool,
}

/// Minecraft のプロフィール。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub skin_url: Option<String>,
    /// 使用中のスキンのモデル。プロフィールに書かれていなければ None。
    pub skin_variant: Option<SkinVariant>,
    /// 所有しているマント。信頼できない URL のものは含めない。
    pub capes: Vec<Cape>,
}

#[derive(Deserialize)]
struct ProfileBody {
    id: String,
    name: String,
    #[serde(default)]
    skins: Vec<SkinBody>,
    #[serde(default)]
    capes: Vec<CapeBody>,
}

#[derive(Deserialize)]
struct CapeBody {
    id: String,
    state: Option<String>,
    url: Option<String>,
    alias: Option<String>,
}

#[derive(Deserialize)]
struct SkinBody {
    state: Option<String>,
    url: Option<String>,
    variant: Option<String>,
}

/// `textures.minecraft.net` の https（ポートの指定なし）だけを受け付ける。
fn trusted_url(u: &reqwest::Url) -> bool {
    u.scheme() == "https" && u.host_str() == Some(SKIN_HOST) && u.port().is_none()
}

/// プロフィールのスキンの URL を、取得に使う https の URL にする。
/// Minecraft Services は `http://textures.minecraft.net/...` を返すため、http は https に置き換える。
fn skin_url_from_profile(url: &str) -> Option<String> {
    let mut u = reqwest::Url::parse(url).ok()?;
    if u.scheme() == "http" && u.port().is_none() {
        u.set_scheme("https").ok()?;
    }
    trusted_url(&u).then(|| u.to_string())
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
    let active = b
        .skins
        .into_iter()
        .find(|s| s.state.as_deref() == Some("ACTIVE"));
    let skin_variant = active
        .as_ref()
        .and_then(|s| s.variant.as_deref())
        .and_then(SkinVariant::parse);
    let skin_url = active
        .and_then(|s| s.url)
        .and_then(|u| skin_url_from_profile(&u));
    let capes = b
        .capes
        .into_iter()
        .filter_map(|c| {
            Some(Cape {
                url: skin_url_from_profile(&c.url?)?,
                active: c.state.as_deref() == Some("ACTIVE"),
                alias: c.alias.unwrap_or_else(|| c.id.clone()),
                id: c.id,
            })
        })
        .collect();
    Ok(Profile {
        id,
        name: b.name,
        skin_url,
        skin_variant,
        capes,
    })
}

fn invalid_skin(reason: &str) -> CoreError {
    CoreError::InvalidSkin {
        reason: reason.into(),
    }
}

/// 変更に使うスキンの画像を、通信の前に確かめる。PNG の署名、最初のチャンクが IHDR であること、
/// IHDR の寸法（64×64 か 64×32）、大きさ（`SKIN_MAX_BYTES` 以下）を見る。CRC と画素は見ない
/// （最終的な判定は Minecraft Services が行う）。
pub fn validate_skin_png(bytes: &[u8]) -> Result<(), CoreError> {
    if bytes.len() > SKIN_MAX_BYTES {
        return Err(invalid_skin("too_large"));
    }
    if !bytes.starts_with(PNG_MAGIC) {
        return Err(invalid_skin("not_png"));
    }
    // 署名（8 バイト）の直後に、長さ（4）、種類（4）、幅（4）、高さ（4）が続く
    let header = bytes.get(8..24).ok_or_else(|| invalid_skin("not_png"))?;
    let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
    if be(&header[0..4]) != 13 || &header[4..8] != b"IHDR" {
        return Err(invalid_skin("not_png"));
    }
    let size = (be(&header[8..12]), be(&header[12..16]));
    if !SKIN_SIZES.contains(&size) {
        return Err(invalid_skin("dimensions"));
    }
    Ok(())
}

/// スキンを変更する（`POST {mc_skins}`、multipart の `variant` と `file`）。
/// 画像は呼び出し側が `validate_skin_png` で確かめておく。
///
/// 429 は一時的な失敗（Offline）にせず `HttpStatus { status: 429 }` として返し、画面に
/// レート制限であることを出せるようにする。5xx と通信の失敗は Offline。本文はエラーに入れない。
pub async fn upload_skin(
    http: &AuthHttp,
    urls: &AuthUrls,
    mc: &McToken,
    variant: SkinVariant,
    png: Vec<u8>,
) -> Result<(), CoreError> {
    let file = reqwest::multipart::Part::bytes(png)
        .file_name("skin.png")
        .mime_str("image/png")
        .map_err(|_| CoreError::Unexpected {
            message: "could not build the skin upload".into(),
        })?;
    let form = reqwest::multipart::Form::new()
        .text("variant", variant.as_str())
        .part("file", file);
    let request = http
        .client()
        .post(&urls.mc_skins)
        .header("Accept", "application/json")
        .bearer_auth(mc.access.expose_secret())
        .multipart(form);
    let reply = send_any_status(request).await?;
    if reply.status >= 500 {
        return Err(transient_status(reply.status));
    }
    if !reply.is_success() {
        return Err(http_status(reply.status, &urls.mc_skins));
    }
    Ok(())
}

/// 使用中のマントを変える。`Some(id)` は `PUT {mc_cape_active}` に `{"capeId": id}`、
/// `None` は `DELETE {mc_cape_active}`（マントを外す）。
/// 429 は `HttpStatus { status: 429 }`、5xx と通信の失敗は Offline。本文はエラーに入れない。
pub async fn set_active_cape(
    http: &AuthHttp,
    urls: &AuthUrls,
    mc: &McToken,
    cape_id: Option<&str>,
) -> Result<(), CoreError> {
    let client = http.client();
    let request = match cape_id {
        Some(id) => client
            .put(&urls.mc_cape_active)
            .header("Content-Type", "application/json")
            .body(json!({ "capeId": id }).to_string()),
        None => client.delete(&urls.mc_cape_active),
    }
    .header("Accept", "application/json")
    .bearer_auth(mc.access.expose_secret());
    let reply = send_any_status(request).await?;
    if reply.status >= 500 {
        return Err(transient_status(reply.status));
    }
    if !reply.is_success() {
        return Err(http_status(reply.status, &urls.mc_cape_active));
    }
    Ok(())
}

/// スキンの画像を取得して `cache/skins/<URL の SHA-256>.png` に保存する。あればそれを返す。
/// 許可された URL 以外は、通信せずに拒否する。取得はリダイレクトを追わない専用の
/// クライアントで行う。
pub async fn cached_skin(paths: &AppPaths, url: &str) -> Result<PathBuf, CoreError> {
    fetch_skin(paths, url, trusted_url).await
}

fn too_large() -> CoreError {
    CoreError::Unexpected {
        message: "skin image is too large".into(),
    }
}

fn untrusted_skin() -> CoreError {
    CoreError::Unexpected {
        message: "untrusted skin URL".into(),
    }
}

/// `allow` が許す URL だけを取得する。
async fn fetch_skin(
    paths: &AppPaths,
    url: &str,
    allow: impl Fn(&reqwest::Url) -> bool,
) -> Result<PathBuf, CoreError> {
    let parsed = reqwest::Url::parse(url)
        .ok()
        .filter(|u| allow(u))
        .ok_or_else(untrusted_skin)?;
    let file = paths
        .skins_dir()
        .join(format!("{}.png", hex(&Sha256::digest(url.as_bytes()))));
    if file.is_file() {
        return Ok(file);
    }
    // リダイレクトを追わない。許可されたホストの外へは一切要求しない。
    let mut response = AuthHttp::new()?
        .client()
        .get(parsed)
        .send()
        .await
        .map_err(|e| transient(&e))?;
    let status = response.status().as_u16();
    if status_is_transient(status) {
        return Err(transient_status(status));
    }
    if !(200..300).contains(&status) {
        return Err(http_status(status, "skin"));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_SKIN_BYTES as u64)
    {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| transient(&e))? {
        if bytes.len() + chunk.len() > MAX_SKIN_BYTES {
            return Err(too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
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

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest";

    fn port_of(server: &MockServer) -> u16 {
        reqwest::Url::parse(&server.uri()).unwrap().port().unwrap()
    }

    async fn serve_png(server: &MockServer, route: &str, expect: u64) {
        Mock::given(method("GET"))
            .and(path(route.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG.to_vec()))
            .expect(expect)
            .mount(server)
            .await;
    }

    #[test]
    fn only_textures_host_without_a_port_is_trusted() {
        let https = Some("https://textures.minecraft.net/texture/a".to_string());
        assert_eq!(
            skin_url_from_profile("https://textures.minecraft.net/texture/a"),
            https
        );
        assert_eq!(
            skin_url_from_profile("https://textures.minecraft.net:443/texture/a"),
            https
        );
        assert_eq!(
            skin_url_from_profile("http://textures.minecraft.net/texture/a"),
            https
        );
        assert_eq!(
            skin_url_from_profile("https://textures.minecraft.net:8443/texture/a"),
            None
        );
        assert_eq!(
            skin_url_from_profile("http://textures.minecraft.net:8080/texture/a"),
            None
        );
        assert_eq!(skin_url_from_profile("http://evil.example/texture/a"), None);
        assert_eq!(
            skin_url_from_profile("https://evil.example/texture/a"),
            None
        );
    }

    #[tokio::test]
    async fn skin_is_cached_by_url_hash() {
        let server = MockServer::start().await;
        serve_png(&server, "/texture/a", 1).await;
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path());
        let url = format!("{}/texture/a", server.uri());
        let allow = |u: &reqwest::Url| u.port() == Some(port_of(&server));
        let first = fetch_skin(&paths, &url, allow).await.unwrap();
        let second = fetch_skin(&paths, &url, allow).await.unwrap();
        assert_eq!(first, second);
        assert!(first.starts_with(paths.skins_dir()));
        assert_eq!(std::fs::read(&first).unwrap(), PNG);
    }

    #[tokio::test]
    async fn an_untrusted_url_is_refused_without_a_request() {
        let server = MockServer::start().await;
        serve_png(&server, "/texture/a", 0).await;
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path());
        let url = format!("{}/texture/a", server.uri());
        // 本物の許可判定では、wiremock の http URL は通らない
        let err = cached_skin(&paths, &url).await.unwrap_err();
        assert!(matches!(err, CoreError::Unexpected { .. }), "{err:?}");
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_redirect_off_the_allowed_host_is_refused() {
        let other = MockServer::start().await;
        serve_png(&other, "/texture/b", 0).await;
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/texture/a"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("Location", format!("{}/texture/b", other.uri())),
            )
            .mount(&server)
            .await;
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path());
        let url = format!("{}/texture/a", server.uri());
        let allowed = port_of(&server);
        let result = fetch_skin(&paths, &url, |u| u.port() == Some(allowed)).await;
        assert!(result.is_err());
        assert!(!paths.skins_dir().exists());
    }

    #[tokio::test]
    async fn an_oversized_skin_is_rejected() {
        let server = MockServer::start().await;
        let mut big = PNG.to_vec();
        big.resize(MAX_SKIN_BYTES + 1, 0);
        Mock::given(method("GET"))
            .and(path("/texture/a"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(big))
            .mount(&server)
            .await;
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path());
        let url = format!("{}/texture/a", server.uri());
        let port = port_of(&server);
        let result = fetch_skin(&paths, &url, |u| u.port() == Some(port)).await;
        assert!(result.is_err());
        assert!(!paths.skins_dir().exists());
    }

    #[tokio::test]
    async fn a_non_png_skin_is_rejected_and_not_cached() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/texture/a"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"<html>".to_vec()))
            .mount(&server)
            .await;
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path());
        let url = format!("{}/texture/a", server.uri());
        let port = port_of(&server);
        let result = fetch_skin(&paths, &url, |u| u.port() == Some(port)).await;
        assert!(result.is_err());
        assert!(!paths.skins_dir().exists());
    }

    /// 署名と IHDR だけの PNG（幅 w、高さ h）。CRC と後続のチャンクは検証しないため省く。
    fn skin_png(w: u32, h: u32) -> Vec<u8> {
        let mut v = PNG_MAGIC.to_vec();
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]);
        v.extend_from_slice(&[0, 0, 0, 0]);
        v
    }

    fn invalid(bytes: &[u8]) -> bool {
        matches!(validate_skin_png(bytes), Err(CoreError::InvalidSkin { .. }))
    }

    #[test]
    fn validate_skin_png_accepts_64x64_and_64x32() {
        validate_skin_png(&skin_png(64, 64)).unwrap();
        validate_skin_png(&skin_png(64, 32)).unwrap();
        // ちょうど上限の大きさは受け付ける
        let mut max = skin_png(64, 64);
        max.resize(SKIN_MAX_BYTES, 0);
        validate_skin_png(&max).unwrap();
    }

    #[test]
    fn validate_skin_png_rejects_other_sizes_non_png_and_huge() {
        assert_eq!(SKIN_MAX_BYTES, 64 * 1024);
        for (w, h) in [(32, 32), (128, 128), (32, 64), (64, 65), (0, 0)] {
            assert!(invalid(&skin_png(w, h)), "{w}x{h}");
        }
        assert!(invalid(b""));
        assert!(invalid(b"<html>not a png</html>"));
        assert!(invalid(PNG_MAGIC));
        // IHDR が最初のチャンクでない
        let mut wrong_chunk = skin_png(64, 64);
        wrong_chunk[12..16].copy_from_slice(b"IDAT");
        assert!(invalid(&wrong_chunk));
        // IHDR の長さが 13 でない
        let mut wrong_len = skin_png(64, 64);
        wrong_len[8..12].copy_from_slice(&12u32.to_be_bytes());
        assert!(invalid(&wrong_len));
        let mut huge = skin_png(64, 64);
        huge.resize(SKIN_MAX_BYTES + 1, 0);
        assert!(invalid(&huge));
        let err = validate_skin_png(&huge).unwrap_err();
        assert_eq!(err.code(), "skin.invalid");
    }

    #[test]
    fn skin_variant_uses_the_api_values() {
        assert_eq!(SkinVariant::Classic.as_str(), "classic");
        assert_eq!(SkinVariant::Slim.as_str(), "slim");
        assert_eq!(
            serde_json::to_string(&SkinVariant::Slim).unwrap(),
            r#""slim""#
        );
        assert_eq!(
            serde_json::from_str::<SkinVariant>(r#""classic""#).unwrap(),
            SkinVariant::Classic
        );
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
