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

#[tokio::test]
async fn refresh_clamps_a_hostile_expires_in() {
    for (expires_in, expected) in [(i64::MAX, i64::MAX), (-5, NOW)] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                r#"{{"access_token":"A","expires_in":{expires_in}}}"#
            )))
            .mount(&server)
            .await;
        let tokens = refresh(
            &AuthHttp::new().unwrap(),
            &auth_urls(&server),
            "cid",
            &secret("r"),
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(tokens.expires_at_ms, expected);
    }
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
async fn profile_upgrades_an_http_textures_url_to_https() {
    // Minecraft Services はスキンの URL を http で返す
    let server = MockServer::start().await;
    mount_profile(
        &server,
        200,
        r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"N","skins":[
            {"state":"ACTIVE","url":"http://textures.minecraft.net/texture/a"}]}"#,
    )
    .await;
    let p = profile(&AuthHttp::new().unwrap(), &auth_urls(&server), &mc_token())
        .await
        .unwrap();
    assert_eq!(
        p.skin_url.as_deref(),
        Some("https://textures.minecraft.net/texture/a")
    );
}

#[tokio::test]
async fn profile_ignores_skin_urls_off_textures_host() {
    for url in [
        "http://textures.minecraft.net:8080/texture/a",
        "ftp://textures.minecraft.net/texture/a",
        "https://evil.example/texture/a",
        "http://evil.example/texture/a",
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

// ---- AuthService ----

use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicI64, Ordering};

use common::{
    ALL_SECRETS, MC_TOKEN, MS_ACCESS, Ownership, PROFILE_ID, PROFILE_NAME, REFRESH_1, REFRESH_2,
    REFRESH_3, XBL_TOKEN, XUID, hits_on, mount_chain, mount_entitlements, mount_mc_login,
    mount_mc_profile, mount_refresh, mount_token_failure, mount_xbox,
};
use nexora_core::auth::OFFLINE_GRACE_MS;
use nexora_core::auth::accounts::{self, ProfileRecord};
use nexora_core::auth::login::LoginProgress;
use nexora_core::auth::microsoft::MsTokens;
use nexora_core::auth::service::{AuthService, offline_allowed};
use nexora_core::auth::store::{MemoryStore, SecretStore};
use nexora_core::db::Db;
use nexora_core::launch::session::UserType;
use nexora_core::paths::AppPaths;
use uuid::Uuid;

const T0: i64 = 1_800_000_000_000;

struct Env {
    server: MockServer,
    db: Db,
    store: Arc<MemoryStore>,
    clock: Arc<AtomicI64>,
    temp: tempfile::TempDir,
    auth: AuthService,
}

fn profile_id() -> Uuid {
    Uuid::parse_str(PROFILE_ID).unwrap()
}

fn service(
    server: &MockServer,
    db: &Db,
    root: &std::path::Path,
    store: Option<Arc<dyn SecretStore>>,
    clock: &Arc<AtomicI64>,
) -> AuthService {
    let clock = clock.clone();
    AuthService::new(
        db.clone(),
        AppPaths::from_root(root),
        AuthHttp::new().unwrap(),
        auth_urls(server),
        store,
        Some("cid".into()),
    )
    .with_clock(Arc::new(move || clock.load(Ordering::SeqCst)))
}

async fn env_with(store: bool) -> Env {
    let server = MockServer::start().await;
    let db = Db::open_in_memory().unwrap();
    let mem = Arc::new(MemoryStore::default());
    let clock = Arc::new(AtomicI64::new(T0));
    let temp = tempfile::tempdir().unwrap();
    let dyn_store = store.then(|| mem.clone() as Arc<dyn SecretStore>);
    let auth = service(&server, &db, temp.path(), dyn_store, &clock);
    Env {
        server,
        db,
        store: mem,
        clock,
        temp,
        auth,
    }
}

async fn env() -> Env {
    env_with(true).await
}

impl Env {
    /// 使用中のアカウントを 1 件入れ、store に REFRESH_1 を置く。
    fn seed(&self, entitlement: EntitlementState, at: i64) -> Uuid {
        let id = profile_id();
        accounts::save_profile(
            &self.db,
            &ProfileRecord {
                id,
                username: PROFILE_NAME.into(),
                xuid: Some(XUID.into()),
                skin_url: None,
            },
            entitlement,
            at,
        )
        .unwrap();
        accounts::set_active(&self.db, id).unwrap();
        self.store
            .set(&id.simple().to_string(), &secret(REFRESH_1))
            .unwrap();
        id
    }

    fn stored(&self) -> Option<String> {
        self.store.value(PROFILE_ID)
    }

    fn row(&self) -> accounts::AccountRow {
        accounts::get(&self.db, profile_id()).unwrap()
    }

    fn advance(&self, ms: i64) {
        self.clock.fetch_add(ms, Ordering::SeqCst);
    }

    fn now(&self) -> i64 {
        self.clock.load(Ordering::SeqCst)
    }

    async fn login(&self) -> Result<Uuid, CoreError> {
        let tokens = MsTokens {
            access: secret(MS_ACCESS),
            refresh: Some(secret(REFRESH_1)),
            expires_at_ms: T0 + 3_600_000,
        };
        self.auth
            .complete_login(tokens, &|_| {}, &CancellationToken::new())
            .await
    }

    async fn requests(&self) -> usize {
        self.server.received_requests().await.unwrap().len()
    }
}

fn row_for_unit() -> accounts::AccountRow {
    accounts::AccountRow {
        id: profile_id(),
        username: "A".into(),
        xuid: None,
        skin_url: None,
        is_active: true,
        added_at: 0,
        profile_fetched_at: Some(0),
        entitlement: EntitlementState::Verified,
        entitlement_checked_at: Some(0),
        verified_at: Some(0),
        needs_login: false,
    }
}

#[test]
fn service_offline_allowed_within_30_days_of_verification() {
    let mut row = row_for_unit();
    row.verified_at = Some(0);
    assert!(offline_allowed(&row, OFFLINE_GRACE_MS));
    assert!(!offline_allowed(&row, OFFLINE_GRACE_MS + 1));
}

#[test]
fn service_offline_not_allowed_when_not_owned_or_never_verified() {
    let mut row = row_for_unit();
    row.entitlement = EntitlementState::NotOwned;
    assert!(!offline_allowed(&row, 1));
    let mut row = row_for_unit();
    row.entitlement = EntitlementState::Unknown;
    row.verified_at = None;
    assert!(!offline_allowed(&row, 1));
}

#[test]
fn service_offline_allowed_when_latest_check_is_unknown_but_recently_verified() {
    let mut row = row_for_unit();
    row.entitlement = EntitlementState::Unknown;
    row.verified_at = Some(0);
    assert!(offline_allowed(&row, 1));
}

#[test]
fn service_offline_not_allowed_when_login_is_needed() {
    let mut row = row_for_unit();
    row.needs_login = true;
    assert!(!offline_allowed(&row, 1));
}

#[tokio::test]
async fn service_complete_login_saves_the_account_and_refresh_token() {
    let e = env().await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let seen = StdMutex::new(Vec::new());
    let tokens = MsTokens {
        access: secret(MS_ACCESS),
        refresh: Some(secret(REFRESH_1)),
        expires_at_ms: T0 + 3_600_000,
    };
    let id = e
        .auth
        .complete_login(
            tokens,
            &|p| seen.lock().unwrap().push(p),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(id, profile_id());
    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            LoginProgress::CheckingXbox,
            LoginProgress::CheckingMinecraft
        ]
    );
    let rows = accounts::list(&e.db).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].is_active);
    assert_eq!(rows[0].entitlement, EntitlementState::Verified);
    assert_eq!(rows[0].verified_at, Some(T0));
    assert_eq!(rows[0].xuid.as_deref(), Some(XUID));
    assert_eq!(e.stored().as_deref(), Some(REFRESH_1));
    let list = e.auth.list().unwrap();
    assert_eq!(list.accounts.len(), 1);
    let a = &list.accounts[0];
    assert_eq!(a.id, PROFILE_ID);
    assert_eq!(a.username, PROFILE_NAME);
    assert!(a.is_active && a.login_saved && !a.needs_login && !a.has_skin);
}

