//! MOD の版の切り替え（一覧と切り替えのタスク）を Launcher を通して確かめる。
mod common;

use common::content::Harness;
use nexora_core::content::{ContentItem, ContentKind, InstalledAs};
use nexora_core::task::TaskState;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

/// 版の JSON に、種類と公開日を足す。
fn version(h: &Harness, id: &str, body: &[u8], kind: &str, date: &str) -> Value {
    let mut v = h.version_json("pa", id, &format!("{id}.jar"), body);
    v["version_type"] = json!(kind);
    v["date_published"] = json!(date);
    v
}

/// `pa` の版を、インスタンスの Minecraft とローダーで絞った問い合わせにだけ返す。
async fn serve_versions(h: &Harness, versions: Vec<Value>) {
    Mock::given(method("GET"))
        .and(path("/project/pa/version"))
        .and(query_param("game_versions", r#"["1.99-test"]"#))
        .and(query_param("loaders", r#"["fabric"]"#))
        .respond_with(ResponseTemplate::new(200).set_body_json(Value::Array(versions)))
        .mount(&h.server)
        .await;
}

/// v1（入っている版）、v2、v3（最新のリリース）、v4（最新だがベータ）。API の並びは日付の順ではない。
async fn publish_versions(h: &Harness) {
    let versions = vec![
        version(h, "pa-v2", b"a2", "release", "2024-02-01T00:00:00Z"),
        version(h, "pa-v4", b"a4", "beta", "2024-04-01T00:00:00Z"),
        version(h, "pa-v1", b"a1", "release", "2024-01-01T00:00:00Z"),
        version(h, "pa-v3", b"a3", "release", "2024-03-01T00:00:00Z"),
    ];
    serve_versions(h, versions).await;
    for (id, body) in [("pa-v2", b"a2"), ("pa-v3", b"a3"), ("pa-v4", b"a4")] {
        h.serve_file("pa", id, &format!("{id}.jar"), body, None)
            .await;
    }
}

async fn row(h: &Harness, rel: &str) -> ContentItem {
    h.launcher
        .list_content(&h.id, ContentKind::Mod)
        .await
        .unwrap()
        .into_iter()
        .find(|i| i.rel_path == rel)
        .unwrap_or_else(|| panic!("no row {rel}"))
}

#[tokio::test]
async fn list_returns_compatible_versions_newest_first_with_the_current_one_flagged() {
    let h = Harness::new().await;
    h.seed("pa.jar", b"a1", "pa", "pa-v1");
    publish_versions(&h).await;

    let choices = h
        .launcher
        .list_content_versions(&h.id, "mods/pa.jar")
        .await
        .unwrap();

    let ids: Vec<&str> = choices.iter().map(|c| c.version_id.as_str()).collect();
    assert_eq!(ids, ["pa-v4", "pa-v3", "pa-v2", "pa-v1"]);
    let current: Vec<&str> = choices
        .iter()
        .filter(|c| c.current)
        .map(|c| c.version_id.as_str())
        .collect();
    assert_eq!(current, ["pa-v1"]);
    assert_eq!(choices[0].version_type, "beta");
    assert_eq!(choices[0].version_number, "pa-v4-number");
    assert_eq!(choices[0].date_published, "2024-04-01T00:00:00Z");
}

#[tokio::test]
async fn list_refuses_rows_that_modrinth_did_not_identify() {
    let h = Harness::new().await;
    h.seed_local("local.jar", b"l");

    let err = h
        .launcher
        .list_content_versions(&h.id, "mods/local.jar")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "content.version_unavailable");
    let err = h
        .launcher
        .list_content_versions(&h.id, "mods/missing.jar")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "content.not_found");
    assert_eq!(h.hits("/project/pa/version").await, 0);
}

#[tokio::test]
async fn changing_to_an_older_version_swaps_the_file_and_pins_the_row() {
    let h = Harness::new().await;
    h.seed("pa.jar", b"a1", "pa", "pa-v1");
    publish_versions(&h).await;

    let task = h
        .launcher
        .change_content_version(&h.id, "mods/pa.jar", "pa-v2")
        .unwrap();
    let snap = h.wait(&task).await;

    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(snap.kind, "content");
    assert_eq!(h.mods(), ["pa-v2.jar"]);
    assert_eq!(
        std::fs::read(h.mods_dir().join("pa-v2.jar")).unwrap(),
        b"a2"
    );
    let item = row(&h, "mods/pa-v2.jar").await;
    assert_eq!(item.version_id.as_deref(), Some("pa-v2"));
    // 最新のリリースではないので、更新の確認が利用者の選択を戻さないよう固定する
    assert!(item.pinned);
    let report = h.launcher.content_task_report(&task).unwrap();
    assert_eq!(report.updated, 1);
    assert!(report.failures.is_empty());

    // 最新のリリースを選び直すと、固定を外す
    let task = h
        .launcher
        .change_content_version(&h.id, "mods/pa-v2.jar", "pa-v3")
        .unwrap();
    let snap = h.wait(&task).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(h.mods(), ["pa-v3.jar"]);
    assert!(!row(&h, "mods/pa-v3.jar").await.pinned);

    // 最新のリリースより新しいベータは、最新のリリースではないので固定する
    let task = h
        .launcher
        .change_content_version(&h.id, "mods/pa-v3.jar", "pa-v4")
        .unwrap();
    let snap = h.wait(&task).await;
    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert!(row(&h, "mods/pa-v4.jar").await.pinned);
}

#[tokio::test]
async fn a_disabled_mod_stays_disabled_after_a_change() {
    let h = Harness::new().await;
    h.seed("pa.jar.disabled", b"a1", "pa", "pa-v1");
    publish_versions(&h).await;

    let task = h
        .launcher
        .change_content_version(&h.id, "mods/pa.jar.disabled", "pa-v2")
        .unwrap();
    let snap = h.wait(&task).await;

    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(h.mods(), ["pa-v2.jar.disabled"]);
    assert!(!row(&h, "mods/pa-v2.jar.disabled").await.enabled);
}

#[tokio::test]
async fn a_hash_mismatch_rolls_the_change_back() {
    let h = Harness::new().await;
    h.seed("pa.jar", b"a1", "pa", "pa-v1");
    serve_versions(
        &h,
        vec![version(
            &h,
            "pa-v2",
            b"a2",
            "release",
            "2024-02-01T00:00:00Z",
        )],
    )
    .await;
    h.serve_file("pa", "pa-v2", "pa-v2.jar", b"tampered", None)
        .await;

    let task = h
        .launcher
        .change_content_version(&h.id, "mods/pa.jar", "pa-v2")
        .unwrap();
    let snap = h.wait(&task).await;

    assert_eq!(snap.state, TaskState::Failed);
    assert_eq!(snap.error.unwrap().code, "net.hash_mismatch");
    assert_eq!(h.mods(), ["pa.jar"]);
    assert_eq!(std::fs::read(h.mods_dir().join("pa.jar")).unwrap(), b"a1");
    let item = row(&h, "mods/pa.jar").await;
    assert_eq!(item.version_id.as_deref(), Some("pa-v1"));
    assert!(!item.pinned);
}

#[tokio::test]
async fn a_version_outside_the_compatible_list_is_refused() {
    let h = Harness::new().await;
    h.seed("pa.jar", b"a1", "pa", "pa-v1");
    publish_versions(&h).await;
    // 別の project の版や、このインスタンスに合わない版の ID
    h.serve_file("pb", "pb-v1", "pb.jar", b"b", None).await;

    for foreign in ["pb-v1", "pa-v0-for-other-minecraft"] {
        let task = h
            .launcher
            .change_content_version(&h.id, "mods/pa.jar", foreign)
            .unwrap();
        let snap = h.wait(&task).await;
        assert_eq!(snap.state, TaskState::Failed);
        assert_eq!(
            snap.error.unwrap().code,
            "content.version_unavailable",
            "{foreign}"
        );
    }
    assert_eq!(h.mods(), ["pa.jar"]);
    assert_eq!(h.hits("/data/pb/pb-v1/pb.jar").await, 0);
}

#[tokio::test]
async fn required_dependencies_of_the_chosen_version_are_installed_as_dependencies() {
    let h = Harness::new().await;
    h.seed("pa.jar", b"a1", "pa", "pa-v1");
    let mut v2 = version(&h, "pa-v2", b"a2", "release", "2024-02-01T00:00:00Z");
    v2["dependencies"] = json!([{ "project_id": "lib", "dependency_type": "required" }]);
    serve_versions(&h, vec![v2]).await;
    h.serve_file("pa", "pa-v2", "pa-v2.jar", b"a2", None).await;
    let lib = h.version_json("lib", "lib-v1", "lib.jar", b"lib");
    h.serve_project_versions("lib", vec![lib]).await;
    h.serve_file("lib", "lib-v1", "lib.jar", b"lib", None).await;

    let task = h
        .launcher
        .change_content_version(&h.id, "mods/pa.jar", "pa-v2")
        .unwrap();
    let snap = h.wait(&task).await;

    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    assert_eq!(h.mods(), ["lib.jar", "pa-v2.jar"]);
    assert_eq!(
        row(&h, "mods/lib.jar").await.installed_as,
        InstalledAs::Dependency
    );
    let report = h.launcher.content_task_report(&task).unwrap();
    assert_eq!(report.updated, 1);
    assert_eq!(report.dependencies_added, 1);
}

#[tokio::test]
async fn change_is_refused_while_the_game_runs() {
    let h = Harness::new().await;
    h.seed("pa.jar", b"a1", "pa", "pa-v1");
    publish_versions(&h).await;
    h.run();

    let err = h
        .launcher
        .change_content_version(&h.id, "mods/pa.jar", "pa-v2")
        .unwrap_err();
    assert_eq!(err.code(), "instance.busy");
    assert_eq!(h.mods(), ["pa.jar"]);
    // 版の一覧は、起動中でも見られる
    let choices = h
        .launcher
        .list_content_versions(&h.id, "mods/pa.jar")
        .await
        .unwrap();
    assert_eq!(choices.len(), 4);
}
