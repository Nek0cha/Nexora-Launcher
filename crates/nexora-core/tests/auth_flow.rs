//! Microsoft 認証の結合テスト（wiremock）。

mod common;

use nexora_core::auth::http::AuthHttp;
use nexora_core::auth::microsoft::{exchange_code, poll_device_code, refresh, request_device_code};
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
