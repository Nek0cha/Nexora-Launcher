//! Microsoft 認証の結合テスト（wiremock）。

mod common;

use nexora_core::auth::http::AuthHttp;
use nexora_core::auth::microsoft::{exchange_code, poll_device_code, refresh, request_device_code};
use nexora_core::auth::xbox::{UserToken, user_token, xsts};
use nexora_core::error::CoreError;
use secrecy::{ExposeSecret, SecretString};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use common::auth_urls;

const NOW: i64 = 1_000_000;

fn secret(s: &str) -> SecretString {
    SecretString::from(s.to_string())
}

async fn token_error(server: &MockServer, status: u16, error: &str) {
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(status).set_body_string(format!(
            r#"{{"error":"{error}","error_description":"SECRET-DETAIL"}}"#
        )))
        .mount(server)
        .await;
}

async fn refresh_err(server: &MockServer) -> CoreError {
    refresh(
        &AuthHttp::new().unwrap(),
        &auth_urls(server),
        "cid",
        &secret("r"),
        NOW,
    )
    .await
    .unwrap_err()
}

#[tokio::test]
async fn exchange_code_posts_the_verifier_and_parses_tokens() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains("code_verifier=the-verifier"))
        .and(body_string_contains("client_id=cid"))
        .and(body_string_contains(
            "redirect_uri=http%3A%2F%2Flocalhost%3A1234",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"access_token":"A","refresh_token":"R","expires_in":3600,"token_type":"Bearer"}"#,
        ))
        .mount(&server)
        .await;
    let http = AuthHttp::new().unwrap();
    let tokens = exchange_code(
        &http,
        &auth_urls(&server),
        "cid",
        &secret("the-code"),
        &secret("the-verifier"),
        "http://localhost:1234",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(tokens.access.expose_secret(), "A");
    assert_eq!(tokens.refresh.unwrap().expose_secret(), "R");
    assert_eq!(tokens.expires_at_ms, NOW + 3_600_000);
    let body =
        String::from_utf8(server.received_requests().await.unwrap()[0].body.clone()).unwrap();
    assert!(!body.contains("client_secret"));
}

#[tokio::test]
async fn refresh_invalid_grant_is_login_required() {
    let server = MockServer::start().await;
    token_error(&server, 400, "invalid_grant").await;
    let err = refresh_err(&server).await;
    assert_eq!(err.code(), "auth.login_required");
    assert!(!format!("{err:?}").contains("SECRET-DETAIL"));
}

#[tokio::test]
async fn refresh_interaction_required_is_login_required() {
    let server = MockServer::start().await;
    token_error(&server, 400, "interaction_required").await;
    assert_eq!(refresh_err(&server).await.code(), "auth.login_required");
}

#[tokio::test]
async fn refresh_server_error_is_transient() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let err = refresh_err(&server).await;
    assert!(matches!(err, CoreError::Offline { .. }), "{err:?}");
}

#[tokio::test]
async fn refresh_other_client_error_is_http_status() {
    let server = MockServer::start().await;
    token_error(&server, 400, "invalid_request").await;
    let err = refresh_err(&server).await;
    assert!(
        matches!(err, CoreError::HttpStatus { status: 400, .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn refresh_without_a_new_refresh_token_returns_none() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=old"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"access_token":"A2","expires_in":60}"#),
        )
        .mount(&server)
        .await;
    let tokens = refresh(
        &AuthHttp::new().unwrap(),
        &auth_urls(&server),
        "cid",
        &secret("old"),
        NOW,
    )
    .await
    .unwrap();
    assert!(tokens.refresh.is_none());
    assert_eq!(tokens.expires_at_ms, NOW + 60_000);
}

#[tokio::test]
async fn redirects_are_not_followed() {
    let server = MockServer::start().await;
    let target = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", format!("{}/evil", target.uri())),
        )
        .mount(&server)
        .await;
    let err = refresh_err(&server).await;
    assert!(
        matches!(err, CoreError::HttpStatus { status: 302, .. }),
        "{err:?}"
    );
    assert!(target.received_requests().await.unwrap().is_empty());
}

async fn device_server(interval: u64) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/devicecode"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"device_code":"DC","user_code":"ABCD-EFGH","verification_uri":"https://www.microsoft.com/link","expires_in":900,"interval":{interval}}}"#
        )))
        .mount(&server)
        .await;
    server
}

