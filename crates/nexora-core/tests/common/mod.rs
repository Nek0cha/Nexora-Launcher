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

/// dir の下（再帰）に残っている `.part` のファイル。
pub fn part_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(part_files(&path));
        } else if path.extension().is_some_and(|e| e == "part") {
            out.push(path);
        }
    }
    out
}

use nexora_core::java::platform::platform_key;
use nexora_core::java::runtime::java_relative_path;

/// テスト用の Java の中身。unix ではそのまま実行できるシェルスクリプトにする。
pub const FAKE_JAVA: &[u8] = b"#!/bin/sh\necho \"fake game $*\"\nexit 0\n";

/// FakeMojang が配るライブラリの中身。
pub const LIBRARY: &[u8] = b"library-jar";

/// 本文の途中で接続を切る HTTP サーバー（127.0.0.1 だけで待ち受ける）。
/// 最初の cut_times 回は、Content-Length は本来の長さのまま本文を半分だけ送って切る。
/// それ以降は本文をすべて送る。wiremock（hyper）は長さの合わない本文を送らないため、自前で書く。
pub struct CuttingServer {
    pub url: String,
    hits: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl CuttingServer {
    pub async fn start(body: &'static [u8], cut_times: usize) -> Self {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    // リクエストのヘッダーの終わりまで読む
                    let mut request = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(k) => request.extend_from_slice(&buf[..k]),
                        }
                    }
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let sent = if n < cut_times {
                        &body[..body.len() / 2]
                    } else {
                        body
                    };
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(sent).await;
                    let _ = socket.flush().await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self {
            url: format!("http://{addr}/lib.jar"),
            hits,
        }
    }

    pub fn hits(&self) -> usize {
        self.hits.load(std::sync::atomic::Ordering::SeqCst)
    }
}

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
    ///
    /// fetch_all は最初の失敗で取得中のほかのファイルを打ち切るため、404 を遅らせて、
    /// ほかのファイルの取得が先に終わるようにする。
    pub async fn start_with(serve_library: bool) -> Self {
        let fake = Self::build(serve_library, None).await;
        if !serve_library {
            Mock::given(method("GET"))
                .and(path("/lib.jar"))
                .respond_with(ResponseTemplate::new(404).set_delay(Duration::from_millis(500)))
                .up_to_n_times(1)
                .mount(&fake.server)
                .await;
        }
        fake
    }

    /// ライブラリ（中身は LIBRARY）だけを、別のサーバーの url から取得させる。
    pub async fn with_library_at(url: &str) -> Self {
        Self::build(false, Some(url.to_string())).await
    }

    async fn build(serve_library: bool, library_url: Option<String>) -> Self {
        let server = MockServer::start().await;
        let u = server.uri();
        let client = b"client-jar".to_vec();
        let library = LIBRARY.to_vec();
        let lib_url = library_url.unwrap_or_else(|| format!("{u}/lib.jar"));
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
"libraries":[{{"name":"com.example:lib:1.0","downloads":{{"artifact":{{"path":"com/example/lib/1.0/lib-1.0.jar","sha1":"{ls}","size":{ll},"url":"{lib_url}"}}}}}}],
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

use std::path::Path;
use std::pin::Pin;

use nexora_core::error::CoreError;
use nexora_core::java::verify::{JavaInfo, JavaProbe};

pub struct FixedProbe(pub u32);

impl JavaProbe for FixedProbe {
    fn probe<'a>(
        &'a self,
        _java: &'a Path,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<JavaInfo, CoreError>> + Send + 'a>> {
        let major = self.0;
        Box::pin(async move {
            Ok(JavaInfo {
                major,
                arch: std::env::consts::ARCH.to_string(),
            })
        })
    }
}

use std::sync::Mutex;

use nexora_core::launch::log4j::LogRecord;
use nexora_core::launch::process::{GameNotifier, RunningSession};

/// ゲームのイベントを記録する notifier。
#[derive(Default)]
pub struct RecordingGame {
    pub started: Mutex<Vec<RunningSession>>,
    pub logs: Mutex<Vec<(String, LogRecord)>>,
    pub exits: Mutex<Vec<(String, Option<i32>)>>,
}

impl GameNotifier for RecordingGame {
    fn started(&self, session: &RunningSession) {
        self.started.lock().unwrap().push(session.clone());
    }
    fn log(&self, instance_id: &str, record: &LogRecord) {
        self.logs
            .lock()
            .unwrap()
            .push((instance_id.to_string(), record.clone()));
    }
    fn exited(&self, instance_id: &str, exit_code: Option<i32>) {
        self.exits
            .lock()
            .unwrap()
            .push((instance_id.to_string(), exit_code));
    }
}

impl RecordingGame {
    /// instance_id の終了が届くまで待つ。
    pub async fn wait_exit(&self, instance_id: &str) -> Option<i32> {
        for _ in 0..1500 {
            if let Some((_, code)) = self
                .exits
                .lock()
                .unwrap()
                .iter()
                .find(|(id, _)| id == instance_id)
            {
                return *code;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("{instance_id} did not exit");
    }

    pub fn messages(&self) -> Vec<String> {
        self.logs
            .lock()
            .unwrap()
            .iter()
            .map(|(_, r)| r.message.clone())
            .collect()
    }
}

/// OS のシェルで script を実行する (program, args)。
pub fn shell(script: &str) -> (std::path::PathBuf, Vec<String>) {
    if cfg!(windows) {
        ("cmd".into(), vec!["/C".into(), script.into()])
    } else {
        ("sh".into(), vec!["-c".into(), script.into()])
    }
}

/// 認証のエンドポイントを wiremock の URL に向ける。
pub fn auth_urls(server: &MockServer) -> nexora_core::auth::AuthUrls {
    let u = server.uri();
    nexora_core::auth::AuthUrls {
        authorize: format!("{u}/authorize"),
        token: format!("{u}/token"),
        device_code: format!("{u}/devicecode"),
        xbox_user: format!("{u}/xbox/user"),
        xsts: format!("{u}/xsts"),
        mc_login: format!("{u}/mc/login"),
        mc_store: format!("{u}/mc/store"),
        mc_license: format!("{u}/mc/license"),
        mc_profile: format!("{u}/mc/profile"),
    }
}
