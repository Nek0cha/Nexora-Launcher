//! OAuth のうち、通信を伴わない部分（PKCE、認可 URL、コールバックの解析）。

use std::collections::HashMap;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use secrecy::SecretString;
use sha2::{Digest, Sha256};

use super::AuthUrls;

/// 要求するスコープ。
const SCOPE: &str = "XboxLive.signin offline_access";

/// PKCE の verifier と、その S256 challenge。
pub struct Pkce {
    pub verifier: SecretString,
    pub challenge: String,
}

/// コールバックの結果。
pub enum CallbackOutcome {
    Code(SecretString),
    /// ユーザーが同意しなかった（`access_denied`）。
    Denied,
    /// その他のエラー応答。
    Error,
}

/// コールバックとして受け付けない要求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackReject {
    /// コールバックのパスではない。
    NotFound,
    /// パラメーターの欠落、重複、`state` の不一致。
    Invalid,
}

fn random_b64(len: usize) -> String {
    let mut buf = vec![0u8; len];
    getrandom::fill(&mut buf).expect("OS の乱数が使えない");
    URL_SAFE_NO_PAD.encode(buf)
}

/// S256 の challenge（base64url、パディングなし）。
pub fn challenge_for(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// 32 バイトの乱数から verifier（43 文字）を作り、challenge を付ける。
pub fn new_pkce() -> Pkce {
    let verifier = random_b64(32);
    let challenge = challenge_for(&verifier);
    Pkce {
        verifier: SecretString::from(verifier),
        challenge,
    }
}

/// 16 バイトの乱数の base64url。
pub fn new_state() -> String {
    random_b64(16)
}

/// ブラウザで開く認可 URL。
pub fn authorize_url(
    urls: &AuthUrls,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> String {
    let mut url = reqwest::Url::parse(&urls.authorize).expect("認可エンドポイントが URL ではない");
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", SCOPE)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        // サブのアカウントを追加するとき、ブラウザに残るメインのアカウントで自動ログインさせない。
        .append_pair("prompt", "select_account");
    url.into()
}

/// HTTP のリクエストターゲット（パスとクエリ）を解析する。
/// `error` は `code` より優先するが、どちらも `state` が一致していなければ拒否する。
pub fn parse_callback(
    target: &str,
    expected_state: &str,
) -> Result<CallbackOutcome, CallbackReject> {
    let url = reqwest::Url::parse(&format!("http://localhost{target}"))
        .map_err(|_| CallbackReject::NotFound)?;
    if url.path() != "/" {
        return Err(CallbackReject::NotFound);
    }
    let mut params: HashMap<String, String> = HashMap::new();
    for (k, v) in url.query_pairs() {
        if params.insert(k.into_owned(), v.into_owned()).is_some() {
            return Err(CallbackReject::Invalid);
        }
    }
    if params.get("state").map(String::as_str) != Some(expected_state) {
        return Err(CallbackReject::Invalid);
    }
    if let Some(error) = params.get("error") {
        return Ok(if error == "access_denied" {
            CallbackOutcome::Denied
        } else {
            CallbackOutcome::Error
        });
    }
    match params.remove("code") {
        Some(code) if !code.is_empty() => Ok(CallbackOutcome::Code(SecretString::from(code))),
        _ => Err(CallbackReject::Invalid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    fn code_of(r: Result<CallbackOutcome, CallbackReject>) -> String {
        match r {
            Ok(CallbackOutcome::Code(c)) => c.expose_secret().to_string(),
            _ => panic!("コードではない"),
        }
    }

    #[test]
    fn challenge_matches_rfc7636_appendix_b() {
        assert_eq!(
            challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn pkce_and_state_have_the_expected_shape() {
        let p = new_pkce();
        assert_eq!(p.verifier.expose_secret().len(), 43);
        assert_eq!(p.challenge, challenge_for(p.verifier.expose_secret()));
        assert_eq!(new_state().len(), 22);
        assert_ne!(new_state(), new_state());
    }

    #[test]
    fn authorize_url_has_the_required_parameters() {
        let url = authorize_url(
            &AuthUrls::default(),
            "cid",
            "http://localhost:5555",
            "st",
            "ch",
        );
        let parsed = reqwest::Url::parse(&url).unwrap();
        let q: HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(q["client_id"], "cid");
        assert_eq!(q["response_type"], "code");
        assert_eq!(q["redirect_uri"], "http://localhost:5555");
        assert_eq!(q["scope"], "XboxLive.signin offline_access");
        assert_eq!(q["state"], "st");
        assert_eq!(q["code_challenge"], "ch");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["prompt"], "select_account");
    }

    #[test]
    fn callback_with_code_and_matching_state() {
        assert_eq!(code_of(parse_callback("/?code=abc&state=st", "st")), "abc");
    }

    #[test]
    fn callback_values_are_percent_decoded() {
        assert_eq!(
            code_of(parse_callback("/?code=a%2Bb%20c&state=st", "st")),
            "a+b c"
        );
    }

    #[test]
    fn callback_other_path_is_not_found() {
        assert!(matches!(
            parse_callback("/favicon.ico", "st"),
            Err(CallbackReject::NotFound)
        ));
    }

    #[test]
    fn callback_state_mismatch_is_invalid() {
        assert!(matches!(
            parse_callback("/?code=abc&state=zz", "st"),
            Err(CallbackReject::Invalid)
        ));
    }

    #[test]
    fn callback_duplicate_parameters_are_invalid() {
        assert!(matches!(
            parse_callback("/?code=a&code=b&state=st", "st"),
            Err(CallbackReject::Invalid)
        ));
    }

    #[test]
    fn callback_access_denied_is_denied() {
        assert!(matches!(
            parse_callback("/?error=access_denied&state=st", "st"),
            Ok(CallbackOutcome::Denied)
        ));
    }

    #[test]
    fn callback_other_error_is_error() {
        assert!(matches!(
            parse_callback("/?error=server_error&state=st", "st"),
            Ok(CallbackOutcome::Error)
        ));
    }

    #[test]
    fn callback_error_wins_over_code_but_needs_the_state() {
        assert!(matches!(
            parse_callback("/?error=access_denied&code=abc&state=st", "st"),
            Ok(CallbackOutcome::Denied)
        ));
        assert!(matches!(
            parse_callback("/?error=access_denied&state=zz", "st"),
            Err(CallbackReject::Invalid)
        ));
    }

    #[test]
    fn callback_without_query_is_invalid() {
        assert!(matches!(
            parse_callback("/", "st"),
            Err(CallbackReject::Invalid)
        ));
    }
}