#[tokio::test]
async fn service_complete_login_not_owned_saves_nothing() {
    let e = env().await;
    mount_chain(&e.server, 86_400, Ownership::NotOwned).await;
    let err = e.login().await.unwrap_err();
    assert_eq!(err.code(), "auth.not_owned");
    assert!(accounts::list(&e.db).unwrap().is_empty());
    assert_eq!(e.stored(), None);
    assert!(matches!(
        e.auth.session_for_launch().await.unwrap_err(),
        CoreError::AuthNoAccount
    ));
}

#[tokio::test]
async fn service_complete_login_unknown_ownership_is_saved_as_unknown() {
    let e = env().await;
    mount_chain(&e.server, 86_400, Ownership::Unknown).await;
    e.login().await.unwrap();
    let row = e.row();
    assert_eq!(row.entitlement, EntitlementState::Unknown);
    assert_eq!(row.verified_at, None);
    assert!(row.is_active);
    assert_eq!(e.stored().as_deref(), Some(REFRESH_1));
}

#[tokio::test]
async fn service_complete_login_offline_is_auth_offline() {
    let e = env().await;
    Mock::given(method("POST"))
        .and(path("/xbox/user"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&e.server)
        .await;
    let err = e.login().await.unwrap_err();
    assert!(
        matches!(
            err,
            CoreError::AuthOffline {
                offline_available: false
            }
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn service_session_reuses_a_fresh_minecraft_token() {
    let e = env().await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    e.login().await.unwrap();
    let before = e.requests().await;
    let s1 = e.auth.session_for_launch().await.unwrap();
    // 期限まで余裕（5 分）がちょうど残っている間は、取り直さない
    e.advance(86_400_000 - 300_000);
    let s2 = e.auth.session_for_launch().await.unwrap();
    assert_eq!(e.requests().await, before);
    for s in [&s1, &s2] {
        assert_eq!(s.uuid, profile_id());
        assert_eq!(s.username, PROFILE_NAME);
        assert_eq!(s.access_token.expose_secret(), MC_TOKEN);
        assert_eq!(s.xuid.as_deref(), Some(XUID));
        assert_eq!(s.user_type, UserType::Msa);
    }
}

#[tokio::test]
async fn service_session_refreshes_within_the_margin() {
    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    mount_refresh(&e.server, None, None).await;
    mount_chain(&e.server, 240, Ownership::Owned).await;
    e.auth.session_for_launch().await.unwrap();
    assert_eq!(hits_on(&e.server, "/token").await, 1);
    e.auth.session_for_launch().await.unwrap();
    assert_eq!(hits_on(&e.server, "/token").await, 2);
}

#[tokio::test]
async fn service_session_refreshes_once_the_margin_is_reached() {
    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    mount_refresh(&e.server, None, None).await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    e.auth.session_for_launch().await.unwrap();
    e.advance(86_400_000 - 300_000 + 1);
    e.auth.session_for_launch().await.unwrap();
    assert_eq!(hits_on(&e.server, "/token").await, 2);
}

#[tokio::test]
async fn service_concurrent_sessions_refresh_once() {
    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"access_token":"{MS_ACCESS}","expires_in":3600}}"#
        )))
        .expect(1)
        .mount(&e.server)
        .await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let (a, b) = tokio::join!(e.auth.session_for_launch(), e.auth.session_for_launch());
    assert_eq!(a.unwrap().access_token.expose_secret(), MC_TOKEN);
    assert_eq!(b.unwrap().access_token.expose_secret(), MC_TOKEN);
    e.server.verify().await;
}

