//! Microsoft 認証（OAuth、Xbox、Minecraft Services）。

pub mod accounts;
pub mod oauth;
pub mod store;

use std::time::Duration;

/// ログインの待ち受け時間。
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// トークンを取り直す余裕（ミリ秒）。
pub const REFRESH_MARGIN_MS: i64 = 5 * 60 * 1000;
/// 最後の `verified` からオフラインで遊べる期間（ミリ秒）。
pub const OFFLINE_GRACE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// 認証で使うエンドポイント。テストでは差し替える。
#[derive(Debug, Clone)]
pub struct AuthUrls {
    pub authorize: String,
    pub token: String,
    pub device_code: String,
    pub xbox_user: String,
    pub xsts: String,
    pub mc_login: String,
    pub mc_store: String,
    pub mc_license: String,
    pub mc_profile: String,
}

impl Default for AuthUrls {
    fn default() -> Self {
        let ms = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
        let mc = "https://api.minecraftservices.com";
        Self {
            authorize: format!("{ms}/authorize"),
            token: format!("{ms}/token"),
            device_code: format!("{ms}/devicecode"),
            xbox_user: "https://user.auth.xboxlive.com/user/authenticate".into(),
            xsts: "https://xsts.auth.xboxlive.com/xsts/authorize".into(),
            mc_login: format!("{mc}/authentication/login_with_xbox"),
            mc_store: format!("{mc}/entitlements/mcstore"),
            mc_license: format!("{mc}/entitlements/license"),
            mc_profile: format!("{mc}/minecraft/profile"),
        }
    }
}

/// ビルドに埋め込んだ Microsoft のクライアント ID。空文字は未設定として扱う。
pub fn client_id() -> Option<&'static str> {
    option_env!("NEXORA_MS_CLIENT_ID").filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_urls_are_the_real_endpoints() {
        let u = AuthUrls::default();
        assert_eq!(
            u.authorize,
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize"
        );
        assert_eq!(
            u.token,
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/token"
        );
        assert_eq!(
            u.device_code,
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode"
        );
        assert_eq!(
            u.xbox_user,
            "https://user.auth.xboxlive.com/user/authenticate"
        );
        assert_eq!(u.xsts, "https://xsts.auth.xboxlive.com/xsts/authorize");
        assert_eq!(
            u.mc_login,
            "https://api.minecraftservices.com/authentication/login_with_xbox"
        );
        assert_eq!(
            u.mc_store,
            "https://api.minecraftservices.com/entitlements/mcstore"
        );
        assert_eq!(
            u.mc_license,
            "https://api.minecraftservices.com/entitlements/license"
        );
        assert_eq!(
            u.mc_profile,
            "https://api.minecraftservices.com/minecraft/profile"
        );
    }

    #[test]
    fn constants_match_the_design() {
        assert_eq!(LOGIN_TIMEOUT.as_secs(), 600);
        assert_eq!(REFRESH_MARGIN_MS, 300_000);
        assert_eq!(OFFLINE_GRACE_MS, 2_592_000_000);
    }
}
