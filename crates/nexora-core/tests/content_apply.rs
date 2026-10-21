//! Modrinth の版の導入と更新（ダウンロード、退避、巻き戻し、バックアップ）。
mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use common::{http, serve};
use nexora_core::content::ContentKind;
use nexora_core::content::apply::{ApplyContext, ApplyOutcome, MODRINTH_CDN, apply_plan};
use nexora_core::content::modrinth::{Dependency, DependencyKind, FileInfo, FullVersion};
use nexora_core::content::resolve::{InstallPlan, PlannedFile};
use nexora_core::content::scan::scan;
use nexora_core::db::Db;
use nexora_core::error::CoreError;
use nexora_core::instance::{self, NewInstance};
use nexora_core::net::Http;
use nexora_core::net::hash::{hex, sha1_hex};
use nexora_core::paths::AppPaths;
use sha2::{Digest, Sha512};
use tokio_util::sync::CancellationToken;
use wiremock::MockServer;

fn sha512_hex(bytes: &[u8]) -> String {
    hex(&Sha512::digest(bytes))
}

/// DB の 1 行のうち、確かめる列。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RowView {
    rel_path: String,
    enabled: bool,
    provider: String,
    project_id: Option<String>,
    version_id: Option<String>,
    installed_as: String,
    pinned: bool,
    identified_at: Option<i64>,
    sha1: String,
    size: i64,
}

struct Env {
    _temp: tempfile::TempDir,
    db: Db,
    paths: AppPaths,
    id: String,
    game: PathBuf,
    http: Http,
    server: MockServer,
    cdn: String,
    token: CancellationToken,
}

async fn env() -> Env {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_root(temp.path());
    paths.ensure_dirs().unwrap();
    let db = Db::open_in_memory().unwrap();
    let inst = instance::create(&db, &paths, &NewInstance::vanilla("x", "1.99-test"), 0).unwrap();
    let game = instance::game_dir(&paths, &inst);
    std::fs::create_dir_all(game.join("mods")).unwrap();
    let server = MockServer::start().await;
    let cdn = server.uri();
    Env {
        _temp: temp,
        db,
        paths,
        id: inst.id,
        game,
        http: http(),
        server,
        cdn,
        token: CancellationToken::new(),
    }
}