async fn poll_state(server: &MockServer, status: u16, body: &str, times: Option<u64>) {
    let mock = Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("device_code=DC"))
        .and(body_string_contains(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code",
        ))
        .respond_with(ResponseTemplate::new(status).set_body_string(body.to_string()));
    match times {
        Some(n) => mock.up_to_n_times(n).mount(server).await,
        None => mock.mount(server).await,
    }
}

const PENDING: &str = r#"{"error":"authorization_pending"}"#;
const OK: &str = r#"{"access_token":"A","refresh_token":"R","expires_in":3600}"#;

#[tokio::test]
async fn device_code_polls_until_authorized() {
    let server = device_server(0).await;
    poll_state(&server, 400, PENDING, Some(2)).await;
    poll_state(&server, 200, OK, None).await;
    let http = AuthHttp::new().unwrap();
    let urls = auth_urls(&server);
    let dc = request_device_code(&http, &urls, "cid", NOW).await.unwrap();
    assert_eq!(dc.user_code, "ABCD-EFGH");
    assert_eq!(dc.verification_uri, "https://www.microsoft.com/link");
    let tokens = poll_device_code(&http, &urls, "cid", &dc, &CancellationToken::new(), || NOW)
        .await
        .unwrap();
    assert_eq!(tokens.access.expose_secret(), "A");
    let polls = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/token")
        .count();
    assert_eq!(polls, 3);
}

#[tokio::test]
async fn device_code_declined_and_expired() {
    for (error, code) in [
        ("authorization_declined", "auth.denied"),
        ("expired_token", "auth.timeout"),
    ] {
        let server = device_server(0).await;
        poll_state(&server, 400, &format!(r#"{{"error":"{error}"}}"#), None).await;
        let http = AuthHttp::new().unwrap();
        let urls = auth_urls(&server);
        let dc = request_device_code(&http, &urls, "cid", NOW).await.unwrap();
        let err = poll_device_code(&http, &urls, "cid", &dc, &CancellationToken::new(), || NOW)
            .await
            .unwrap_err();
        assert_eq!(err.code(), code);
    }
}

#[tokio::test]
async fn device_code_times_out_locally() {
    let server = device_server(0).await;
    poll_state(&server, 400, PENDING, None).await;
    let http = AuthHttp::new().unwrap();
    let urls = auth_urls(&server);
    let dc = request_device_code(&http, &urls, "cid", NOW).await.unwrap();
    let err = poll_device_code(&http, &urls, "cid", &dc, &CancellationToken::new(), || {
        NOW + 900_000
    })
    .await
    .unwrap_err();
    assert_eq!(err.code(), "auth.timeout");
}

#[tokio::test]
async fn device_code_cancel() {
    let server = device_server(0).await;
    poll_state(&server, 400, PENDING, None).await;
    let http = AuthHttp::new().unwrap();
    let urls = auth_urls(&server);
    let dc = request_device_code(&http, &urls, "cid", NOW).await.unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = poll_device_code(&http, &urls, "cid", &dc, &cancel, || NOW)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::Cancelled));
}

// ---- Xbox / XSTS ----

use wiremock::matchers::{body_json, header};

/// 2030-01-01T00:00:00Z のミリ秒。
const Y2030_MS: i64 = 1_893_456_000_000;

#[tokio::test]
async fn user_token_sends_the_rps_ticket() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xbox/user"))
        .and(header("accept", "application/json"))
        .and(header("x-xbl-contract-version", "1"))
        .and(body_json(serde_json::json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": "d=ms-access"
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"Token":"utok","NotAfter":"2030-01-01T00:00:00.1234567Z","DisplayClaims":{"xui":[{"uhs":"h"}]}}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;
    let t = user_token(
        &AuthHttp::new().unwrap(),
        &auth_urls(&server),
        &secret("ms-access"),
    )
    .await
    .unwrap();
    assert_eq!(t.token.expose_secret(), "utok");
    assert_eq!(t.expires_at_ms, Y2030_MS);
}

