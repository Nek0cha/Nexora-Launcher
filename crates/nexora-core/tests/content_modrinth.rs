//! Modrinth の読み取りクライアント。
mod common;

use std::time::Duration;

use common::http;
use nexora_core::content::Side;
use nexora_core::content::modrinth::{
    DependencyKind, FullVersion, SearchQuery, SearchSort, UPDATE_BATCH, categories, loader_slugs,
    pick_version, primary_file, project_versions, projects, search, updates_by_hash,
    versions_by_hash, versions_by_ids,
};
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
        .up_to_n_times(1)
        .mount(&server)
        .await;
    // 2 回目以降は 5xx にする。サーバーを落とす方法だと、同じポートを別のテストのサーバーが
    // 取り直すことがあり（Linux と macOS で起きる）、404 が返って不安定になる
    Mock::given(method("GET"))
        .and(path("/tag/category"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let api = server.uri();
    let first = categories(&http(), &api, cache.path()).await.unwrap();
    assert_eq!(first.len(), 4);
    assert_eq!(first[2].name, "realistic");
    assert_eq!(first[2].project_type, "resourcepack");

    // 取得に失敗しても、キャッシュから返る
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

#[tokio::test]
async fn gives_up_after_a_second_429() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/version_files"))
        .respond_with(ResponseTemplate::new(429).insert_header("X-Ratelimit-Reset", "0"))
        .mount(&server)
        .await;
    let err = versions_by_hash(&http(), &server.uri(), &[HASH.to_string()])
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        nexora_core::error::CoreError::HttpStatus { status: 429, .. }
    ));
    // 最初と、待った後の 1 回だけ
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

const SEARCH: &str = include_str!("fixtures/modrinth/search.json");
const PROJECT_VERSIONS: &str = include_str!("fixtures/modrinth/project_versions.json");
const UPDATE: &str = include_str!("fixtures/modrinth/version_files_update.json");

fn query() -> SearchQuery {
    SearchQuery {
        query: "sodium".into(),
        mc_version: Some("1.20.1".into()),
        loaders: vec!["fabric".into()],
        categories: vec![],
        client_sides: vec![Side::Required],
        server_sides: vec![],
        sort: SearchSort::Downloads,
    }
}

#[tokio::test]
async fn search_builds_facets_from_the_query() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("query", "sodium"))
        .and(query_param("offset", "20"))
        .and(query_param("limit", "20"))
        .and(query_param("index", "downloads"))
        .respond_with(json(SEARCH))
        .expect(1)
        .mount(&server)
        .await;
    let page = search(&http(), &server.uri(), &query(), 20, 20)
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let facets = requests[0]
        .url
        .query_pairs()
        .find(|(k, _)| k == "facets")
        .unwrap()
        .1
        .to_string();
    let parsed: Vec<Vec<String>> = serde_json::from_str(&facets).unwrap();
    for want in [
        "project_type:mod",
        "versions:1.20.1",
        "categories:fabric",
        "client_side:required",
    ] {
        assert!(
            parsed.iter().any(|g| g.iter().any(|f| f == want)),
            "{want} missing in {facets}"
        );
    }
    assert_eq!(page.total, 42);
    assert_eq!(page.offset, 20);
    assert_eq!(page.hits.len(), 2);
    let h = &page.hits[0];
    assert_eq!(h.project_id, "AANobbMI");
    assert_eq!(h.downloads, 30_000_000_000);
    assert_eq!(h.client_side, Some(Side::Required));
    assert_eq!(page.hits[1].client_side, None);
    assert_eq!(page.hits[1].server_side, Some(Side::Optional));
}