impl Env {
    fn cx(&self) -> ApplyContext<'_> {
        ApplyContext {
            db: &self.db,
            http: &self.http,
            paths: &self.paths,
            game_dir: &self.game,
            instance_id: &self.id,
            token: &self.token,
            cdn: &self.cdn,
        }
    }

    async fn apply(&self, plan: &InstallPlan, now_ms: i64) -> Result<ApplyOutcome, CoreError> {
        apply_plan(&self.cx(), plan, now_ms, |_, _| {}).await
    }

    fn mods_dir(&self) -> PathBuf {
        self.game.join("mods")
    }

    fn put_mod(&self, name: &str, body: &[u8]) {
        std::fs::write(self.mods_dir().join(name), body).unwrap();
    }

    fn read_mod(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.mods_dir().join(name)).unwrap()
    }

    fn mods(&self) -> Vec<String> {
        names_in(&self.mods_dir())
    }

    /// ディスクを走査して行を作り、Modrinth で特定済みの行に見せかける。
    fn seed(&self, name: &str, body: &[u8], project: &str, version: &str) {
        self.put_mod(name, body);
        scan(
            &self.db,
            &self.game,
            &self.id,
            ContentKind::Mod,
            &CancellationToken::new(),
        )
        .unwrap();
        self.db
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE instance_contents SET provider = 'modrinth', project_id = ?3,
                     version_id = ?4, identified_at = 1
                     WHERE instance_id = ?1 AND rel_path = ?2",
                    rusqlite::params![self.id, format!("mods/{name}"), project, version],
                )?;
                Ok(())
            })
            .unwrap();
    }

    fn set_row(&self, rel_path: &str, installed_as: &str, pinned: bool) {
        self.db
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE instance_contents SET installed_as = ?3, pinned = ?4
                     WHERE instance_id = ?1 AND rel_path = ?2",
                    rusqlite::params![self.id, rel_path, installed_as, pinned],
                )?;
                Ok(())
            })
            .unwrap();
    }

    fn rows(&self) -> Vec<RowView> {
        self.db
            .with_conn(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT rel_path, enabled, provider, project_id, version_id, installed_as,
                            pinned, identified_at, sha1, size
                     FROM instance_contents WHERE instance_id = ?1 ORDER BY rel_path",
                )?;
                let rows = stmt.query_map([&self.id], |r| {
                    Ok(RowView {
                        rel_path: r.get(0)?,
                        enabled: r.get::<_, i64>(1)? != 0,
                        provider: r.get(2)?,
                        project_id: r.get(3)?,
                        version_id: r.get(4)?,
                        installed_as: r.get(5)?,
                        pinned: r.get::<_, i64>(6)? != 0,
                        identified_at: r.get(7)?,
                        sha1: r.get(8)?,
                        size: r.get(9)?,
                    })
                })?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            })
            .unwrap()
    }

    fn row(&self, rel_path: &str) -> RowView {
        self.rows()
            .into_iter()
            .find(|r| r.rel_path == rel_path)
            .unwrap_or_else(|| panic!("no row for {rel_path}"))
    }

    fn operations(&self) -> i64 {
        self.db
            .with_conn(|conn| {
                Ok(conn.query_row("SELECT count(*) FROM operations", [], |r| r.get(0))?)
            })
            .unwrap()
    }

    /// `backups` の行の path（古い順）。
    fn backups(&self) -> Vec<String> {
        self.db
            .with_conn(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT path FROM backups WHERE instance_id = ?1 AND reason = 'pre-update'
                     ORDER BY created_at, id",
                )?;
                let rows = stmt.query_map([&self.id], |r| r.get(0))?;
                Ok(rows.collect::<Result<Vec<String>, _>>()?)
            })
            .unwrap()
    }

    fn staging(&self) -> Vec<String> {
        names_in(&self.paths.staging_dir())
    }

    /// 何も変わっていないことを確かめる（`mods/` の中身、行、記録、staging）。
    fn assert_untouched(&self, mods: &[(&str, &[u8])], rows: &[RowView]) {
        let names: Vec<&str> = mods.iter().map(|(n, _)| *n).collect();
        assert_eq!(self.mods(), names);
        for (name, body) in mods {
            assert_eq!(self.read_mod(name), *body, "{name} changed");
        }
        assert_eq!(self.rows(), rows);
        assert_eq!(self.operations(), 0);
        assert!(self.staging().is_empty(), "staging: {:?}", self.staging());
        assert!(self.backups().is_empty());
    }

    /// project `p` の版 `id`。ファイル `filename` は CDN の `route` から `body` を返す。
    async fn version(&self, p: &str, id: &str, filename: &str, body: &[u8]) -> FullVersion {
        let route = format!("/data/{p}/versions/{id}/{filename}");
        serve(&self.server, &route, body).await;
        self.unserved_version(p, id, filename, body)
    }

    /// CDN に何も登録しない版（URL には `cdn` を使う）。
    fn unserved_version(&self, p: &str, id: &str, filename: &str, body: &[u8]) -> FullVersion {
        FullVersion {
            id: id.to_string(),
            project_id: p.to_string(),
            version_number: format!("{id}-number"),
            version_type: "release".into(),
            date_published: String::new(),
            game_versions: vec!["1.99-test".into()],
            loaders: vec!["fabric".into()],
            files: vec![FileInfo {
                url: format!("{}/data/{p}/versions/{id}/{filename}", self.cdn),
                filename: filename.to_string(),
                sha1: sha1_hex(body),
                sha512: Some(sha512_hex(body)),
                size: body.len() as u64,
                primary: true,
            }],
            dependencies: Vec::new(),
        }
    }
}

fn names_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn planned(version: FullVersion, as_dependency: bool, replaces: Option<&str>) -> PlannedFile {
    let file = version.files[0].clone();
    PlannedFile {
        version,
        file,
        as_dependency,
        replaces: replaces.map(str::to_string),
        pin: None,
    }
}

