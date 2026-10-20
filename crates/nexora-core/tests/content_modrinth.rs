//! Modrinth の読み取りクライアント。
mod common;

use std::time::Duration;

use common::http;
use nexora_core::content::Side;
use nexora_core::content::modrinth::{categories, projects, versions_by_hash};
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const VERSION_FILES: &str = include_str!("fixtures/modrinth/version_files.json");
const PROJECTS: &str = include_str!("fixtures/modrinth/projects.json");
const CATEGORIES: &str = include_str!("fixtures/modrinth/categories.json");

const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn json(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.as_bytes().to_vec(), "application/json")
}

#[tokio::test]
async fn versions_by_hash_posts_hashes_and_maps_by_input_hash() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/version_files"))
        .and(body_json(serde_json::json!({
            "hashes": [HASH, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"],
            "algorithm": "sha1"
        })))
        .respond_with(json(VERSION_FILES))
        .expect(1)
        .mount(&server)
        .await;
    let inputs = vec![
        HASH.to_string(),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
    ];
    let map = versions_by_hash(&http(), &server.uri(), &inputs)
        .await
        .unwrap();
    // 分からなかったハッシュは含まれない
    assert_eq!(map.len(), 1);
    let v = &map[HASH];
    assert_eq!(v.version_id, "IIJJKKLL");
    assert_eq!(v.project_id, "P7dR8mSH");
    assert_eq!(v.version_number, "0.92.2+1.20.1");
}

#[tokio::test]
async fn only_required_dependencies_are_kept() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/version_files"))
        .respond_with(json(VERSION_FILES))
        .mount(&server)
        .await;
    let map = versions_by_hash(&http(), &server.uri(), &[HASH.to_string()])
        .await
        .unwrap();
    // optional、incompatible、embedded と、project_id が null のものは外れる
    assert_eq!(map[HASH].dependencies, vec!["AANobbMI".to_string()]);
}

#[tokio::test]
async fn projects_parse_environment_and_categories() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/projects"))
        .and(query_param("ids", r#"["P7dR8mSH","YL57xq9U","unk00001"]"#))
        .respond_with(json(PROJECTS))
        .expect(1)
        .mount(&server)
        .await;
    let ids: Vec<String> = ["P7dR8mSH", "YL57xq9U", "unk00001"]
        .map(String::from)
        .into();
    let list = projects(&http(), &server.uri(), &ids).await.unwrap();
    assert_eq!(list.len(), 3);

    let api = &list[0];
    assert_eq!(api.name, "Fabric API");
    assert_eq!(api.client_side, Some(Side::Required));
    assert_eq!(api.server_side, Some(Side::Required));
    assert_eq!(api.categories, vec!["fabric", "library", "utility"]);
    assert_eq!(
        api.icon_url.as_deref(),
        Some("https://cdn.modrinth.com/data/P7dR8mSH/icon.png")
    );

    let iris = &list[1];
    assert_eq!(iris.server_side, Some(Side::Unsupported));
    assert_eq!(iris.icon_url, None);

    // unknown は None、optional は Optional
    let mystery = &list[2];
    assert_eq!(mystery.client_side, None);
    assert_eq!(mystery.server_side, Some(Side::Optional));
}

#[tokio::test]
async fn categories_are_cached_and_served_offline() {
    let cache = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tag/category"))
        .respond_with(json(CATEGORIES))
        .mount(&server)
        .await;
    let api = server.uri();
    let first = categories(&http(), &api, cache.path()).await.unwrap();
    assert_eq!(first.len(), 4);
    assert_eq!(first[2].name, "realistic");
    assert_eq!(first[2].project_type, "resourcepack");

    // サーバーを止めても、キャッシュから返る
    drop(server);
    let second = categories(&http(), &api, cache.path()).await.unwrap();
    assert_eq!(second, first);
}

#[tokio::test]
async fn retries_once_after_429_using_the_reset_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/version_files"))
        .respond_with(ResponseTemplate::new(429).insert_header("X-Ratelimit-Reset", "0"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/version_files"))
        .respond_with(json(VERSION_FILES))
        .expect(1)
        .mount(&server)
        .await;
    let http = http().with_retry_base(Duration::from_millis(1));
    let map = versions_by_hash(&http, &server.uri(), &[HASH.to_string()])
        .await
        .unwrap();
    assert_eq!(map.len(), 1);
}

#[tokio::test]
async fn user_agent_names_the_app_and_a_contact() {
    let server = MockServer::start().await;
    let expected = format!(
        "Nexora/{} (https://github.com/Nek0cha/Nexora-Launcher)",
        env!("CARGO_PKG_VERSION")
    );
    Mock::given(method("POST"))
        .and(path("/version_files"))
        .and(header("user-agent", expected.as_str()))
        .respond_with(json("{}"))
        .expect(1)
        .mount(&server)
        .await;
    versions_by_hash(&http(), &server.uri(), &[HASH.to_string()])
        .await
        .unwrap();
}