#[tokio::test]
async fn service_rotated_refresh_token_replaces_the_old_one() {
    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    mount_refresh(&e.server, Some(REFRESH_1), Some(REFRESH_2)).await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    e.auth.session_for_launch().await.unwrap();
    assert_eq!(e.stored().as_deref(), Some(REFRESH_2));
    assert!(e.auth.list().unwrap().accounts[0].login_saved);
}

#[tokio::test]
async fn service_failed_store_write_keeps_the_old_token_and_marks_unsaved() {
    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    mount_refresh(&e.server, Some(REFRESH_1), Some(REFRESH_2)).await;
    mount_refresh(&e.server, Some(REFRESH_2), Some(REFRESH_3)).await;
    // 余裕の 5 分より短い期限にして、毎回取り直させる
    mount_chain(&e.server, 240, Ownership::Owned).await;
    e.store.fail_writes(true);
    e.auth.session_for_launch().await.unwrap();
    assert_eq!(e.stored().as_deref(), Some(REFRESH_1));
    assert!(!e.auth.list().unwrap().accounts[0].login_saved);
    // 2 回目は、メモリの REFRESH_2 で refresh する
    e.auth.session_for_launch().await.unwrap();
    let bodies: Vec<String> = e
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/token")
        .map(|r| String::from_utf8_lossy(&r.body).into_owned())
        .collect();
    assert_eq!(bodies.len(), 2);
    assert!(bodies[1].contains(&format!("refresh_token={REFRESH_2}")));
    assert_eq!(e.stored().as_deref(), Some(REFRESH_1));
    // 保存できるようになれば、回転しなくても、メモリの refresh token を保存し直す
    mount_refresh(&e.server, Some(REFRESH_3), None).await;
    e.store.fail_writes(false);
    e.auth.session_for_launch().await.unwrap();
    assert_eq!(e.stored().as_deref(), Some(REFRESH_3));
    assert!(e.auth.list().unwrap().accounts[0].login_saved);
}