fn plan(files: Vec<PlannedFile>) -> InstallPlan {
    InstallPlan {
        files,
        ..InstallPlan::default()
    }
}

#[tokio::test]
async fn install_with_dependencies_places_all_files_and_marks_dependencies() {
    let e = env().await;
    let dep = e.version("b", "b1", "b-1.jar", b"bee").await;
    let root = e.version("a", "a1", "a-1.jar", b"ay").await;

    let outcome = e
        .apply(
            &plan(vec![planned(dep, true, None), planned(root, false, None)]),
            10,
        )
        .await
        .unwrap();

    assert_eq!(
        outcome,
        ApplyOutcome {
            installed: 1,
            dependencies_added: 1,
            replaced: 0
        }
    );
    assert_eq!(e.mods(), ["a-1.jar", "b-1.jar"]);
    assert_eq!(e.read_mod("a-1.jar"), b"ay");
    assert_eq!(e.read_mod("b-1.jar"), b"bee");
    let a = e.row("mods/a-1.jar");
    assert_eq!(
        a,
        RowView {
            rel_path: "mods/a-1.jar".into(),
            enabled: true,
            provider: "modrinth".into(),
            project_id: Some("a".into()),
            version_id: Some("a1".into()),
            installed_as: "explicit".into(),
            pinned: false,
            identified_at: None,
            sha1: sha1_hex(b"ay"),
            size: 2,
        }
    );
    let b = e.row("mods/b-1.jar");
    assert_eq!(b.installed_as, "dependency");
    assert_eq!(b.project_id.as_deref(), Some("b"));
    assert_eq!(b.version_id.as_deref(), Some("b1"));
    assert_eq!(e.rows().len(), 2);
    assert_eq!(e.operations(), 0);
    assert!(e.staging().is_empty());
    // 新規の導入ではバックアップを作らない
    assert!(e.backups().is_empty());
}

#[tokio::test]
async fn hash_mismatch_places_nothing() {
    let e = env().await;
    e.seed("x.jar", b"keep", "x", "x1");
    let before = e.rows();
    let dep = e.version("b", "b1", "b-1.jar", b"bee").await;
    let mut root = e.version("a", "a1", "a-1.jar", b"ay").await;
    root.files[0].sha1 = sha1_hex(b"something else");

    let err = e
        .apply(
            &plan(vec![planned(dep, true, None), planned(root, false, None)]),
            10,
        )
        .await
        .unwrap_err();

    assert!(matches!(err, CoreError::HashMismatch { .. }), "{err:?}");
    e.assert_untouched(&[("x.jar", b"keep")], &before);
}

#[tokio::test]
async fn sha512_mismatch_places_nothing() {
    let e = env().await;
    let mut root = e.version("a", "a1", "a-1.jar", b"ay").await;
    root.files[0].sha512 = Some(sha512_hex(b"something else"));

    let err = e
        .apply(&plan(vec![planned(root, false, None)]), 10)
        .await
        .unwrap_err();

    assert!(matches!(err, CoreError::HashMismatch { .. }), "{err:?}");
    e.assert_untouched(&[], &[]);
}

#[tokio::test]
async fn failure_in_the_middle_restores_the_replaced_jar() {
    let e = env().await;
    e.seed("a-1.jar", b"old a", "a", "a1");
    let before = e.rows();
    let dep = e.version("b", "b1", "b-1.jar", b"bee").await;
    // 2 件目（置き換える版）は CDN に無いので 404 になる
    let root = e.unserved_version("a", "a2", "a-2.jar", b"new a");

    let err = e
        .apply(
            &plan(vec![
                planned(dep, true, None),
                planned(root, false, Some("mods/a-1.jar")),
            ]),
            10,
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, CoreError::HttpStatus { status: 404, .. }),
        "{err:?}"
    );
    e.assert_untouched(&[("a-1.jar", b"old a")], &before);
}

