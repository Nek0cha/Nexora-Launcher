//! ブラウザから戻ってくる OAuth のリダイレクトを、ループバックで 1 回だけ受け取る。
//!
//! 要求の内容（コールバックの URL、認可コード）はログに書かない。

use std::net::{Ipv4Addr, Ipv6Addr, TcpListener as StdTcpListener};
use std::sync::Arc;
use std::time::Duration;

use secrecy::SecretString;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::oauth::{CallbackOutcome, CallbackReject, parse_callback};
use crate::error::CoreError;

/// 受け付けたあとにブラウザへ返す完了のページ。外部の資源は読み込まない。
pub const COMPLETION_PAGE: &str = "<!doctype html>\n\
<html lang=\"ja\"><head><meta charset=\"utf-8\"><title>Nexora</title></head>\n\
<body style=\"font-family:sans-serif;margin:3em\">\n\
<p>ログインが完了しました。このタブを閉じて、Nexora に戻ってください。</p>\n\
<p lang=\"en\">Sign-in complete. You can close this tab and return to Nexora.</p>\n\
</body></html>\n";

/// 拒否や失敗で待ち受けを終えるときにブラウザへ返すページ。
const FAILURE_PAGE: &str = "<!doctype html>\n\
<html lang=\"ja\"><head><meta charset=\"utf-8\"><title>Nexora</title></head>\n\
<body style=\"font-family:sans-serif;margin:3em\">\n\
<p>ログインは完了しませんでした。このタブを閉じて、Nexora に戻ってください。</p>\n\
<p lang=\"en\">Sign-in was not completed. You can close this tab and return to Nexora.</p>\n\
</body></html>\n";

const NOT_FOUND: &str = "Not Found\n";
const BAD_REQUEST: &str = "Bad Request\n";
const METHOD_NOT_ALLOWED: &str = "Method Not Allowed\n";

/// 要求の行とヘッダーの上限。
const MAX_HEAD: usize = 8 * 1024;
/// 1 つの接続に許す時間。何も送らない先行接続が残り続けないようにする。
const CONN_TIMEOUT: Duration = Duration::from_secs(30);
/// 同時に扱う接続の上限。
const MAX_CONNS: usize = 32;

/// 1 回だけのコールバックを待つループバックの待ち受け。
pub struct Loopback {
    v4: StdTcpListener,
    v6: Option<StdTcpListener>,
    port: u16,
}

fn io_err(source: std::io::Error) -> CoreError {
    CoreError::Io {
        path: "127.0.0.1".to_string(),
        source,
    }
}

impl Loopback {
    /// `127.0.0.1` の空いているポートで待ち受け、同じポートで `[::1]` も試す（失敗は無視）。
    pub fn bind() -> Result<Loopback, CoreError> {
        let v4 = StdTcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(io_err)?;
        v4.set_nonblocking(true).map_err(io_err)?;
        let port = v4.local_addr().map_err(io_err)?.port();
        let v6 = StdTcpListener::bind((Ipv6Addr::LOCALHOST, port))
            .ok()
            .filter(|l| l.set_nonblocking(true).is_ok());
        Ok(Loopback { v4, v6, port })
    }

    /// 認可 URL に渡す `redirect_uri`。
    pub fn redirect_uri(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    /// コールバックを 1 回だけ受け付け、認可コードを返す。
    /// 戻るときに待ち受けを閉じる（`self` を消費する）。
    pub async fn wait(
        self,
        expected_state: &str,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<SecretString, CoreError> {
        let v4 = TcpListener::from_std(self.v4).map_err(io_err)?;
        let v6 = self.v6.and_then(|l| TcpListener::from_std(l).ok());
        let state: Arc<str> = Arc::from(expected_state);
        // 戻るときに破棄され、残りの接続の処理は中断される。
        let mut conns: JoinSet<Step> = JoinSet::new();
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Err(CoreError::Cancelled),
                _ = &mut deadline => return Err(CoreError::AuthTimeout),
                accepted = accept_any(&v4, v6.as_ref()), if conns.len() < MAX_CONNS => {
                    match accepted {
                        Ok(stream) => {
                            let state = Arc::clone(&state);
                            conns.spawn(async move {
                                tokio::time::timeout(CONN_TIMEOUT, handle(stream, &state))
                                    .await
                                    .unwrap_or(Step::Continue)
                            });
                        }
                        Err(e) => {
                            tracing::debug!(kind = ?e.kind(), "ループバックの accept に失敗");
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    }
                }
                Some(joined) = conns.join_next() => {
                    if let Ok(Step::Done(result)) = joined {
                        return result;
                    }
                }
            }
        }
    }
}

async fn accept_any(v4: &TcpListener, v6: Option<&TcpListener>) -> std::io::Result<TcpStream> {
    let v6_accept = async {
        match v6 {
            Some(l) => l.accept().await,
            None => std::future::pending().await,
        }
    };
    tokio::select! {
        r = v4.accept() => r.map(|(s, _)| s),
        r = v6_accept => r.map(|(s, _)| s),
    }
}

/// 1 つの接続の結果。
enum Step {
    /// この要求は無視して待ち続ける。
    Continue,
    /// 待ち受けを終える。
    Done(Result<SecretString, CoreError>),
}

enum Head {
    /// 要求の行（CRLF を除く）。
    Line(String),
    TooLarge,
    Malformed,
    Closed,
}

/// 空行までを最大 `MAX_HEAD` バイト読み、要求の行を返す。
async fn read_head(s: &mut TcpStream) -> Head {
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        let n = match s.read(&mut chunk).await {
            Ok(0) | Err(_) => return Head::Closed,
            Ok(n) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            if end + 4 > MAX_HEAD {
                return Head::TooLarge;
            }
            let line_end = buf.windows(2).position(|w| w == b"\r\n").unwrap_or(end);
            return match std::str::from_utf8(&buf[..line_end]) {
                Ok(line) => Head::Line(line.to_string()),
                Err(_) => Head::Malformed,
            };
        }
        if buf.len() >= MAX_HEAD {
            return Head::TooLarge;
        }
    }
}