#[tokio::test]
async fn service_invalid_grant_sets_needs_login_and_keeps_the_token() {
    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    mount_token_failure(&e.server, 400, r#"{"error":"invalid_grant"}"#).await;
    let err = e.auth.session_for_launch().await.unwrap_err();
    let p = err.to_payload();
    assert_eq!(p.code, "auth.login_required");
    assert_eq!(
        p.params.get("accountId"),
        Some(&nexora_core::error::ParamValue::Str(PROFILE_ID.into()))
    );
    assert!(e.row().needs_login);
    assert_eq!(e.stored().as_deref(), Some(REFRESH_1));
    assert!(e.auth.list().unwrap().accounts[0].needs_login);
}

#[tokio::test]
async fn service_server_errors_are_offline_and_do_not_set_needs_login() {
    for (verified_at, expected) in [(T0, true), (T0 - OFFLINE_GRACE_MS - 1, false)] {
        let e = env().await;
        e.seed(EntitlementState::Verified, verified_at);
        mount_token_failure(&e.server, 503, "").await;
        let err = e.auth.session_for_launch().await.unwrap_err();
        let row = e.row();
        assert_eq!(offline_allowed(&row, e.now()), expected);
        match err {
            CoreError::AuthOffline { offline_available } => {
                assert_eq!(offline_available, expected)
            }
            other => panic!("{other:?}"),
        }
        assert!(!row.needs_login);
        assert_eq!(e.stored().as_deref(), Some(REFRESH_1));
    }
}

#[tokio::test]
async fn service_missing_refresh_token_is_login_required() {
    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    e.store.delete(PROFILE_ID).unwrap();
    let err = e.auth.session_for_launch().await.unwrap_err();
    assert!(
        matches!(&err, CoreError::LoginRequired { account_id } if account_id == PROFILE_ID),
        "{err:?}"
    );
    assert!(e.row().needs_login);
    assert_eq!(e.requests().await, 0);
}

#[tokio::test]
async fn service_failed_store_read_is_offline_and_does_not_set_needs_login() {
    use std::sync::atomic::AtomicUsize;
    for (verified_at, expected) in [(T0, true), (T0 - OFFLINE_GRACE_MS - 1, false)] {
        let e = env().await;
        e.seed(EntitlementState::Verified, verified_at);
        e.store.fail_reads(true);
        let changes = Arc::new(AtomicUsize::new(0));
        let c = changes.clone();
        let auth = e.auth.clone().with_on_change(Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        let err = auth.session_for_launch().await.unwrap_err();
        match err {
            CoreError::AuthOffline { offline_available } => {
                assert_eq!(offline_available, expected)
            }
            other => panic!("{other:?}"),
        }
        // 起動時の warm_up も、静かに終わる
        auth.warm_up().await;
        assert!(!e.row().needs_login);
        assert_eq!(changes.load(Ordering::SeqCst), 0);
        assert_eq!(e.requests().await, 0);
        assert_eq!(e.stored().as_deref(), Some(REFRESH_1));
        // 読めるようになれば、そのまま refresh できる
        e.store.fail_reads(false);
        mount_refresh(&e.server, None, None).await;
        mount_chain(&e.server, 86_400, Ownership::Owned).await;
        auth.session_for_launch().await.unwrap();
    }
}

#[tokio::test]
async fn service_minecraft_401_retries_once() {
    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    mount_refresh(&e.server, None, None).await;
    mount_xbox(&e.server).await;
    mount_mc_login(&e.server, 86_400).await;
    mount_entitlements(&e.server, Ownership::Owned).await;
    Mock::given(method("GET"))
        .and(path("/mc/profile"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&e.server)
        .await;
    let err = e.auth.session_for_launch().await.unwrap_err();
    assert_eq!(err.code(), "auth.login_required");
    assert_eq!(hits_on(&e.server, "/token").await, 2);
    assert_eq!(hits_on(&e.server, "/mc/profile").await, 2);
    assert!(e.row().needs_login);
}

#[tokio::test]
async fn service_minecraft_401_then_success_launches() {
    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    mount_refresh(&e.server, None, None).await;
    mount_xbox(&e.server).await;
    mount_mc_login(&e.server, 86_400).await;
    mount_entitlements(&e.server, Ownership::Owned).await;
    Mock::given(method("GET"))
        .and(path("/mc/profile"))
        .respond_with(ResponseTemplate::new(401))
        .up_to_n_times(1)
        .mount(&e.server)
        .await;
    mount_mc_profile(&e.server).await;
    e.auth.session_for_launch().await.unwrap();
    assert_eq!(hits_on(&e.server, "/token").await, 2);
    assert!(!e.row().needs_login);
}

#[tokio::test]
async fn service_no_store_means_session_only() {
    let e = env_with(false).await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    e.login().await.unwrap();
    assert!(!e.auth.list().unwrap().accounts[0].login_saved);
    e.auth.session_for_launch().await.unwrap();
    // 再起動の代わりに、新しい AuthService を作る
    let restarted = service(&e.server, &e.db, e.temp.path(), None, &e.clock);
    let err = restarted.session_for_launch().await.unwrap_err();
    assert_eq!(err.code(), "auth.login_required");
    assert!(e.row().needs_login);
    assert!(restarted.list().unwrap().accounts[0].needs_login);
}

#[tokio::test]
async fn service_launch_blocks_not_owned_and_never_verified_unknown() {
    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    mount_refresh(&e.server, None, None).await;
    mount_chain(&e.server, 86_400, Ownership::NotOwned).await;
    let err = e.auth.session_for_launch().await.unwrap_err();
    assert_eq!(err.code(), "auth.not_owned");
    assert_eq!(e.row().entitlement, EntitlementState::NotOwned);

    let e = env().await;
    e.seed(EntitlementState::Unknown, T0);
    mount_refresh(&e.server, None, None).await;
    mount_chain(&e.server, 86_400, Ownership::Unknown).await;
    let err = e.auth.session_for_launch().await.unwrap_err();
    assert_eq!(err.code(), "auth.ownership_unknown");
}

#[tokio::test]
async fn service_launch_allows_unknown_when_verified_before() {
    let e = env().await;
    e.seed(EntitlementState::Verified, T0 - 1000);
    mount_refresh(&e.server, None, None).await;
    mount_chain(&e.server, 86_400, Ownership::Unknown).await;
    e.auth.session_for_launch().await.unwrap();
    let row = e.row();
    assert_eq!(row.entitlement, EntitlementState::Unknown);
    assert_eq!(row.verified_at, Some(T0 - 1000));
}

#[tokio::test]
async fn service_remove_deletes_row_token_and_cache() {
    let e = env().await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let id = e.login().await.unwrap();
    e.auth.remove(id).await.unwrap();
    assert!(accounts::list(&e.db).unwrap().is_empty());
    assert_eq!(e.stored(), None);
    assert!(e.auth.list().unwrap().accounts.is_empty());
    // 行を戻しても、メモリの Minecraft のトークンは使わない（store も空なのでログインが必要）
    accounts::save_profile(
        &e.db,
        &ProfileRecord {
            id,
            username: PROFILE_NAME.into(),
            xuid: None,
            skin_url: None,
        },
        EntitlementState::Verified,
        T0,
    )
    .unwrap();
    accounts::set_active(&e.db, id).unwrap();
    let err = e.auth.session_for_launch().await.unwrap_err();
    assert_eq!(err.code(), "auth.login_required");
    let missing = e.auth.remove(Uuid::from_u128(7)).await.unwrap_err();
    assert_eq!(missing.code(), "auth.account_not_found");
}

#[tokio::test]
async fn service_no_active_account_is_auth_no_account() {
    let e = env().await;
    assert!(matches!(
        e.auth.session_for_launch().await.unwrap_err(),
        CoreError::AuthNoAccount
    ));
    assert!(matches!(
        e.auth.offline_session().unwrap_err(),
        CoreError::AuthNoAccount
    ));
}

#[tokio::test]
async fn service_offline_session_uses_cached_identity() {
    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    let s = e.auth.offline_session().unwrap();
    assert_eq!(s.uuid, profile_id());
    assert_eq!(s.username, PROFILE_NAME);
    assert_eq!(s.xuid.as_deref(), Some(XUID));
    assert_eq!(s.access_token.expose_secret(), "0");
    assert_eq!(s.user_type, UserType::Msa);
    assert_eq!(e.requests().await, 0);
    e.advance(OFFLINE_GRACE_MS + 1);
    assert!(matches!(
        e.auth.offline_session().unwrap_err(),
        CoreError::AuthOffline {
            offline_available: false
        }
    ));
}

#[tokio::test]
async fn service_set_active_switches_accounts() {
    let e = env().await;
    let a = e.seed(EntitlementState::Verified, T0);
    let b = Uuid::from_u128(2);
    accounts::save_profile(
        &e.db,
        &ProfileRecord {
            id: b,
            username: "Alex".into(),
            xuid: None,
            skin_url: Some("https://textures.minecraft.net/texture/x".into()),
        },
        EntitlementState::Verified,
        T0 + 1,
    )
    .unwrap();
    e.auth.set_active(b).unwrap();
    let list = e.auth.list().unwrap();
    let active: Vec<_> = list.accounts.iter().filter(|x| x.is_active).collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, b.simple().to_string());
    assert!(active[0].has_skin);
    assert!(e.auth.skin_path(a).await.unwrap().is_none());
    let missing = e.auth.set_active(Uuid::from_u128(9)).unwrap_err();
    assert_eq!(missing.code(), "auth.account_not_found");
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn service_dev_offline_account_takes_priority() {
    use nexora_core::launch::session::set_dev_offline_active;
    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    set_dev_offline_active(&e.db, true, T0).unwrap();
    let s = e.auth.session_for_launch().await.unwrap();
    assert_eq!(s.user_type, UserType::Offline);
    assert_eq!(s.username, "Player");
    let o = e.auth.offline_session().unwrap();
    assert_eq!(o.user_type, UserType::Offline);
    assert_eq!(e.requests().await, 0);
    let list = e.auth.list().unwrap();
    let dev = list.dev_offline.unwrap();
    assert!(dev.active);
    assert_eq!(dev.username, "Player");
    assert!(!list.accounts[0].is_active);
    e.auth.warm_up().await;
    assert_eq!(e.requests().await, 0);
}

#[tokio::test]
async fn service_warm_up_marks_needs_login_without_failing() {
    let e = env().await;
    e.auth.warm_up().await;
    e.seed(EntitlementState::Verified, T0);
    mount_token_failure(&e.server, 400, r#"{"error":"invalid_grant"}"#).await;
    e.auth.warm_up().await;
    assert!(e.row().needs_login);
}

#[tokio::test]
async fn service_errors_do_not_contain_tokens() {
    let mut errors = Vec::new();

    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    mount_token_failure(&e.server, 400, r#"{"error":"invalid_grant"}"#).await;
    errors.push(e.auth.session_for_launch().await.unwrap_err());

    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    mount_token_failure(&e.server, 503, "").await;
    errors.push(e.auth.session_for_launch().await.unwrap_err());

    let e = env().await;
    e.seed(EntitlementState::Verified, T0);
    mount_refresh(&e.server, None, Some(REFRESH_2)).await;
    mount_xbox(&e.server).await;
    mount_mc_login(&e.server, 86_400).await;
    mount_entitlements(&e.server, Ownership::Owned).await;
    Mock::given(method("GET"))
        .and(path("/mc/profile"))
        .respond_with(ResponseTemplate::new(401).set_body_string(MC_TOKEN))
        .mount(&e.server)
        .await;
    errors.push(e.auth.session_for_launch().await.unwrap_err());

    let e = env().await;
    mount_chain(&e.server, 86_400, Ownership::NotOwned).await;
    errors.push(e.login().await.unwrap_err());

    let e = env().await;
    e.seed(EntitlementState::Unknown, T0);
    mount_refresh(&e.server, None, Some(REFRESH_2)).await;
    mount_chain(&e.server, 86_400, Ownership::Unknown).await;
    errors.push(e.auth.session_for_launch().await.unwrap_err());

    assert_eq!(errors.len(), 5);
    for err in &errors {
        let json = serde_json::to_string(&err.to_payload()).unwrap();
        let debug = format!("{err:?}");
        for s in ALL_SECRETS {
            assert!(!json.contains(s), "{json} contains {s}");
            assert!(!debug.contains(s), "{debug} contains {s}");
        }
    }
}

// ---- ログインのタスク ----

use nexora_core::auth::login::{
    LOGIN_KIND, LoginMethod, LoginNotifier, LoginProgressEvent, UrlOpener, run_login,
};
use nexora_core::task::{TaskChange, TaskManager, TaskNotifier, TaskState};

const AUTH_CODE: &str = "AUTH-CODE-SECRET";

struct NoTasks;

impl TaskNotifier for NoTasks {
    fn notify(&self, _: &str, _: u64, _: TaskChange) {}
}

/// 受け取った進捗を記録する。
#[derive(Default)]
struct Progress(StdMutex<Vec<LoginProgressEvent>>);

impl LoginNotifier for Progress {
    fn progress(&self, event: &LoginProgressEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

impl Progress {
    fn stages(&self) -> Vec<LoginProgress> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.progress.clone())
            .collect()
    }

    fn browser_url(&self) -> Option<String> {
        self.stages().into_iter().find_map(|p| match p {
            LoginProgress::WaitingForBrowser { url } => Some(url),
            _ => None,
        })
    }
}

/// 認可 URL の state と redirect_uri のポート。
fn callback_target(url: &str) -> (String, u16) {
    let parsed = reqwest::Url::parse(url).unwrap();
    let q: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
    let redirect = reqwest::Url::parse(&q["redirect_uri"]).unwrap();
    assert_eq!(redirect.host_str(), Some("localhost"));
    (q["state"].clone(), redirect.port().unwrap())
}

/// ブラウザの代わりに、ループバックへ `/?code=...&state=...` を送る（別のスレッドで）。
fn send_callback(url: &str) {
    use std::io::{Read, Write};
    let (state, port) = callback_target(url);
    std::thread::spawn(move || {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            s,
            "GET /?code={AUTH_CODE}&state={state} HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n"
        )
        .unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
    });
}

/// 開いた URL を記録し、ブラウザの代わりにコールバックを送る。
#[derive(Default)]
struct FakeBrowser(StdMutex<Vec<String>>);

impl UrlOpener for FakeBrowser {
    fn open(&self, url: &str) -> Result<(), String> {
        self.0.lock().unwrap().push(url.to_string());
        send_callback(url);
        Ok(())
    }
}

/// 開けないブラウザ。呼ばれた回数だけ数える。
#[derive(Default)]
struct BrokenOpener(StdMutex<usize>);

impl UrlOpener for BrokenOpener {
    fn open(&self, _: &str) -> Result<(), String> {
        *self.0.lock().unwrap() += 1;
        Err("no browser".into())
    }
}

/// 認可コードの交換に答える。
async fn mount_code_exchange(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains(format!("code={AUTH_CODE}")))
        .and(body_string_contains("code_verifier="))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"access_token":"{MS_ACCESS}","refresh_token":"{REFRESH_2}","expires_in":3600}}"#
        )))
        .mount(server)
        .await;
}