#[tokio::test]
async fn user_token_unparseable_not_after_falls_back_to_one_hour() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xbox/user"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"Token":"utok","NotAfter":"garbage"}"#),
        )
        .mount(&server)
        .await;
    let before = nexora_core::install::now_ms();
    let t = user_token(&AuthHttp::new().unwrap(), &auth_urls(&server), &secret("m"))
        .await
        .unwrap();
    let hour = 3_600_000;
    assert!(t.expires_at_ms >= before + hour && t.expires_at_ms <= before + hour + 60_000);
}

fn user(token: &str) -> UserToken {
    UserToken {
        token: secret(token),
        expires_at_ms: 0,
    }
}

#[tokio::test]
async fn xsts_reads_uhs_and_xid() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xsts"))
        .and(header("x-xbl-contract-version", "1"))
        .and(body_json(serde_json::json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": ["utok"] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"Token":"xtok","NotAfter":"2030-01-01T09:00:00+09:00","DisplayClaims":{"xui":[{"uhs":"hash","xid":"2535"}]}}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;
    let x = xsts(
        &AuthHttp::new().unwrap(),
        &auth_urls(&server),
        &user("utok"),
    )
    .await
    .unwrap();
    assert_eq!(x.token.expose_secret(), "xtok");
    assert_eq!(x.uhs, "hash");
    assert_eq!(x.xuid.as_deref(), Some("2535"));
    assert_eq!(x.expires_at_ms, Y2030_MS);
}

#[tokio::test]
async fn xsts_without_xid_is_none() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xsts"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"Token":"xtok","NotAfter":"2030-01-01T00:00:00Z","DisplayClaims":{"xui":[{"uhs":"hash"}]}}"#,
        ))
        .mount(&server)
        .await;
    let x = xsts(&AuthHttp::new().unwrap(), &auth_urls(&server), &user("u"))
        .await
        .unwrap();
    assert_eq!(x.xuid, None);
}

async fn xsts_err(status: u16, body: &str) -> CoreError {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xsts"))
        .respond_with(ResponseTemplate::new(status).set_body_string(body.to_string()))
        .mount(&server)
        .await;
    xsts(&AuthHttp::new().unwrap(), &auth_urls(&server), &user("u"))
        .await
        .unwrap_err()
}

#[tokio::test]
async fn xsts_xerr_is_auth_xbox() {
    let e = xsts_err(
        401,
        r#"{"Identity":"0","XErr":2148916233,"Message":"SECRET"}"#,
    )
    .await;
    assert!(matches!(e, CoreError::AuthXbox { xerr: 2148916233 }));
    let p = e.to_payload();
    assert_eq!(p.code, "auth.xbox");
    assert_eq!(
        p.params.get("xerr"),
        Some(&nexora_core::error::ParamValue::Num(2148916233.0))
    );
    assert!(!format!("{p:?}").contains("SECRET"));
}

#[tokio::test]
async fn xsts_unknown_xerr_keeps_the_number() {
    let e = xsts_err(401, r#"{"XErr":4242424242}"#).await;
    assert!(matches!(e, CoreError::AuthXbox { xerr: 4242424242 }));
}

#[tokio::test]
async fn xsts_other_failures() {
    assert!(matches!(
        xsts_err(503, "x").await,
        CoreError::Offline { .. }
    ));
    match xsts_err(400, "SECRET").await {
        CoreError::HttpStatus { status: 400, url } => assert!(!url.contains("SECRET")),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        xsts_err(
            200,
            r#"{"Token":"t","NotAfter":"2030-01-01T00:00:00Z","DisplayClaims":{"xui":[]}}"#
        )
        .await,
        CoreError::Unexpected { .. }
    ));
}

// ---- Minecraft Services ----

use nexora_core::auth::accounts::EntitlementState;
use nexora_core::auth::minecraft::{
    McToken, entitlement, identity_token, login_with_xbox, profile,
};
use nexora_core::auth::xbox::XstsToken;
use wiremock::matchers::query_param_contains;

fn xsts_token() -> XstsToken {
    XstsToken {
        token: secret("XSTS"),
        uhs: "UHS".into(),
        xuid: None,
        expires_at_ms: 0,
    }
}

fn mc_token() -> McToken {
    McToken {
        access: secret("MCTOKEN"),
        expires_at_ms: 0,
    }
}

#[test]
fn identity_token_has_the_xbl3_format() {
    assert_eq!(
        identity_token(&xsts_token()).expose_secret(),
        "XBL3.0 x=UHS;XSTS"
    );
}