#[tokio::test]
async fn failure_after_the_backup_restores_the_replaced_jar_and_the_disabled_dependency() {
    let e = env().await;
    e.seed("a-1.jar", b"old a", "a", "a1");
    e.seed("c.jar.disabled", b"cee", "c", "c1");
    let before = e.rows();
    let dep = e.version("b", "b1", "b-1.jar", b"bee").await;
    let root = e.version("a", "a2", "a-2.jar", b"new a").await;
    let mut p = plan(vec![
        planned(dep, true, None),
        planned(root, false, Some("mods/a-1.jar")),
    ]);
    p.enable = vec!["mods/c.jar.disabled".into()];
    // ダウンロードが終わった後に、置き場所に別のファイルが現れる（移動の段階で失敗させる）
    let intruder = e.mods_dir().join("b-1.jar");
    let placed = AtomicBool::new(false);

    let err = apply_plan(&e.cx(), &p, 10, |done, total| {
        if total > 0 && done == total && !placed.swap(true, Ordering::SeqCst) {
            std::fs::write(&intruder, b"intruder").unwrap();
        }
    })
    .await
    .unwrap_err();

    assert!(matches!(err, CoreError::ContentExists { .. }), "{err:?}");
    // 割り込んだファイルには触れず、ほかは元に戻る
    e.assert_untouched(
        &[
            ("a-1.jar", b"old a"),
            ("b-1.jar", b"intruder"),
            ("c.jar.disabled", b"cee"),
        ],
        &before,
    );
}

#[tokio::test]
async fn cancel_restores_everything() {
    let e = env().await;
    e.seed("a-1.jar", b"old a", "a", "a1");
    let before = e.rows();
    let root = e.version("a", "a2", "a-2.jar", b"new a").await;
    let p = plan(vec![planned(root, false, Some("mods/a-1.jar"))]);

    let err = apply_plan(&e.cx(), &p, 10, |_, _| e.token.cancel())
        .await
        .unwrap_err();

    assert!(matches!(err, CoreError::Cancelled), "{err:?}");
    e.assert_untouched(&[("a-1.jar", b"old a")], &before);
}

#[tokio::test]
async fn update_of_a_disabled_mod_stays_disabled() {
    let e = env().await;
    e.seed("a.jar.disabled", b"old a", "a", "a1");
    e.set_row("mods/a.jar.disabled", "dependency", true);
    let root = e.version("a", "a2", "a-2.jar", b"new a").await;

    let outcome = e
        .apply(
            &plan(vec![planned(root, false, Some("mods/a.jar.disabled"))]),
            10,
        )
        .await
        .unwrap();

    assert_eq!(
        outcome,
        ApplyOutcome {
            installed: 0,
            dependencies_added: 0,
            replaced: 1
        }
    );
    assert_eq!(e.mods(), ["a-2.jar.disabled"]);
    assert_eq!(e.read_mod("a-2.jar.disabled"), b"new a");
    let rows = e.rows();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        rows[0],
        RowView {
            rel_path: "mods/a-2.jar.disabled".into(),
            enabled: false,
            provider: "modrinth".into(),
            project_id: Some("a".into()),
            version_id: Some("a2".into()),
            // 入れた経緯と固定は元の行から引き継ぐ
            installed_as: "dependency".into(),
            pinned: true,
            identified_at: None,
            sha1: sha1_hex(b"new a"),
            size: 5,
        }
    );
    assert_eq!(e.operations(), 0);
    assert!(e.staging().is_empty());
    assert_eq!(e.backups().len(), 1);
}

/// 特定で書かれる表示の列。
#[derive(Debug, Clone, PartialEq, Eq)]
struct MetaView {
    name: Option<String>,
    description: Option<String>,
    icon_url: Option<String>,
    version_number: Option<String>,
    client_side: Option<String>,
    server_side: Option<String>,
    categories: String,
    dependencies: String,
    local_icon_key: Option<String>,
}