async fn respond(s: &mut TcpStream, status: &str, body: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    // ブラウザへの応答の失敗は、ログインの結果に影響させない。
    let _ = s.write_all(head.as_bytes()).await;
    let _ = s.write_all(body.as_bytes()).await;
    let _ = s.shutdown().await;
}

/// 読み残しのある接続を閉じると RST になり、応答が届かないことがあるため、
/// 応答の後に残りを少しの間読み捨てる。
async fn drain(s: &mut TcpStream) {
    let mut sink = [0u8; 4096];
    let _ = tokio::time::timeout(Duration::from_secs(1), async {
        let mut total = 0usize;
        while total < 64 * 1024 {
            match s.read(&mut sink).await {
                Ok(0) | Err(_) => break,
                Ok(n) => total += n,
            }
        }
    })
    .await;
}

async fn handle(mut s: TcpStream, expected_state: &str) -> Step {
    let line = match read_head(&mut s).await {
        Head::Line(line) => line,
        Head::TooLarge => {
            respond(&mut s, "400 Bad Request", BAD_REQUEST).await;
            drain(&mut s).await;
            return Step::Continue;
        }
        Head::Malformed => {
            respond(&mut s, "400 Bad Request", BAD_REQUEST).await;
            return Step::Continue;
        }
        Head::Closed => return Step::Continue,
    };
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        respond(&mut s, "400 Bad Request", BAD_REQUEST).await;
        return Step::Continue;
    };
    if !version.starts_with("HTTP/") {
        respond(&mut s, "400 Bad Request", BAD_REQUEST).await;
        return Step::Continue;
    }
    if method != "GET" {
        respond(&mut s, "405 Method Not Allowed", METHOD_NOT_ALLOWED).await;
        return Step::Continue;
    }
    // origin-form（"/" で始まり "//" で始まらない）以外は、ブラウザのリダイレクトからは来ない。
    if !target.starts_with('/') || target.starts_with("//") {
        respond(&mut s, "400 Bad Request", BAD_REQUEST).await;
        return Step::Continue;
    }
    match parse_callback(target, expected_state) {
        Ok(CallbackOutcome::Code(code)) => {
            respond(&mut s, "200 OK", COMPLETION_PAGE).await;
            Step::Done(Ok(code))
        }
        Ok(CallbackOutcome::Denied) => {
            respond(&mut s, "400 Bad Request", FAILURE_PAGE).await;
            Step::Done(Err(CoreError::AuthDenied))
        }
        Ok(CallbackOutcome::Error) | Err(CallbackReject::Invalid) => {
            respond(&mut s, "400 Bad Request", FAILURE_PAGE).await;
            Step::Done(Err(CoreError::AuthCallbackInvalid))
        }
        Err(CallbackReject::NotFound) => {
            respond(&mut s, "404 Not Found", NOT_FOUND).await;
            Step::Continue
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::task::JoinHandle;

    const LONG: Duration = Duration::from_secs(30);

    /// 生の要求を送り、サーバーが閉じるまでの応答を返す。
    async fn send(port: u16, raw: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(raw.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.unwrap();
        String::from_utf8_lossy(&out).into_owned()
    }

    fn get(target: &str) -> String {
        format!("GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n")
    }

    async fn start(
        state: &'static str,
        timeout: Duration,
    ) -> (
        u16,
        CancellationToken,
        JoinHandle<Result<SecretString, CoreError>>,
    ) {
        let lb = Loopback::bind().unwrap();
        let port = lb.port;
        let cancel = CancellationToken::new();
        let c = cancel.clone();
        let h = tokio::spawn(async move { lb.wait(state, timeout, &c).await });
        (port, cancel, h)
    }

    #[tokio::test]
    async fn redirect_uri_uses_localhost_and_the_bound_port() {
        let lb = Loopback::bind().unwrap();
        assert_ne!(lb.port, 0);
        assert_eq!(lb.redirect_uri(), format!("http://localhost:{}", lb.port));
    }

    #[test]
    fn completion_page_is_self_contained_and_bilingual() {
        assert!(COMPLETION_PAGE.contains("ログインが完了しました。"));
        assert!(COMPLETION_PAGE.contains("Nexora"));
        assert!(COMPLETION_PAGE.contains("Sign-in complete"));
        assert!(!COMPLETION_PAGE.contains("http"));
        assert!(!COMPLETION_PAGE.contains("src="));
    }

    #[tokio::test]
    async fn accepts_one_valid_callback() {
        let (port, _c, h) = start("st", LONG).await;
        let resp = send(port, &get("/?code=c1&state=st")).await;
        assert!(resp.starts_with("HTTP/1.1 200 "), "{resp}");
        assert!(resp.contains(COMPLETION_PAGE));
        assert_eq!(h.await.unwrap().unwrap().expose_secret(), "c1");
    }

    #[tokio::test]
    async fn ignores_unrelated_paths_and_keeps_waiting() {
        let (port, _c, h) = start("st", LONG).await;
        let resp = send(port, &get("/favicon.ico")).await;
        assert!(resp.starts_with("HTTP/1.1 404 "), "{resp}");
        assert!(!h.is_finished());
        let resp = send(port, &get("/?code=c2&state=st")).await;
        assert!(resp.starts_with("HTTP/1.1 200 "), "{resp}");
        assert_eq!(h.await.unwrap().unwrap().expose_secret(), "c2");
    }

    #[tokio::test]
    async fn rejects_non_origin_form_targets_and_keeps_waiting() {
        let (port, _c, h) = start("st", LONG).await;
        for target in ["?code=evil&state=st", "//x", "//x/?code=evil&state=st"] {
            let resp = send(port, &get(target)).await;
            assert!(resp.starts_with("HTTP/1.1 400 "), "{target}: {resp}");
        }
        assert!(!h.is_finished());
        send(port, &get("/?code=c3&state=st")).await;
        assert_eq!(h.await.unwrap().unwrap().expose_secret(), "c3");
    }

    #[tokio::test]
    async fn rejects_other_methods_and_keeps_waiting() {
        let (port, _c, h) = start("st", LONG).await;
        let resp = send(
            port,
            "POST /?code=evil&state=st HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )
        .await;
        assert!(resp.starts_with("HTTP/1.1 405 "), "{resp}");
        assert!(!h.is_finished());
        send(port, &get("/?code=c4&state=st")).await;
        assert_eq!(h.await.unwrap().unwrap().expose_secret(), "c4");
    }

    #[tokio::test]
    async fn state_mismatch_fails_with_callback_invalid() {
        let (port, _c, h) = start("st", LONG).await;
        let resp = send(port, &get("/?code=c1&state=zz")).await;
        assert!(resp.starts_with("HTTP/1.1 400 "), "{resp}");
        let err = h.await.unwrap().unwrap_err();
        assert_eq!(err.code(), "auth.callback_invalid");
    }

    #[tokio::test]
    async fn denied_fails_with_auth_denied() {
        let (port, _c, h) = start("st", LONG).await;
        send(port, &get("/?error=access_denied&state=st")).await;
        let err = h.await.unwrap().unwrap_err();
        assert_eq!(err.code(), "auth.denied");
    }

    #[tokio::test]
    async fn other_error_fails_with_callback_invalid() {
        let (port, _c, h) = start("st", LONG).await;
        send(port, &get("/?error=server_error&state=st")).await;
        let err = h.await.unwrap().unwrap_err();
        assert_eq!(err.code(), "auth.callback_invalid");
    }

    #[tokio::test]
    async fn times_out() {
        let (_port, _c, h) = start("st", Duration::from_millis(50)).await;
        let err = h.await.unwrap().unwrap_err();
        assert_eq!(err.code(), "auth.timeout");
    }

    #[tokio::test]
    async fn cancel_returns_cancelled_and_frees_the_port() {
        let (port, cancel, h) = start("st", LONG).await;
        cancel.cancel();
        let err = h.await.unwrap().unwrap_err();
        assert!(matches!(err, CoreError::Cancelled));
        TcpListener::bind(("127.0.0.1", port)).await.unwrap();
    }

    #[tokio::test]
    async fn rejects_oversized_requests() {
        let (port, _c, h) = start("st", LONG).await;
        let big = format!("/{}", "a".repeat(9000));
        let resp = send(port, &get(&big)).await;
        assert!(resp.starts_with("HTTP/1.1 400 "), "{resp}");
        assert!(!h.is_finished());
        send(port, &get("/?code=c5&state=st")).await;
        assert_eq!(h.await.unwrap().unwrap().expose_secret(), "c5");
    }

    #[tokio::test]
    async fn an_idle_connection_does_not_block_the_callback() {
        let (port, _c, h) = start("st", LONG).await;
        // ブラウザの先行接続のように、何も送らない接続を開いたままにする。
        let _idle = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        send(port, &get("/?code=c6&state=st")).await;
        assert_eq!(h.await.unwrap().unwrap().expose_secret(), "c6");
    }

    #[tokio::test]
    async fn closes_after_success() {
        let (port, _c, h) = start("st", LONG).await;
        send(port, &get("/?code=c1&state=st")).await;
        h.await.unwrap().unwrap();
        assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
    }
}