#[tokio::test]
async fn login_with_xbox_sends_the_identity_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/mc/login"))
        .and(body_string_contains(
            r#""identityToken":"XBL3.0 x=UHS;XSTS""#,
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"username":"u","access_token":"MC","token_type":"Bearer","expires_in":86400}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;
    let t = login_with_xbox(
        &AuthHttp::new().unwrap(),
        &auth_urls(&server),
        &xsts_token(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(t.access.expose_secret(), "MC");
    assert_eq!(t.expires_at_ms, NOW + 86_400_000);
}

#[tokio::test]
async fn login_with_xbox_clamps_a_hostile_expires_in() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/mc/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"access_token":"MC","expires_in":9223372036854775807}"#),
        )
        .mount(&server)
        .await;
    let t = login_with_xbox(
        &AuthHttp::new().unwrap(),
        &auth_urls(&server),
        &xsts_token(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(t.expires_at_ms, i64::MAX);
}

async fn login_403(body: &str) -> CoreError {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/mc/login"))
        .respond_with(ResponseTemplate::new(403).set_body_string(body.to_string()))
        .mount(&server)
        .await;
    login_with_xbox(
        &AuthHttp::new().unwrap(),
        &auth_urls(&server),
        &xsts_token(),
        NOW,
    )
    .await
    .unwrap_err()
}

#[tokio::test]
async fn invalid_app_registration_is_app_not_approved() {
    let err = login_403(
        r#"{"path":"/x","errorType":"Forbidden","errorMessage":"Invalid app registration, see https://aka.ms/AppRegInfo"}"#,
    )
    .await;
    assert!(matches!(err, CoreError::AppNotApproved), "{err:?}");
}

#[tokio::test]
async fn other_403_is_http_status() {
    let err = login_403("SECRET-BODY forbidden").await;
    match &err {
        CoreError::HttpStatus { status: 403, url } => assert!(url.ends_with("/mc/login")),
        other => panic!("{other:?}"),
    }
    assert!(!format!("{err:?}").contains("SECRET-BODY"));
}

#[tokio::test]
async fn login_server_error_is_offline() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/mc/login"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let err = login_with_xbox(
        &AuthHttp::new().unwrap(),
        &auth_urls(&server),
        &xsts_token(),
        NOW,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, CoreError::Offline { .. }), "{err:?}");
}

