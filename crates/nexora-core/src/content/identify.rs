//! 中身をハッシュで Modrinth に照会して、表示と絞り込みに使う情報を行に書く。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::params;

use super::local_meta::{LocalMeta, read_local_meta};
use super::modrinth::{ProjectInfo, VersionInfo, projects, versions_by_hash};
use super::store::{self, Row};
use super::{ContentKind, Provider, Side};
use crate::db::Db;
use crate::error::CoreError;
use crate::net::Http;

const KINDS: [ContentKind; 3] = [
    ContentKind::Mod,
    ContentKind::ResourcePack,
    ContentKind::Shader,
];

/// 特定の結果。`identified` は Modrinth で分かった行、`local` は照会したが分からなかった行。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct IdentifyReport {
    pub identified: usize,
    pub local: usize,
}

/// 1 回の照会で送る件数の上限。
#[derive(Debug, Clone, Copy)]
struct Batches {
    hashes: usize,
    projects: usize,
}

const BATCHES: Batches = Batches {
    hashes: 500,
    projects: 100,
};

/// `identified_at` が空の行を、種類を問わずすべて特定する。
///
/// まず通信なしでファイルの中のメタデータを読んで行に書く（`identified_at` は触らない）。
/// 次に sha1 を Modrinth に照会し、見つかった行は Modrinth の値で上書きして、
/// 見つからなかった行は `local` のまま、どちらも `identified_at = now_ms` にする。
/// 通信の失敗（`Offline` などの HTTP エラー）では、ローカルの分だけを書いたまま `Err` を返す。
pub async fn identify(
    db: &Db,
    http: &Http,
    api: &str,
    icons_dir: &Path,
    game_dir: &Path,
    instance_id: &str,
    now_ms: i64,
) -> Result<IdentifyReport, CoreError> {
    identify_with(
        db,
        http,
        api,
        icons_dir,
        game_dir,
        instance_id,
        now_ms,
        BATCHES,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn identify_with(
    db: &Db,
    http: &Http,
    api: &str,
    icons_dir: &Path,
    game_dir: &Path,
    instance_id: &str,
    now_ms: i64,
    batches: Batches,
) -> Result<IdentifyReport, CoreError> {
    let mut targets = Vec::new();
    for kind in KINDS {
        targets.extend(
            store::rows(db, instance_id, kind)?
                .into_iter()
                .filter(|r| r.identified_at.is_none()),
        );
    }
    if targets.is_empty() {
        return Ok(IdentifyReport::default());
    }

    // ファイルを読むので、非同期の実行を止めないように別のスレッドで行う
    let local_targets: Vec<Row> = targets
        .iter()
        .filter(|r| needs_local_meta(r))
        .cloned()
        .collect();
    if !local_targets.is_empty() {
        let (db, icons_dir, game_dir, instance_id) = (
            db.clone(),
            icons_dir.to_path_buf(),
            game_dir.to_path_buf(),
            instance_id.to_string(),
        );
        tokio::task::spawn_blocking(move || {
            write_local_meta(&db, &icons_dir, &game_dir, &instance_id, &local_targets)
        })
        .await
        .map_err(|e| CoreError::Unexpected {
            message: format!("local metadata task failed: {e}"),
        })??;
    }

    // 同じ中身のファイルが複数あっても 1 回だけ照会する
    let mut seen = HashSet::new();
    let hashes: Vec<String> = targets
        .iter()
        .map(|r| r.item.sha1.clone())
        .filter(|h| seen.insert(h.clone()))
        .collect();
    let mut versions: HashMap<String, VersionInfo> = HashMap::new();
    for chunk in hashes.chunks(batches.hashes) {
        versions.extend(versions_by_hash(http, api, chunk).await?);
    }
    let mut seen = HashSet::new();
    let project_ids: Vec<String> = hashes
        .iter()
        .filter_map(|h| versions.get(h))
        .map(|v| v.project_id.clone())
        .filter(|id| seen.insert(id.clone()))
        .collect();
    let mut found: HashMap<String, ProjectInfo> = HashMap::new();
    for chunk in project_ids.chunks(batches.projects) {
        for p in projects(http, api, chunk).await? {
            found.insert(p.project_id.clone(), p);
        }
    }

    write_remote(db, instance_id, &targets, &versions, &found, now_ms)
}

/// まだローカルのメタデータを読んでいない行か。
///
/// 走査で特定の結果が捨てられた行は、表示の列も空に戻っているので、また読む。
/// 中に何も無いファイルは、特定されるまで毎回読み直すことになるが、読むのは安い。
fn needs_local_meta(row: &Row) -> bool {
    row.item.name.is_none()
        && row.item.version_number.is_none()
        && row.item.description.is_none()
        && row.local_icon_key.is_none()
}

/// ファイルの中のメタデータを読み、まとめて 1 トランザクションで書く。
///
/// 読めないファイル（消えた、壊れた）は空のメタデータとして扱い、全体は止めない。
/// アイコンは `icons_dir/<sha1>.png` に書き、書けなければアイコンだけを諦める。
fn write_local_meta(
    db: &Db,
    icons_dir: &Path,
    game_dir: &Path,
    instance_id: &str,
    rows: &[Row],
) -> Result<(), CoreError> {
    let metas: Vec<(&Row, LocalMeta, Option<String>)> = rows
        .iter()
        .map(|row| {
            let path: PathBuf = game_dir.join(&row.item.rel_path);
            let meta = read_local_meta(row.item.kind, &path);
            let icon_key = meta
                .icon_png
                .as_deref()
                .and_then(|png| save_icon(icons_dir, &row.item.sha1, png));
            (row, meta, icon_key)
        })
        .collect();
    db.with_conn(|conn| {
        let tx = conn.transaction()?;
        for (row, meta, icon_key) in &metas {
            // 読んでいる間に走査で中身が変わった行は書かない（sha1 で確かめる）
            tx.execute(
                "UPDATE instance_contents
                 SET name = ?4, version_number = ?5, description = ?6, local_icon_key = ?7
                 WHERE instance_id = ?1 AND rel_path = ?2 AND sha1 = ?3
                   AND identified_at IS NULL",
                params![
                    instance_id,
                    row.item.rel_path,
                    row.item.sha1,
                    meta.name,
                    meta.version,
                    meta.description,
                    icon_key,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
}

/// アイコンを書いて、そのキー（sha1）を返す。書けなければ警告を出して `None`。
fn save_icon(icons_dir: &Path, sha1: &str, png: &[u8]) -> Option<String> {
    let path = icons_dir.join(format!("{sha1}.png"));
    let written = std::fs::create_dir_all(icons_dir).and_then(|()| std::fs::write(&path, png));
    match written {
        Ok(()) => Some(sha1.to_string()),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "could not save a local icon");
            None
        }
    }
}

/// 照会の結果を 1 トランザクションで書く。
fn write_remote(
    db: &Db,
    instance_id: &str,
    targets: &[Row],
    versions: &HashMap<String, VersionInfo>,
    found: &HashMap<String, ProjectInfo>,
    now_ms: i64,
) -> Result<IdentifyReport, CoreError> {
    db.with_conn(|conn| {
        let tx = conn.transaction()?;
        let mut report = IdentifyReport::default();
        for row in targets {
            let key = params![instance_id, row.item.rel_path, row.item.sha1, now_ms];
            let Some(version) = versions.get(&row.item.sha1) else {
                // 照会したが分からなかった、の印だけを付ける
                // 照会の間に走査で中身が変わった行は数えない
                report.local += tx.execute(
                    "UPDATE instance_contents SET identified_at = ?4
                     WHERE instance_id = ?1 AND rel_path = ?2 AND sha1 = ?3",
                    key,
                )?;
                continue;
            };
            let dependencies =
                serde_json::to_string(&version.dependencies).unwrap_or_else(|_| "[]".into());
            let changed = tx.execute(
                "UPDATE instance_contents
                 SET identified_at = ?4, provider = ?5, project_id = ?6, version_id = ?7,
                     version_number = ?8, dependencies = ?9
                 WHERE instance_id = ?1 AND rel_path = ?2 AND sha1 = ?3",
                params![
                    instance_id,
                    row.item.rel_path,
                    row.item.sha1,
                    now_ms,
                    Provider::Modrinth.as_str(),
                    version.project_id,
                    version.version_id,
                    version.version_number,
                    dependencies,
                ],
            )?;
            if changed == 0 {
                continue;
            }
            // プロジェクトが引けなかった（消された、非公開など）ときは、表示の列はローカルの値のまま
            if let Some(p) = found.get(&version.project_id) {
                let categories =
                    serde_json::to_string(&p.categories).unwrap_or_else(|_| "[]".into());
                tx.execute(
                    "UPDATE instance_contents
                     SET name = ?4, description = ?5, icon_url = ?6, client_side = ?7,
                         server_side = ?8, categories = ?9
                     WHERE instance_id = ?1 AND rel_path = ?2 AND sha1 = ?3",
                    params![
                        instance_id,
                        row.item.rel_path,
                        row.item.sha1,
                        p.name,
                        p.description,
                        p.icon_url,
                        p.client_side.map(Side::as_str),
                        p.server_side.map(Side::as_str),
                        categories,
                    ],
                )?;
            }
            report.identified += 1;
        }
        tx.commit()?;
        Ok(report)
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use zip::write::SimpleFileOptions;

    use super::*;
    use crate::content::store::{self, Row};
    use crate::content::{ContentKind, Provider, Side, ops, scan};

    const NOW: i64 = 1_000;
    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3];

    fn http() -> Http {
        Http::new()
            .unwrap()
            .with_retry_base(Duration::from_millis(1))
    }

    fn db_with_instance() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO instances (id, name, dir_name, mc_version, state, created_at)
                 VALUES ('i1', 'n', 'd', '1.21', 'ready', 0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        db
    }

    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, body) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    /// fabric.mod.json を持つ jar。
    fn mod_jar(path: &Path, name: &str) {
        let json = format!(
            r#"{{"name":"{name}","version":"0.1-local","description":"local desc","icon":"icon.png"}}"#
        );
        make_zip(
            path,
            &[("fabric.mod.json", json.as_bytes()), ("icon.png", PNG)],
        );
    }

    struct Env {
        temp: tempfile::TempDir,
        db: Db,
    }

    impl Env {
        fn new() -> Self {
            Self {
                temp: tempfile::tempdir().unwrap(),
                db: db_with_instance(),
            }
        }

        fn game(&self) -> PathBuf {
            self.temp.path().join("game")
        }

        fn icons(&self) -> PathBuf {
            self.temp.path().join("icons")
        }

        /// mod を `add_files` で入れて、その sha1 を返す。
        fn add_mod(&self, file_name: &str, name: &str) -> String {
            let source = self.temp.path().join("src").join(file_name);
            mod_jar(&source, name);
            let report = ops::add_files(
                &self.db,
                &self.game(),
                "i1",
                ContentKind::Mod,
                &[source],
                false,
            )
            .unwrap();
            report.added[0].sha1.clone()
        }

        fn rows(&self, kind: ContentKind) -> Vec<Row> {
            store::rows(&self.db, "i1", kind).unwrap()
        }

        fn row(&self, rel_path: &str) -> Row {
            self.rows(ContentKind::Mod)
                .into_iter()
                .find(|r| r.item.rel_path == rel_path)
                .unwrap()
        }

        async fn identify(&self, api: &str, batches: Batches) -> Result<IdentifyReport, CoreError> {
            identify_with(
                &self.db,
                &http(),
                api,
                &self.icons(),
                &self.game(),
                "i1",
                NOW,
                batches,
            )
            .await
        }
    }

    fn json(body: serde_json::Value) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(body)
    }

    fn version_files(hashes: &[&str]) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (i, hash) in hashes.iter().enumerate() {
            map.insert(
                hash.to_string(),
                serde_json::json!({
                    "id": format!("VER{i}"),
                    "project_id": format!("PROJ{i}"),
                    "version_number": format!("{i}.0.0"),
                    "dependencies": [
                        {"project_id": "DEP00001", "dependency_type": "required"},
                        {"project_id": "OPT00001", "dependency_type": "optional"}
                    ]
                }),
            );
        }
        serde_json::Value::Object(map)
    }

    fn project(i: usize) -> serde_json::Value {
        serde_json::json!({
            "id": format!("PROJ{i}"),
            "title": format!("Remote {i}"),
            "description": "remote desc",
            "icon_url": "https://cdn.example.com/icon.png",
            "client_side": "required",
            "server_side": "optional",
            "categories": ["optimization"],
            "additional_categories": ["utility"]
        })
    }

    async fn mount_hashes(server: &MockServer, body: serde_json::Value) {
        Mock::given(method("POST"))
            .and(path("/version_files"))
            .respond_with(json(body))
            .mount(server)
            .await;
    }

    async fn mount_projects(server: &MockServer, body: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path("/projects"))
            .respond_with(json(body))
            .mount(server)
            .await;
    }

    async fn count(server: &MockServer, route: &str) -> usize {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == route)
            .count()
    }

    #[tokio::test]
    async fn identify_writes_modrinth_fields_for_a_known_hash() {
        let env = Env::new();
        let sha1 = env.add_mod("a.jar", "Local A");
        let server = MockServer::start().await;
        mount_hashes(&server, version_files(&[&sha1])).await;
        mount_projects(&server, serde_json::json!([project(0)])).await;

        let report = env.identify(&server.uri(), BATCHES).await.unwrap();

        assert_eq!(
            report,
            IdentifyReport {
                identified: 1,
                local: 0
            }
        );
        let row = env.row("mods/a.jar");
        assert_eq!(row.item.provider, Provider::Modrinth);
        assert_eq!(row.item.project_id.as_deref(), Some("PROJ0"));
        assert_eq!(row.item.version_id.as_deref(), Some("VER0"));
        // ローカルで読んだ値は Modrinth の値で上書きされる
        assert_eq!(row.item.name.as_deref(), Some("Remote 0"));
        assert_eq!(row.item.version_number.as_deref(), Some("0.0.0"));
        assert_eq!(row.item.description.as_deref(), Some("remote desc"));
        assert_eq!(
            row.item.icon_url.as_deref(),
            Some("https://cdn.example.com/icon.png")
        );
        assert_eq!(row.item.client_side, Some(Side::Required));
        assert_eq!(row.item.server_side, Some(Side::Optional));
        assert_eq!(row.item.categories, ["optimization", "utility"]);
        assert_eq!(row.item.dependencies, ["DEP00001"]);
        assert_eq!(row.identified_at, Some(NOW));
        assert!(row.item.identified);
    }

    #[tokio::test]
    async fn identify_makes_one_hash_call_and_one_projects_call_for_many_files() {
        let env = Env::new();
        let a = env.add_mod("a.jar", "A");
        let b = env.add_mod("b.jar", "B");
        let c = env.add_mod("c.jar", "C");
        let server = MockServer::start().await;
        // a と b は同じプロジェクトに見せる
        let mut body = version_files(&[&a, &b, &c]);
        body[&b]["project_id"] = "PROJ0".into();
        mount_hashes(&server, body).await;
        mount_projects(&server, serde_json::json!([project(0), project(2)])).await;

        let report = env.identify(&server.uri(), BATCHES).await.unwrap();

        assert_eq!(report.identified, 3);
        assert_eq!(count(&server, "/version_files").await, 1);
        assert_eq!(count(&server, "/projects").await, 1);
        let requests = server.received_requests().await.unwrap();
        let projects = requests
            .iter()
            .find(|r| r.url.path() == "/projects")
            .unwrap();
        let ids = projects
            .url
            .query_pairs()
            .find(|(k, _)| k == "ids")
            .unwrap()
            .1
            .into_owned();
        let ids: Vec<String> = serde_json::from_str(&ids).unwrap();
        assert_eq!(ids, ["PROJ0", "PROJ2"]);
        assert_eq!(env.row("mods/b.jar").item.name.as_deref(), Some("Remote 0"));
    }

    #[tokio::test]
    async fn large_lists_are_split_into_batches() {
        let env = Env::new();
        let hashes: Vec<String> = (0..5)
            .map(|i| env.add_mod(&format!("m{i}.jar"), &format!("M{i}")))
            .collect();
        let refs: Vec<&str> = hashes.iter().map(String::as_str).collect();
        let server = MockServer::start().await;
        mount_hashes(&server, version_files(&refs)).await;
        mount_projects(
            &server,
            serde_json::json!((0..5).map(project).collect::<Vec<_>>()),
        )
        .await;

        let report = env
            .identify(
                &server.uri(),
                Batches {
                    hashes: 2,
                    projects: 3,
                },
            )
            .await
            .unwrap();

        assert_eq!(report.identified, 5);
        assert_eq!(count(&server, "/version_files").await, 3);
        assert_eq!(count(&server, "/projects").await, 2);
    }

    #[tokio::test]
    async fn unknown_hash_stays_local_but_is_marked_identified() {
        let env = Env::new();
        env.add_mod("a.jar", "Local A");
        let server = MockServer::start().await;
        mount_hashes(&server, serde_json::json!({})).await;

        let report = env.identify(&server.uri(), BATCHES).await.unwrap();

        assert_eq!(
            report,
            IdentifyReport {
                identified: 0,
                local: 1
            }
        );
        // 見つからなければ projects は呼ばない
        assert_eq!(count(&server, "/projects").await, 0);
        let row = env.row("mods/a.jar");
        assert_eq!(row.item.provider, Provider::Local);
        assert_eq!(row.item.project_id, None);
        assert_eq!(row.item.name.as_deref(), Some("Local A"));
        assert_eq!(row.item.version_number.as_deref(), Some("0.1-local"));
        assert_eq!(row.item.description.as_deref(), Some("local desc"));
        assert_eq!(row.identified_at, Some(NOW));
    }

    #[tokio::test]
    async fn offline_keeps_identified_at_empty_and_still_writes_local_names() {
        let env = Env::new();
        env.add_mod("a.jar", "Local A");
        // wiremock のサーバーは落としてもプールに残るので、使い終えたポートを使う
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);

        let err = env.identify(&api, BATCHES).await.unwrap_err();

        assert!(matches!(err, CoreError::Offline { .. }), "{err:?}");
        let row = env.row("mods/a.jar");
        assert_eq!(row.item.name.as_deref(), Some("Local A"));
        assert_eq!(row.item.version_number.as_deref(), Some("0.1-local"));
        assert_eq!(row.item.provider, Provider::Local);
        assert_eq!(row.identified_at, None);
        assert!(!row.item.identified);
    }

    #[tokio::test]
    async fn http_error_from_projects_keeps_identified_at_empty() {
        let env = Env::new();
        let sha1 = env.add_mod("a.jar", "Local A");
        let server = MockServer::start().await;
        mount_hashes(&server, version_files(&[&sha1])).await;
        Mock::given(method("GET"))
            .and(path("/projects"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        assert!(env.identify(&server.uri(), BATCHES).await.is_err());

        let row = env.row("mods/a.jar");
        assert_eq!(row.item.provider, Provider::Local);
        assert_eq!(row.item.name.as_deref(), Some("Local A"));
        assert_eq!(row.identified_at, None);
    }

    #[tokio::test]
    async fn identify_skips_rows_already_identified() {
        let env = Env::new();
        env.add_mod("a.jar", "Local A");
        let server = MockServer::start().await;
        mount_hashes(&server, serde_json::json!({})).await;
        env.identify(&server.uri(), BATCHES).await.unwrap();
        server.reset().await;
        mount_hashes(&server, serde_json::json!({})).await;

        let report = env.identify(&server.uri(), BATCHES).await.unwrap();

        assert_eq!(report, IdentifyReport::default());
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn local_icon_is_written_to_the_icons_dir() {
        let env = Env::new();
        let sha1 = env.add_mod("a.jar", "Local A");
        let server = MockServer::start().await;
        mount_hashes(&server, serde_json::json!({})).await;

        env.identify(&server.uri(), BATCHES).await.unwrap();

        let icon = std::fs::read(env.icons().join(format!("{sha1}.png"))).unwrap();
        assert_eq!(icon, PNG);
        let row = env.row("mods/a.jar");
        assert_eq!(row.local_icon_key.as_deref(), Some(sha1.as_str()));
        assert!(row.item.has_local_icon);
    }

    #[tokio::test]
    async fn resource_packs_are_looked_up_too() {
        let env = Env::new();
        let source = env.temp.path().join("src").join("r.zip");
        make_zip(
            &source,
            &[
                ("pack.mcmeta", br#"{"pack":{"description":"Pack desc"}}"#),
                ("pack.png", PNG),
            ],
        );
        let rp = ops::add_files(
            &env.db,
            &env.game(),
            "i1",
            ContentKind::ResourcePack,
            &[source],
            false,
        )
        .unwrap()
        .added[0]
            .sha1
            .clone();
        let server = MockServer::start().await;
        mount_hashes(&server, serde_json::json!({})).await;

        let report = env.identify(&server.uri(), BATCHES).await.unwrap();

        assert_eq!(report.local, 1);
        let row = env.rows(ContentKind::ResourcePack).remove(0);
        assert_eq!(row.item.description.as_deref(), Some("Pack desc"));
        assert_eq!(row.local_icon_key.as_deref(), Some(rp.as_str()));
        assert_eq!(row.identified_at, Some(NOW));
    }

    #[tokio::test]
    async fn a_missing_file_does_not_stop_identify() {
        let env = Env::new();
        env.add_mod("a.jar", "Local A");
        env.add_mod("b.jar", "Local B");
        std::fs::remove_file(env.game().join("mods/a.jar")).unwrap();
        let server = MockServer::start().await;
        mount_hashes(&server, serde_json::json!({})).await;

        let report = env.identify(&server.uri(), BATCHES).await.unwrap();

        assert_eq!(report.local, 2);
        assert_eq!(env.row("mods/a.jar").item.name, None);
        assert_eq!(env.row("mods/b.jar").item.name.as_deref(), Some("Local B"));
    }

    #[tokio::test]
    async fn a_file_changed_on_disk_is_looked_up_again() {
        let env = Env::new();
        let old = env.add_mod("a.jar", "Old");
        let server = MockServer::start().await;
        mount_hashes(&server, version_files(&[&old])).await;
        mount_projects(&server, serde_json::json!([project(0)])).await;
        env.identify(&server.uri(), BATCHES).await.unwrap();
        assert_eq!(env.row("mods/a.jar").item.provider, Provider::Modrinth);

        // 中身を替えて走査すると、特定の結果が捨てられる
        mod_jar(
            &env.game().join("mods/a.jar"),
            "Replaced with a longer name",
        );
        scan::scan(
            &env.db,
            &env.game(),
            "i1",
            ContentKind::Mod,
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(env.row("mods/a.jar").identified_at, None);

        let report = env.identify(&server.uri(), BATCHES).await.unwrap();

        assert_eq!(report.local, 1);
        assert_eq!(count(&server, "/version_files").await, 2);
        let row = env.row("mods/a.jar");
        assert_eq!(row.item.provider, Provider::Local);
        assert_eq!(
            row.item.name.as_deref(),
            Some("Replaced with a longer name")
        );
        assert_eq!(row.identified_at, Some(NOW));
    }
}
