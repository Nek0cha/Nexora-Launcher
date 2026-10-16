//! ログインのタスク。ブラウザ（PKCE とループバック）とデバイスコードの 2 つの方式で
//! Microsoft のトークンを得て、`AuthService::complete_login` でアカウントを追加する。

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::LOGIN_TIMEOUT;
use super::loopback::Loopback;
use super::microsoft;
use super::oauth;
use super::service::AuthService;
use crate::error::CoreError;
use crate::task::TaskHandle;

/// ログインのタスクの kind。
pub const LOGIN_KIND: &str = "login";

/// ログインの方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LoginMethod {
    Browser,
    DeviceCode,
}

/// ログインの進み具合。UI のダイアログの表示に使う。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "stage",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum LoginProgress {
    WaitingForBrowser {
        url: String,
    },
    DeviceCode {
        user_code: String,
        verification_uri: String,
    },
    CheckingXbox,
    CheckingMinecraft,
}

/// `login://progress` で送る 1 件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LoginProgressEvent {
    pub task_id: String,
    pub progress: LoginProgress,
}

/// 認可 URL をシステムブラウザで開く口。Tauri 側が実装する。
pub trait UrlOpener: Send + Sync {
    fn open(&self, url: &str) -> Result<(), String>;
}

/// ログインの進捗を外へ伝える口。Tauri 側が実装する。
pub trait LoginNotifier: Send + Sync {
    fn progress(&self, event: &LoginProgressEvent);
}

/// キャンセルされたら `Cancelled` で打ち切る。
async fn cancellable<T>(
    handle: &TaskHandle,
    work: impl Future<Output = Result<T, CoreError>>,
) -> Result<T, CoreError> {
    tokio::select! {
        _ = handle.token().cancelled() => Err(CoreError::Cancelled),
        r = work => r,
    }
}

/// ログインを最後まで進め、追加（または更新）したアカウントの UUID を返す。
/// アカウントは使用中になる。
pub async fn run_login(
    auth: &AuthService,
    method: LoginMethod,
    opener: &dyn UrlOpener,
    notifier: &dyn LoginNotifier,
    handle: &TaskHandle,
) -> Result<Uuid, CoreError> {
    let client_id = auth.client_id().ok_or(CoreError::NoClientId)?;
    let report = |progress: LoginProgress| {
        notifier.progress(&LoginProgressEvent {
            task_id: handle.id().to_string(),
            progress,
        });
    };

    let tokens = match method {
        LoginMethod::Browser => {
            let loopback = Loopback::bind()?;
            let redirect_uri = loopback.redirect_uri();
            let pkce = oauth::new_pkce();
            let state = oauth::new_state();
            let url = oauth::authorize_url(
                auth.urls(),
                client_id,
                &redirect_uri,
                &state,
                &pkce.challenge,
            );
            // 開けなくても待ち続ける（UI が URL を表示し、開き直せるようにする）。
            // エラーの文に URL が入ることがあるため、ログには書かない
            report(LoginProgress::WaitingForBrowser { url: url.clone() });
            if opener.open(&url).is_err() {
                tracing::warn!("could not open the browser for sign-in");
            }
            let code = loopback.wait(&state, LOGIN_TIMEOUT, handle.token()).await?;
            cancellable(
                handle,
                microsoft::exchange_code(
                    auth.http(),
                    auth.urls(),
                    client_id,
                    &code,
                    &pkce.verifier,
                    &redirect_uri,
                    auth.now(),
                ),
            )
            .await?
        }
        LoginMethod::DeviceCode => {
            let code = cancellable(
                handle,
                microsoft::request_device_code(auth.http(), auth.urls(), client_id, auth.now()),
            )
            .await?;
            // verification_uri はそのまま渡す。開くかどうかは UI が許可リストで決める
            report(LoginProgress::DeviceCode {
                user_code: code.user_code.clone(),
                verification_uri: code.verification_uri.clone(),
            });
            microsoft::poll_device_code(
                auth.http(),
                auth.urls(),
                client_id,
                &code,
                handle.token(),
                || auth.now(),
            )
            .await?
        }
    };

    // ここから先は保存を伴うため、途中で打ち切らない
    if handle.token().is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    auth.complete_login(tokens, &report).await
}
