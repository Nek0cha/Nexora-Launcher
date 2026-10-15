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

use nexora_core::java::platform::platform_key;
use nexora_core::java::runtime::java_relative_path;

/// テスト用の Java の中身。unix ではそのまま実行できるシェルスクリプトにする。
pub const FAKE_JAVA: &[u8] = b"#!/bin/sh\necho \"fake game $*\"\nexit 0\n";

/// Mojang の配信の偽物。version 1.99-test（Java は jre-test / 17）を配る。
pub struct FakeMojang {
    pub server: MockServer,
    library: Vec<u8>,
}

impl FakeMojang {
    pub async fn start() -> Self {
        Self::start_with(true).await
    }

    /// serve_library が false なら、ライブラリだけ 404 にする（あとで serve_library を呼ぶ）。
    pub async fn start_with(serve_library: bool) -> Self {
        let server = MockServer::start().await;
        let u = server.uri();
        let client = b"client-jar".to_vec();
        let library = b"library-jar".to_vec();
        let asset = b"asset-object".to_vec();
        let asset_hash = sha1_hex(&asset);
        let index = format!(
            r#"{{"objects":{{"icon.png":{{"hash":"{asset_hash}","size":{}}}}}}}"#,
            asset.len()
        );
        let version = format!(
            r#"{{"id":"1.99-test","type":"release","mainClass":"net.minecraft.client.main.Main",
"arguments":{{"game":["--username","${{auth_player_name}}","--version","${{version_name}}","--gameDir","${{game_directory}}"],"jvm":["-cp","${{classpath}}"]}},
"assetIndex":{{"id":"test","sha1":"{is}","size":{il},"totalSize":0,"url":"{u}/index.json"}},
"downloads":{{"client":{{"sha1":"{cs}","size":{cl},"url":"{u}/client.jar"}}}},
"libraries":[{{"name":"com.example:lib:1.0","downloads":{{"artifact":{{"path":"com/example/lib/1.0/lib-1.0.jar","sha1":"{ls}","size":{ll},"url":"{u}/lib.jar"}}}}}}],
"javaVersion":{{"component":"jre-test","majorVersion":17}}}}"#,
            is = sha1_hex(index.as_bytes()),
            il = index.len(),
            cs = sha1_hex(&client),
            cl = client.len(),
            ls = sha1_hex(&library),
            ll = library.len(),
        );
        let manifest = format!(
            r#"{{"versions":[
{{"id":"1.99-test","type":"release","url":"{u}/version.json","releaseTime":"2030-01-01T00:00:00+00:00","sha1":"{vs}"}},
{{"id":"1.8.9","type":"release","url":"{u}/unused.json","releaseTime":"2015-12-03T09:24:39+00:00","sha1":"{vs}"}}]}}"#,
            vs = sha1_hex(version.as_bytes())
        );
        let java_rel = java_relative_path(std::env::consts::OS);
        let runtime_manifest = format!(
            r#"{{"files":{{"{java_rel}":{{"type":"file","executable":true,"downloads":{{"raw":{{"sha1":"{js}","size":{jl},"url":"{u}/java"}}}}}}}}}}"#,
            js = sha1_hex(FAKE_JAVA),
            jl = FAKE_JAVA.len()
        );
        let platform = platform_key(std::env::consts::OS, std::env::consts::ARCH)
            .expect("supported test platform");
        let runtime_index = format!(
            r#"{{"{platform}":{{"jre-test":[{{"availability":{{"group":1,"progress":100}},"manifest":{{"sha1":"{ms}","size":{ml},"url":"{u}/runtime.json"}},"version":{{"name":"17.0.1","released":"r"}}}}]}}}}"#,
            ms = sha1_hex(runtime_manifest.as_bytes()),
            ml = runtime_manifest.len()
        );
        serve(&server, "/manifest.json", manifest.as_bytes()).await;
        serve(&server, "/version.json", version.as_bytes()).await;
        serve(&server, "/index.json", index.as_bytes()).await;
        serve(&server, "/client.jar", &client).await;
        serve(
            &server,
            &format!("/res/{}/{asset_hash}", &asset_hash[..2]),
            &asset,
        )
        .await;
        serve(&server, "/runtimes.json", runtime_index.as_bytes()).await;
        serve(&server, "/runtime.json", runtime_manifest.as_bytes()).await;
        serve(&server, "/java", FAKE_JAVA).await;
        let fake = Self { server, library };
        if serve_library {
            fake.serve_library().await;
        }
        fake
    }

    pub async fn serve_library(&self) {
        serve(&self.server, "/lib.jar", &self.library).await;
    }

    pub fn manifest_url(&self) -> String {
        format!("{}/manifest.json", self.server.uri())
    }

    pub fn runtime_index_url(&self) -> String {
        format!("{}/runtimes.json", self.server.uri())
    }

    pub fn resources_url(&self) -> String {
        format!("{}/res", self.server.uri())
    }

    /// path へのリクエストの回数。
    pub async fn hits(&self, route: &str) -> usize {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == route)
            .count()
    }
}