/// デバイスコードの要求とポーリングに答える。
async fn mount_device(server: &MockServer, verification_uri: &str) {
    Mock::given(method("POST"))
        .and(path("/devicecode"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"device_code":"DC","user_code":"ABCD-EFGH","verification_uri":"{verification_uri}","expires_in":900,"interval":0}}"#
        )))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("device_code=DC"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"access_token":"{MS_ACCESS}","refresh_token":"{REFRESH_2}","expires_in":3600}}"#
        )))
        .mount(server)
        .await;
}

/// run_login の結果。エラーはコードで持つ（CoreError は Clone できないため、本体はタスクに返す）。
type LoginResult = Result<Uuid, &'static str>;

/// TaskManager の中で動かしているログイン。
struct LoginRun {
    tasks: TaskManager,
    id: String,
    rx: tokio::sync::oneshot::Receiver<LoginResult>,
}

/// TaskManager の中で run_login を動かす。タスクには run_login の結果をそのまま返す。
fn start_login(
    auth: AuthService,
    method: LoginMethod,
    opener: Arc<dyn UrlOpener>,
    notifier: Arc<Progress>,
) -> LoginRun {
    let tasks = TaskManager::new(Arc::new(NoTasks));
    let (tx, rx) = tokio::sync::oneshot::channel();
    let id = tasks.spawn_unique(LOGIN_KIND, None, move |handle| async move {
        let r = run_login(&auth, method, opener.as_ref(), notifier.as_ref(), &handle).await;
        let _ = tx.send(r.as_ref().map(|id| *id).map_err(|e| e.code()));
        r.map(|_| ())
    });
    LoginRun { tasks, id, rx }
}

