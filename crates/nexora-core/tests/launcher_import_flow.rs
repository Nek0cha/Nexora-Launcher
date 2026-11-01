//! 他のランチャーからの取り込み。偽の読み込み元を Launcher に通して確かめる。
mod common;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use common::mrpack::PackHarness;
use nexora_core::error::CoreError;
use nexora_core::import::{ImportKind, ImportRequest, ImportSource, SourceInstance};
use nexora_core::instance::{self, InstanceState};
use nexora_core::meta::loader::LoaderKind;
use nexora_core::task::TaskState;

/// 一時フォルダの `<id>/<種類>/` を元として返す読み込み元。`broken` の id は読もうとすると失敗する。
struct FakeSource {
    root: PathBuf,
    rows: Vec<SourceInstance>,
    broken: HashSet<String>,
}

impl ImportSource for FakeSource {
    fn list(&self) -> Result<Vec<SourceInstance>, CoreError> {
        Ok(self.rows.clone())
    }

    fn source_path(&self, id: &str, kind: ImportKind) -> Result<Option<PathBuf>, CoreError> {
        if self.broken.contains(id) {
            return Err(CoreError::ImportUnavailable {
                reason: "unreadable".into(),
            });
        }
        let path = self.root.join(id).join(kind.rel_path());
        Ok((if kind.is_file() {
            path.is_file()
        } else {
            path.is_dir()
        })
        .then_some(path))
    }
}

fn row(id: &str, name: &str) -> SourceInstance {
    SourceInstance {
        id: id.into(),
        name: name.into(),
        mc_version: "1.99-test".into(),
        loader: LoaderKind::Fabric,
        loader_version: Some("0.16.10".into()),
        available: ImportKind::ALL.to_vec(),
        unsupported: None,
    }
}

fn put(root: &std::path::Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn source(h: &PackHarness, rows: Vec<SourceInstance>, broken: &[&str]) -> Arc<FakeSource> {
    let root = h._temp.path().join("other-launcher");
    for r in &rows {
        put(&root, &format!("{}/mods/{}.jar", r.id, r.id), b"mod");
        put(&root, &format!("{}/saves/world/level.dat", r.id), b"save");
        put(&root, &format!("{}/screenshots/s.png", r.id), b"shot");
        put(&root, &format!("{}/config/mod.toml", r.id), b"cfg");
        put(&root, &format!("{}/options.txt", r.id), b"fov:90");
        put(&root, &format!("{}/servers.dat", r.id), b"srv");
    }
    Arc::new(FakeSource {
        root,
        rows,
        broken: broken.iter().map(|s| s.to_string()).collect(),
    })
}

fn request(id: &str, kinds: &[ImportKind]) -> ImportRequest {
    ImportRequest {
        id: id.into(),
        kinds: kinds.to_vec(),
    }
}

fn game_dir(h: &PackHarness, id: &str) -> PathBuf {
    let inst = instance::get(h.launcher.db(), id).unwrap();
    instance::game_dir(&h.paths, &inst)
}

#[tokio::test]
async fn only_the_chosen_kinds_are_copied_and_the_instance_becomes_ready() {
    let h = PackHarness::new().await;
    let src = source(&h, vec![row("a", "Alpha")], &[]);

    let created = h
        .launcher
        .import_from_source(
            src.clone(),
            vec![request("a", &[ImportKind::Mods, ImportKind::Saves])],
            false,
        )
        .await
        .unwrap();
    let snap = h.wait(&created[0].task_id).await;

    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    let inst = instance::get(h.launcher.db(), &created[0].instance.id).unwrap();
    assert_eq!(
        (inst.state, inst.name.as_str()),
        (InstanceState::Ready, "Alpha")
    );
    let dir = game_dir(&h, &inst.id);
    assert_eq!(std::fs::read(dir.join("mods/a.jar")).unwrap(), b"mod");
    assert_eq!(
        std::fs::read(dir.join("saves/world/level.dat")).unwrap(),
        b"save"
    );
    assert!(!dir.join("screenshots").exists());
    assert!(
        src.root.join("a/mods/a.jar").exists(),
        "the source is untouched"
    );
}

#[tokio::test]
async fn settings_files_and_the_config_folder_are_copied_when_chosen() {
    let h = PackHarness::new().await;
    let src = source(&h, vec![row("a", "Alpha")], &[]);

    let created = h
        .launcher
        .import_from_source(
            src,
            vec![request(
                "a",
                &[ImportKind::Config, ImportKind::Options, ImportKind::Servers],
            )],
            false,
        )
        .await
        .unwrap();
    let snap = h.wait(&created[0].task_id).await;

    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    let dir = game_dir(&h, &created[0].instance.id);
    assert_eq!(std::fs::read(dir.join("config/mod.toml")).unwrap(), b"cfg");
    assert_eq!(std::fs::read(dir.join("options.txt")).unwrap(), b"fov:90");
    assert_eq!(std::fs::read(dir.join("servers.dat")).unwrap(), b"srv");
    assert!(!dir.join("saves").exists());
}

#[tokio::test]
async fn a_row_with_no_kinds_creates_just_the_instance() {
    let h = PackHarness::new().await;
    let src = source(&h, vec![row("a", "Alpha")], &[]);

    let created = h
        .launcher
        .import_from_source(src, vec![request("a", &[])], false)
        .await
        .unwrap();
    let snap = h.wait(&created[0].task_id).await;

    assert_eq!(snap.state, TaskState::Succeeded, "{:?}", snap.error);
    let inst = instance::get(h.launcher.db(), &created[0].instance.id).unwrap();
    assert_eq!(inst.state, InstanceState::Ready);
    assert!(!game_dir(&h, &inst.id).join("mods").exists());
}

#[tokio::test]
async fn a_duplicate_is_refused_unless_allowed() {
    let h = PackHarness::new().await;
    let src = source(&h, vec![row("a", "Alpha")], &[]);
    let first = h
        .launcher
        .import_from_source(src.clone(), vec![request("a", &[])], false)
        .await
        .unwrap();
    h.wait(&first[0].task_id).await;

    let found = h
        .launcher
        .find_source_duplicates(src.clone(), vec!["a".into()])
        .await
        .unwrap();
    assert_eq!(found, ["a"]);
    let err = h
        .launcher
        .import_from_source(src.clone(), vec![request("a", &[])], false)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "import.duplicate");
    assert_eq!(h.launcher.list_instances().unwrap().len(), 1);

    let second = h
        .launcher
        .import_from_source(src, vec![request("a", &[])], true)
        .await
        .unwrap();
    assert_eq!(h.wait(&second[0].task_id).await.state, TaskState::Succeeded);
    assert_eq!(h.launcher.list_instances().unwrap().len(), 2);
}

