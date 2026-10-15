//! 結合テストの共通部品。
#![allow(dead_code)]

use std::time::Duration;

use nexora_core::net::{DownloadItem, Http, hash::sha1_hex};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 再試行の待ち時間を 1 ms にした Http。
pub fn http() -> Http {
    Http::new()
        .unwrap()
        .with_retry_base(Duration::from_millis(1))
}

/// body を返すモックを登録する。
pub async fn serve(server: &MockServer, route: &str, body: &[u8]) {
    Mock::given(method("GET"))
        .and(path(route.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(server)
        .await;
}

/// サーバーの route を、dir/name に保存する項目。
pub fn item(
    server: &MockServer,
    route: &str,
    dest: std::path::PathBuf,
    body: &[u8],
) -> DownloadItem {
    DownloadItem {
        url: format!("{}{route}", server.uri()),
        path: dest,
        size: Some(body.len() as u64),
        sha1: Some(sha1_hex(body)),
        executable: false,
    }
}