impl LoginRun {
    /// 終わるまで待ち、結果とタスクの最後の状態を返す。両者が食い違わないことも確かめる。
    async fn finish(self) -> (LoginResult, TaskState) {
        let r = tokio::time::timeout(std::time::Duration::from_secs(20), self.rx)
            .await
            .expect("ログインが終わらない")
            .unwrap();
        let mut snap = self.tasks.get(&self.id).unwrap();
        for _ in 0..500 {
            if snap.state.is_terminal() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            snap = self.tasks.get(&self.id).unwrap();
        }
        match r {
            Ok(_) => assert_eq!(snap.state, TaskState::Succeeded),
            Err("task.cancelled") => assert_eq!(snap.state, TaskState::Cancelled),
            Err(code) => {
                assert_eq!(snap.state, TaskState::Failed);
                assert_eq!(snap.error.as_ref().map(|p| p.code.as_str()), Some(code));
            }
        }
        (r, snap.state)
    }
}

async fn login_with(
    auth: &AuthService,
    method: LoginMethod,
    opener: Arc<dyn UrlOpener>,
) -> (LoginResult, Arc<Progress>, String) {
    let progress = Arc::new(Progress::default());
    let run = start_login(auth.clone(), method, opener, progress.clone());
    let id = run.id.clone();
    let (r, _) = run.finish().await;
    (r, progress, id)
}