#[tokio::test]
async fn project_versions_parses_files_and_dependencies() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/project/AANobbMI/version"))
        .and(query_param("loaders", "[\"fabric\"]"))
        .and(query_param("game_versions", "[\"1.20.1\"]"))
        .respond_with(json(PROJECT_VERSIONS))
        .expect(1)
        .mount(&server)
        .await;
    let vs = project_versions(
        &http(),
        &server.uri(),
        "AANobbMI",
        "1.20.1",
        &["fabric".to_string()],
    )
    .await
    .unwrap();
    assert_eq!(vs.len(), 2);
    let v = &vs[1];
    let kinds: Vec<_> = v.dependencies.iter().map(|d| d.kind).collect();
    assert_eq!(
        kinds,
        vec![
            DependencyKind::Required,
            DependencyKind::Optional,
            DependencyKind::Incompatible,
            DependencyKind::Embedded
        ]
    );
    assert_eq!(v.dependencies[3].project_id, None);
    assert_eq!(v.dependencies[3].version_id.as_deref(), Some("V4"));
    assert_eq!(v.files.len(), 2);
    assert_eq!(v.files[1].sha512.as_deref(), Some("abcd"));
    assert_eq!(primary_file(v).unwrap().filename, "b.jar");
}

#[tokio::test]
async fn versions_by_ids_parses_versions() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/versions"))
        .and(query_param("ids", "[\"VER1\",\"VER2\"]"))
        .respond_with(json(PROJECT_VERSIONS))
        .expect(1)
        .mount(&server)
        .await;
    let ids = vec!["VER1".to_string(), "VER2".to_string()];
    let vs = versions_by_ids(&http(), &server.uri(), &ids).await.unwrap();
    assert_eq!(vs.len(), 2);
}

#[tokio::test]
async fn updates_by_hash_splits_into_batches() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/version_files/update"))
        .respond_with(json(UPDATE))
        .expect(2)
        .mount(&server)
        .await;
    let mut hashes: Vec<String> = (0..UPDATE_BATCH).map(|i| format!("{i:040x}")).collect();
    hashes.push("1111111111111111111111111111111111111111".into());
    let map = updates_by_hash(
        &http(),
        &server.uri(),
        &hashes,
        "1.20.1",
        &["fabric".to_string()],
    )
    .await
    .unwrap();
    assert_eq!(map.len(), 1);
    assert_eq!(map["1111111111111111111111111111111111111111"].id, "NEWV");
    let reqs = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
    assert_eq!(body["algorithm"], "sha1");
    assert_eq!(body["loaders"], serde_json::json!(["fabric"]));
    assert_eq!(body["game_versions"], serde_json::json!(["1.20.1"]));
}

fn fv(id: &str, ty: &str) -> FullVersion {
    FullVersion {
        id: id.into(),
        project_id: "p".into(),
        version_number: id.into(),
        version_type: ty.into(),
        date_published: String::new(),
        game_versions: vec![],
        loaders: vec![],
        files: vec![],
        dependencies: vec![],
    }
}

#[test]
fn pick_version_prefers_latest_release() {
    let vs = vec![fv("a", "beta"), fv("b", "release"), fv("c", "release")];
    assert_eq!(pick_version(&vs).unwrap().id, "b");
    let vs = vec![fv("a", "beta"), fv("b", "alpha")];
    assert_eq!(pick_version(&vs).unwrap().id, "a");
    assert!(pick_version(&[]).is_none());
}

#[test]
fn loader_slugs_for_quilt_includes_fabric() {
    assert_eq!(loader_slugs(Some("quilt")), vec!["quilt", "fabric"]);
    assert_eq!(loader_slugs(Some("forge")), vec!["forge"]);
    assert_eq!(loader_slugs(Some("neoforge")), vec!["neoforge"]);
    assert!(loader_slugs(Some("vanilla")).is_empty());
    assert!(loader_slugs(None).is_empty());
}

#[tokio::test]
async fn project_versions_refuses_an_id_that_is_not_a_plain_slug() {
    let server = MockServer::start().await;
    for bad in ["../x", "a/b", "a?b", "", "a b", "a%2F"] {
        let err = project_versions(&http(), &server.uri(), bad, "1.20.1", &[])
            .await
            .unwrap_err();
        assert_eq!(err.code(), "meta.invalid", "{bad}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}