impl Env {
    /// 特定済みの行に、Modrinth のプロジェクトの情報（と jar の中のアイコンのキー）を書く。
    fn describe(&self, rel_path: &str, icon_url: Option<&str>, local_icon_key: Option<&str>) {
        self.db
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE instance_contents
                     SET name = 'Alpha', description = 'Alpha description', icon_url = ?3,
                         local_icon_key = ?4, client_side = 'required', server_side = 'optional',
                         categories = '[\"optimization\"]', version_number = 'old-number'
                     WHERE instance_id = ?1 AND rel_path = ?2",
                    rusqlite::params![self.id, rel_path, icon_url, local_icon_key],
                )?;
                Ok(())
            })
            .unwrap();
    }

    fn meta(&self, rel_path: &str) -> MetaView {
        self.db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT name, description, icon_url, version_number, client_side, server_side,
                            categories, dependencies, local_icon_key
                     FROM instance_contents WHERE instance_id = ?1 AND rel_path = ?2",
                    rusqlite::params![self.id, rel_path],
                    |r| {
                        Ok(MetaView {
                            name: r.get(0)?,
                            description: r.get(1)?,
                            icon_url: r.get(2)?,
                            version_number: r.get(3)?,
                            client_side: r.get(4)?,
                            server_side: r.get(5)?,
                            categories: r.get(6)?,
                            dependencies: r.get(7)?,
                            local_icon_key: r.get(8)?,
                        })
                    },
                )?)
            })
            .unwrap()
    }
}

fn required(project: &str) -> Dependency {
    Dependency {
        project_id: Some(project.into()),
        version_id: None,
        kind: DependencyKind::Required,
    }
}

#[tokio::test]
async fn update_keeps_the_project_info_and_marks_the_row_identified() {
    let e = env().await;
    e.seed("a-1.jar", b"old a", "a", "a1");
    e.describe("mods/a-1.jar", Some("https://cdn.example/a.png"), None);
    let mut root = e.version("a", "a2", "a-2.jar", b"new a").await;
    root.dependencies = vec![
        required("lib"),
        Dependency {
            project_id: Some("extra".into()),
            version_id: None,
            kind: DependencyKind::Optional,
        },
    ];

    e.apply(&plan(vec![planned(root, false, Some("mods/a-1.jar"))]), 10)
        .await
        .unwrap();

    // 更新の直後から、名前、説明、アイコンが消えずに出る（タブを開き直すまで待たない）
    assert_eq!(
        e.meta("mods/a-2.jar"),
        MetaView {
            name: Some("Alpha".into()),
            description: Some("Alpha description".into()),
            icon_url: Some("https://cdn.example/a.png".into()),
            version_number: Some("a2-number".into()),
            client_side: Some("required".into()),
            server_side: Some("optional".into()),
            categories: r#"["optimization"]"#.into(),
            // 特定と同じく、必須の依存の project だけを持つ
            dependencies: r#"["lib"]"#.into(),
            local_icon_key: None,
        }
    );
    let row = e.row("mods/a-2.jar");
    assert_eq!(row.identified_at, Some(10));
    assert_eq!(row.provider, "modrinth");
    assert_eq!(row.version_id.as_deref(), Some("a2"));
}

#[tokio::test]
async fn update_with_only_a_jar_icon_leaves_the_row_for_identify() {
    let e = env().await;
    e.seed("a-1.jar", b"old a", "a", "a1");
    // jar の中のアイコンは古い jar の sha1 で引くので、新しい jar には使えない。特定に任せる
    e.describe("mods/a-1.jar", None, Some(&sha1_hex(b"old a")));
    let root = e.version("a", "a2", "a-2.jar", b"new a").await;

    e.apply(&plan(vec![planned(root, false, Some("mods/a-1.jar"))]), 10)
        .await
        .unwrap();

    let row = e.row("mods/a-2.jar");
    assert_eq!(row.identified_at, None);
    assert_eq!(row.version_id.as_deref(), Some("a2"));
    let meta = e.meta("mods/a-2.jar");
    assert_eq!(meta.local_icon_key, None);
    assert_eq!(meta.version_number.as_deref(), Some("a2-number"));
}