#[tokio::test]
async fn one_failing_instance_is_removed_and_the_others_still_finish() {
    let h = PackHarness::new().await;
    let src = source(&h, vec![row("a", "Alpha"), row("b", "Bravo")], &["b"]);

    let created = h
        .launcher
        .import_from_source(
            src,
            vec![
                request("a", &[ImportKind::Mods]),
                request("b", &[ImportKind::Mods]),
            ],
            false,
        )
        .await
        .unwrap();
    let a = h.wait(&created[0].task_id).await;
    let b = h.wait(&created[1].task_id).await;

    assert_eq!(a.state, TaskState::Succeeded, "{:?}", a.error);
    assert_eq!(b.state, TaskState::Failed);
    let left = h.launcher.list_instances().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].name, "Alpha");
    assert_eq!(left[0].state, InstanceState::Ready);
}

#[tokio::test]
async fn cancelling_removes_the_half_made_instance() {
    let h = PackHarness::new().await;
    let src = source(&h, vec![row("a", "Alpha")], &[]);

    let created = h
        .launcher
        .import_from_source(src, vec![request("a", &[ImportKind::Mods])], false)
        .await
        .unwrap();
    h.tasks.cancel(&created[0].task_id).unwrap();
    let snap = h.wait(&created[0].task_id).await;

    assert_eq!(snap.state, TaskState::Cancelled);
    assert!(h.launcher.list_instances().unwrap().is_empty());
    let left: Vec<_> = std::fs::read_dir(h.paths.instances_dir())
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "left behind: {left:?}");
}

#[tokio::test]
async fn an_importing_instance_cannot_be_launched() {
    let h = PackHarness::new().await;
    let src = source(&h, vec![row("a", "Alpha")], &[]);
    let created = h
        .launcher
        .import_from_source(src, vec![request("a", &[ImportKind::Mods])], false)
        .await
        .unwrap();

    let err = h
        .launcher
        .launch_instance(&created[0].instance.id, false)
        .unwrap_err();

    assert_eq!(err.code(), "instance.busy");
    h.wait(&created[0].task_id).await;
}

#[tokio::test]
async fn an_unsupported_row_or_unknown_id_is_refused_before_anything_is_made() {
    let h = PackHarness::new().await;
    let mut bad = row("a", "Alpha");
    bad.unsupported = Some("the loader is not supported".into());
    let src = source(&h, vec![bad], &[]);

    for id in ["a", "missing"] {
        let err = h
            .launcher
            .import_from_source(src.clone(), vec![request(id, &[])], false)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "import.unavailable", "{id}");
    }
    assert!(h.launcher.list_instances().unwrap().is_empty());
}

#[tokio::test]
async fn the_listing_marks_a_loader_version_the_installer_does_not_know() {
    let h = PackHarness::new().await;
    let mut odd = row("a", "Alpha");
    odd.loader_version = Some("9.9.9".into());
    let src = source(&h, vec![odd, row("b", "Bravo")], &[]);

    let list = h.launcher.list_source_instances(src).await.unwrap();

    assert!(list[0].unsupported.is_some());
    assert_eq!(list[1].unsupported, None);
}