async fn mount_items(server: &MockServer, route: &str, names: &[&str]) {
    let items: Vec<String> = names
        .iter()
        .map(|n| format!(r#"{{"name":"{n}"}}"#))
        .collect();
    Mock::given(method("GET"))
        .and(path(route.to_string()))
        .and(header("Authorization", "Bearer MCTOKEN"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!(r#"{{"items":[{}]}}"#, items.join(","))),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn entitlement_is_verified_by_either_endpoint() {
    let server = MockServer::start().await;
    mount_items(&server, "/mc/store", &[]).await;
    mount_items(&server, "/mc/license", &["game_minecraft"]).await;
    let state = entitlement(&AuthHttp::new().unwrap(), &auth_urls(&server), &mc_token()).await;
    assert_eq!(state, EntitlementState::Verified);
}

#[tokio::test]
async fn bedrock_only_is_not_owned() {
    let server = MockServer::start().await;
    mount_items(&server, "/mc/store", &["product_minecraft_bedrock"]).await;
    mount_items(&server, "/mc/license", &["game_minecraft_bedrock"]).await;
    let state = entitlement(&AuthHttp::new().unwrap(), &auth_urls(&server), &mc_token()).await;
    assert_eq!(state, EntitlementState::NotOwned);
}

#[tokio::test]
async fn license_request_has_a_request_id() {
    let server = MockServer::start().await;
    mount_items(&server, "/mc/store", &[]).await;
    Mock::given(method("GET"))
        .and(path("/mc/license"))
        .and(query_param_contains("requestId", "-"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"items":[]}"#))
        .expect(1)
        .mount(&server)
        .await;
    entitlement(&AuthHttp::new().unwrap(), &auth_urls(&server), &mc_token()).await;
    let requests = server.received_requests().await.unwrap();
    let license = requests
        .iter()
        .find(|r| r.url.path() == "/mc/license")
        .unwrap();
    let id = license
        .url
        .query_pairs()
        .find(|(k, _)| k == "requestId")
        .unwrap()
        .1
        .to_string();
    let parsed = uuid::Uuid::parse_str(&id).unwrap();
    assert_eq!(parsed.get_version_num(), 4);
}

#[tokio::test]
async fn entitlement_server_error_is_unknown() {
    let server = MockServer::start().await;
    mount_items(&server, "/mc/store", &[]).await;
    Mock::given(method("GET"))
        .and(path("/mc/license"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let state = entitlement(&AuthHttp::new().unwrap(), &auth_urls(&server), &mc_token()).await;
    assert_eq!(state, EntitlementState::Unknown);
}

#[tokio::test]
async fn entitlement_failure_does_not_hide_a_java_item() {
    let server = MockServer::start().await;
    mount_items(&server, "/mc/store", &["product_minecraft"]).await;
    Mock::given(method("GET"))
        .and(path("/mc/license"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let state = entitlement(&AuthHttp::new().unwrap(), &auth_urls(&server), &mc_token()).await;
    assert_eq!(state, EntitlementState::Verified);
}

async fn mount_profile(server: &MockServer, status: u16, body: &str) {
    Mock::given(method("GET"))
        .and(path("/mc/profile"))
        .and(header("Authorization", "Bearer MCTOKEN"))
        .respond_with(ResponseTemplate::new(status).set_body_string(body.to_string()))
        .mount(server)
        .await;
}

#[tokio::test]
async fn profile_404_is_no_profile() {
    let server = MockServer::start().await;
    mount_profile(&server, 404, r#"{"error":"NOT_FOUND"}"#).await;
    let err = profile(&AuthHttp::new().unwrap(), &auth_urls(&server), &mc_token())
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::NoProfile), "{err:?}");
}

#[tokio::test]
async fn profile_reads_the_active_skin() {
    let server = MockServer::start().await;
    mount_profile(
        &server,
        200,
        r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","skins":[
            {"id":"1","state":"INACTIVE","url":"https://textures.minecraft.net/texture/old"},
            {"id":"2","state":"ACTIVE","url":"https://textures.minecraft.net/texture/new"}]}"#,
    )
    .await;
    let p = profile(&AuthHttp::new().unwrap(), &auth_urls(&server), &mc_token())
        .await
        .unwrap();
    assert_eq!(
        p.id.simple().to_string(),
        "069a79f444e94726a5befca90e38aaf5"
    );
    assert_eq!(p.name, "Notch");
    assert_eq!(
        p.skin_url.as_deref(),
        Some("https://textures.minecraft.net/texture/new")
    );
}

#[tokio::test]
async fn profile_without_an_active_skin_has_none() {
    let server = MockServer::start().await;
    mount_profile(
        &server,
        200,
        r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"N","skins":[
            {"state":"INACTIVE","url":"https://textures.minecraft.net/texture/old"}]}"#,
    )
    .await;
    let p = profile(&AuthHttp::new().unwrap(), &auth_urls(&server), &mc_token())
        .await
        .unwrap();
    assert_eq!(p.skin_url, None);
}

#[tokio::test]
async fn profile_ignores_skin_urls_off_textures_host() {
    for url in [
        "http://textures.minecraft.net/texture/a",
        "https://evil.example/texture/a",
        "https://textures.minecraft.net.evil.example/a",
        "not a url",
    ] {
        let server = MockServer::start().await;
        mount_profile(
            &server,
            200,
            &format!(
                r#"{{"id":"069a79f444e94726a5befca90e38aaf5","name":"N","skins":[{{"state":"ACTIVE","url":"{url}"}}]}}"#
            ),
        )
        .await;
        let p = profile(&AuthHttp::new().unwrap(), &auth_urls(&server), &mc_token())
            .await
            .unwrap();
        assert_eq!(p.skin_url, None, "{url}");
    }
}

#[tokio::test]
async fn a_reply_without_items_is_unknown_not_not_owned() {
    let server = MockServer::start().await;
    for route in ["/mc/store", "/mc/license"] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;
    }
    let state = entitlement(&AuthHttp::new().unwrap(), &auth_urls(&server), &mc_token()).await;
    assert_eq!(state, EntitlementState::Unknown);
}