#[tokio::test]
async fn new_installs_carry_the_version_and_wait_for_identify() {
    let e = env().await;
    let mut root = e.version("a", "a1", "a-1.jar", b"ay").await;
    root.dependencies = vec![required("lib")];

    e.apply(&plan(vec![planned(root, false, None)]), 10)
        .await
        .unwrap();

    let row = e.row("mods/a-1.jar");
    assert_eq!(row.identified_at, None);
    assert_eq!(row.provider, "modrinth");
    let meta = e.meta("mods/a-1.jar");
    assert_eq!(meta.name, None);
    assert_eq!(meta.version_number.as_deref(), Some("a1-number"));
    assert_eq!(meta.dependencies, r#"["lib"]"#);
}

#[tokio::test]
async fn update_to_a_file_with_the_same_name_replaces_it() {
    let e = env().await;
    e.seed("a.jar", b"old a", "a", "a1");
    let root = e.version("a", "a2", "a.jar", b"new a").await;

    let outcome = e
        .apply(&plan(vec![planned(root, false, Some("mods/a.jar"))]), 10)
        .await
        .unwrap();

    assert_eq!(outcome.replaced, 1);
    assert_eq!(e.mods(), ["a.jar"]);
    assert_eq!(e.read_mod("a.jar"), b"new a");
    let rows = e.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].version_id.as_deref(), Some("a2"));
    let backups = e.backups();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        std::fs::read(e.paths.backups_dir().join(&backups[0])).unwrap(),
        b"old a"
    );
}

#[tokio::test]
async fn update_keeps_the_old_jar_as_a_backup_and_prunes_to_five() {
    let e = env().await;
    e.seed("a-0.jar", b"body 0", "a", "a0");
    for i in 1..=6 {
        let body = format!("body {i}");
        let root = e
            .version(
                "a",
                &format!("a{i}"),
                &format!("a-{i}.jar"),
                body.as_bytes(),
            )
            .await;
        let replaced = format!("mods/a-{}.jar", i - 1);
        e.apply(&plan(vec![planned(root, false, Some(&replaced))]), i * 1000)
            .await
            .unwrap();
    }

    assert_eq!(e.mods(), ["a-6.jar"]);
    let backups = e.backups();
    assert_eq!(backups.len(), 5, "{backups:?}");
    // 残るのは a-1 から a-5 の退避で、最古の a-0 の退避はファイルも消える
    for (i, path) in backups.iter().enumerate() {
        let file = e.paths.backups_dir().join(path);
        assert_eq!(
            std::fs::read(&file).unwrap(),
            format!("body {}", i + 1).as_bytes(),
            "{path}"
        );
    }
    let dir = e.paths.backups_dir().join(&e.id);
    assert_eq!(names_in(&dir).len(), 5, "{:?}", names_in(&dir));
    assert_eq!(e.operations(), 0);
    assert!(e.staging().is_empty());
}

#[tokio::test]
async fn enable_only_entries_are_renamed_without_download() {
    let e = env().await;
    e.seed("b.jar.disabled", b"bee", "b", "b1");
    let before = e.row("mods/b.jar.disabled");
    let p = InstallPlan {
        enable: vec!["mods/b.jar.disabled".into()],
        ..InstallPlan::default()
    };

    let outcome = e.apply(&p, 10).await.unwrap();

    assert_eq!(
        outcome,
        ApplyOutcome {
            installed: 0,
            dependencies_added: 0,
            replaced: 0
        }
    );
    assert_eq!(e.mods(), ["b.jar"]);
    assert_eq!(e.read_mod("b.jar"), b"bee");
    let rows = e.rows();
    assert_eq!(
        rows,
        [RowView {
            rel_path: "mods/b.jar".into(),
            enabled: true,
            ..before
        }]
    );
    assert!(e.server.received_requests().await.unwrap().is_empty());
    assert_eq!(e.operations(), 0);
    assert!(e.staging().is_empty());
    assert!(e.backups().is_empty());
}