async fn wait_for_url(progress: &Progress) -> String {
    for _ in 0..500 {
        if let Some(url) = progress.browser_url() {
            return url;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("WaitingForBrowser が来ない");
}

fn assert_no_secrets(progress: &Progress) {
    let events = serde_json::to_string(&*progress.0.lock().unwrap()).unwrap();
    for s in ALL_SECRETS.iter().chain([&AUTH_CODE]) {
        assert!(!events.contains(s), "{events} contains {s}");
    }
}

#[tokio::test]
async fn login_browser_adds_the_account() {
    let e = env().await;
    mount_code_exchange(&e.server).await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let browser = Arc::new(FakeBrowser::default());
    let (r, progress, task_id) = login_with(&e.auth, LoginMethod::Browser, browser.clone()).await;
    assert_eq!(r.unwrap(), profile_id());
    let stages = progress.stages();
    assert_eq!(stages.len(), 3, "{stages:?}");
    let LoginProgress::WaitingForBrowser { url } = &stages[0] else {
        panic!("{stages:?}");
    };
    assert_eq!(
        &stages[1..],
        &[
            LoginProgress::CheckingXbox,
            LoginProgress::CheckingMinecraft
        ]
    );
    assert!(url.starts_with(&format!("{}/authorize?", e.server.uri())));
    assert_eq!(*browser.0.lock().unwrap(), vec![url.clone()]);
    assert!(
        progress
            .0
            .lock()
            .unwrap()
            .iter()
            .all(|ev| ev.task_id == task_id)
    );
    assert!(e.row().is_active);
    assert_eq!(e.auth.list().unwrap().accounts[0].id, PROFILE_ID);
    assert_eq!(e.stored().as_deref(), Some(REFRESH_2));
    assert_no_secrets(&progress);
}

#[tokio::test]
async fn login_browser_without_client_id_is_no_client_id() {
    let e = env().await;
    let auth = AuthService::new(
        e.db.clone(),
        AppPaths::from_root(e.temp.path()),
        AuthHttp::new().unwrap(),
        auth_urls(&e.server),
        None,
        None,
    );
    let browser = Arc::new(FakeBrowser::default());
    for method in [LoginMethod::Browser, LoginMethod::DeviceCode] {
        let (r, progress, _) = login_with(&auth, method, browser.clone()).await;
        assert_eq!(r, Err("auth.no_client_id"));
        assert!(progress.stages().is_empty());
    }
    assert!(browser.0.lock().unwrap().is_empty());
    assert_eq!(e.requests().await, 0);
}

#[tokio::test]
async fn login_opener_failure_still_waits_and_reports_the_url() {
    let e = env().await;
    mount_code_exchange(&e.server).await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let progress = Arc::new(Progress::default());
    let opener = Arc::new(BrokenOpener::default());
    let run = start_login(
        e.auth.clone(),
        LoginMethod::Browser,
        opener.clone(),
        progress.clone(),
    );
    let url = wait_for_url(&progress).await;
    // 開けなくても待ち続ける。UI から開き直した、とみなしてコールバックを送る
    send_callback(&url);
    let (r, state) = run.finish().await;
    assert_eq!(r, Ok(profile_id()));
    assert_eq!(state, TaskState::Succeeded);
    assert_eq!(*opener.0.lock().unwrap(), 1);
}

#[tokio::test]
async fn login_cancel_during_browser_wait_is_cancelled() {
    let e = env().await;
    let progress = Arc::new(Progress::default());
    let run = start_login(
        e.auth.clone(),
        LoginMethod::Browser,
        Arc::new(BrokenOpener::default()),
        progress.clone(),
    );
    let url = wait_for_url(&progress).await;
    let (_, port) = callback_target(&url);
    run.tasks.cancel(&run.id).unwrap();
    let (r, state) = run.finish().await;
    assert_eq!(r, Err("task.cancelled"));
    assert_eq!(state, TaskState::Cancelled);
    // ポートが解放されている
    std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
    assert!(accounts::list(&e.db).unwrap().is_empty());
    assert_eq!(e.requests().await, 0);
}

#[tokio::test]
async fn login_cancel_during_the_xbox_check_saves_nothing() {
    let e = env().await;
    // 元のアカウント（別の UUID）が使用中。キャンセルしたら切り替わってはいけない
    let other = Uuid::from_u128(2);
    accounts::save_profile(
        &e.db,
        &ProfileRecord {
            id: other,
            username: "Alex".into(),
            xuid: None,
            skin_url: None,
        },
        EntitlementState::Verified,
        T0,
    )
    .unwrap();
    accounts::set_active(&e.db, other).unwrap();
    mount_code_exchange(&e.server).await;
    // Xbox のユーザートークンだけ遅らせる。キャンセルが効かなければ、3 秒後に連鎖が成功する
    Mock::given(method("POST"))
        .and(path("/xbox/user"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!(
                    r#"{{"Token":"{XBL_TOKEN}","NotAfter":"2100-01-01T00:00:00Z"}}"#
                ))
                .set_delay(std::time::Duration::from_secs(3)),
        )
        .with_priority(1)
        .mount(&e.server)
        .await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let progress = Arc::new(Progress::default());
    let run = start_login(
        e.auth.clone(),
        LoginMethod::Browser,
        Arc::new(FakeBrowser::default()),
        progress.clone(),
    );
    for _ in 0..500 {
        if progress.stages().contains(&LoginProgress::CheckingXbox) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(progress.stages().contains(&LoginProgress::CheckingXbox));
    run.tasks.cancel(&run.id).unwrap();
    let (r, state) = run.finish().await;
    assert_eq!(r, Err("task.cancelled"));
    assert_eq!(state, TaskState::Cancelled);
    let rows = accounts::list(&e.db).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, other);
    assert!(rows[0].is_active);
    assert_eq!(e.stored(), None);
    assert!(
        !progress
            .stages()
            .contains(&LoginProgress::CheckingMinecraft)
    );
}

#[tokio::test]
async fn login_device_code_reports_the_user_code() {
    let e = env().await;
    mount_device(&e.server, "https://www.microsoft.com/link").await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let browser = Arc::new(FakeBrowser::default());
    let (r, progress, _) = login_with(&e.auth, LoginMethod::DeviceCode, browser.clone()).await;
    assert_eq!(r.unwrap(), profile_id());
    assert_eq!(
        progress.stages(),
        vec![
            LoginProgress::DeviceCode {
                user_code: "ABCD-EFGH".into(),
                verification_uri: "https://www.microsoft.com/link".into(),
            },
            LoginProgress::CheckingXbox,
            LoginProgress::CheckingMinecraft,
        ]
    );
    // デバイスコードでは何も開かない
    assert!(browser.0.lock().unwrap().is_empty());
    assert!(e.row().is_active);
    assert_eq!(e.stored().as_deref(), Some(REFRESH_2));
    let events = serde_json::to_string(&*progress.0.lock().unwrap()).unwrap();
    assert!(
        events.contains(r#""stage":"deviceCode","userCode":"ABCD-EFGH","verificationUri":"#),
        "{events}"
    );
    assert!(events.contains(r#""taskId":"#), "{events}");
    assert!(!events.contains("\"DC\""), "{events}");
    assert_no_secrets(&progress);
}

#[tokio::test]
async fn login_device_code_verification_uri_is_passed_as_text() {
    let e = env().await;
    mount_device(&e.server, "https://evil.example/link?x=1").await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let browser = Arc::new(FakeBrowser::default());
    let (r, progress, _) = login_with(&e.auth, LoginMethod::DeviceCode, browser.clone()).await;
    r.unwrap();
    assert_eq!(
        progress.stages()[0],
        LoginProgress::DeviceCode {
            user_code: "ABCD-EFGH".into(),
            verification_uri: "https://evil.example/link?x=1".into(),
        }
    );
    assert!(browser.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn login_relogin_of_the_same_account_clears_needs_login() {
    let e = env().await;
    let id = e.seed(EntitlementState::Verified, T0);
    accounts::set_needs_login(&e.db, id, true).unwrap();
    assert!(e.auth.list().unwrap().accounts[0].needs_login);
    mount_code_exchange(&e.server).await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let (r, _, _) = login_with(
        &e.auth,
        LoginMethod::Browser,
        Arc::new(FakeBrowser::default()),
    )
    .await;
    assert_eq!(r.unwrap(), id);
    assert!(!e.row().needs_login);
    assert!(!e.auth.list().unwrap().accounts[0].needs_login);
    assert_eq!(accounts::list(&e.db).unwrap().len(), 1);
    assert_eq!(e.stored().as_deref(), Some(REFRESH_2));
}

#[tokio::test]
async fn login_relogin_as_a_different_account_adds_and_activates_it() {
    let e = env().await;
    let other = Uuid::from_u128(2);
    accounts::save_profile(
        &e.db,
        &ProfileRecord {
            id: other,
            username: "Alex".into(),
            xuid: None,
            skin_url: None,
        },
        EntitlementState::Verified,
        T0,
    )
    .unwrap();
    accounts::set_active(&e.db, other).unwrap();
    accounts::set_needs_login(&e.db, other, true).unwrap();
    mount_code_exchange(&e.server).await;
    mount_chain(&e.server, 86_400, Ownership::Owned).await;
    let (r, _, _) = login_with(
        &e.auth,
        LoginMethod::Browser,
        Arc::new(FakeBrowser::default()),
    )
    .await;
    assert_eq!(r.unwrap(), profile_id());
    let rows = accounts::list(&e.db).unwrap();
    assert_eq!(rows.len(), 2);
    let new = rows.iter().find(|r| r.id == profile_id()).unwrap();
    let old = rows.iter().find(|r| r.id == other).unwrap();
    assert!(new.is_active && !new.needs_login);
    // 元のアカウントは残り、使用中ではなくなる
    assert!(!old.is_active);
    assert_eq!(old.username, "Alex");
    assert!(old.needs_login);
}