#[tokio::test]
async fn cdn_host_other_than_modrinth_is_refused() {
    let e = env().await;
    let mut root = e.version("a", "a1", "a-1.jar", b"ay").await;
    root.files[0].url = "https://evil.example.invalid/data/a/versions/a1/a-1.jar".into();

    let err = e
        .apply(&plan(vec![planned(root, false, None)]), 10)
        .await
        .unwrap_err();

    assert!(matches!(err, CoreError::InvalidMetadata { .. }), "{err:?}");
    assert!(e.server.received_requests().await.unwrap().is_empty());
    e.assert_untouched(&[], &[]);
}

#[tokio::test]
async fn production_cdn_accepts_only_https_cdn_modrinth_com() {
    assert_eq!(MODRINTH_CDN, "https://cdn.modrinth.com");
    let e = env().await;
    let cx = ApplyContext {
        cdn: MODRINTH_CDN,
        ..e.cx()
    };
    for url in [
        "http://cdn.modrinth.com/data/a/versions/a1/a-1.jar",
        "https://cdn.modrinth.com.evil.invalid/data/a/versions/a1/a-1.jar",
        "https://api.modrinth.com/data/a/versions/a1/a-1.jar",
        "not a url",
    ] {
        let mut root = e.unserved_version("a", "a1", "a-1.jar", b"ay");
        root.files[0].url = url.into();
        let err = apply_plan(&cx, &plan(vec![planned(root, false, None)]), 10, |_, _| {})
            .await
            .unwrap_err();
        assert!(
            matches!(err, CoreError::InvalidMetadata { .. }),
            "{url}: {err:?}"
        );
    }
    e.assert_untouched(&[], &[]);
}

#[tokio::test]
async fn unsafe_file_names_and_paths_are_refused() {
    let e = env().await;
    e.seed("x.jar", b"keep", "x", "x1");
    let before = e.rows();
    for name in [
        "../evil.jar",
        "sub/evil.jar",
        "evil.zip",
        "C:evil.jar",
        "a.jar.disabled",
    ] {
        let mut root = e.version("a", "a1", "a-1.jar", b"ay").await;
        root.files[0].filename = name.into();
        let err = e
            .apply(&plan(vec![planned(root, false, None)]), 10)
            .await
            .unwrap_err();
        assert!(
            matches!(err, CoreError::InvalidInstanceSetting { .. }),
            "{name}: {err:?}"
        );
    }
    for rel in ["mods/../x.jar", "resourcepacks/x.zip", "x.jar"] {
        let root = e.version("a", "a1", "a-1.jar", b"ay").await;
        let err = e
            .apply(&plan(vec![planned(root, false, Some(rel))]), 10)
            .await
            .unwrap_err();
        assert!(
            matches!(err, CoreError::InvalidInstanceSetting { .. }),
            "{rel}: {err:?}"
        );
        let p = InstallPlan {
            enable: vec![rel.into()],
            ..InstallPlan::default()
        };
        let err = e.apply(&p, 10).await.unwrap_err();
        assert!(
            matches!(err, CoreError::InvalidInstanceSetting { .. }),
            "{rel}: {err:?}"
        );
    }
    assert!(e.server.received_requests().await.unwrap().is_empty());
    e.assert_untouched(&[("x.jar", b"keep")], &before);
}

#[tokio::test]
async fn an_existing_file_with_the_new_name_is_not_overwritten() {
    let e = env().await;
    e.seed("a-1.jar", b"old a", "a", "a1");
    e.put_mod("a-2.jar", b"someone else's");
    scan(
        &e.db,
        &e.game,
        &e.id,
        ContentKind::Mod,
        &CancellationToken::new(),
    )
    .unwrap();
    let before = e.rows();
    let root = e.version("a", "a2", "a-2.jar", b"new a").await;

    let err = e
        .apply(&plan(vec![planned(root, false, Some("mods/a-1.jar"))]), 10)
        .await
        .unwrap_err();

    assert!(matches!(err, CoreError::ContentExists { .. }), "{err:?}");
    assert!(e.server.received_requests().await.unwrap().is_empty());
    e.assert_untouched(
        &[("a-1.jar", b"old a"), ("a-2.jar", b"someone else's")],
        &before,
    );
}
