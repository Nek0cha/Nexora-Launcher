//! インスタンスの中身から `.mrpack` を作る。書いたものを、インポート側の読み取りで読み直して確かめる。
mod common;

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::Path;

use nexora_core::content::modrinth::FileInfo;
use nexora_core::content::{ContentItem, ContentKind, InstalledAs, Provider, Side};
use nexora_core::meta::loader::LoaderKind;
use nexora_core::mrpack::Limits;
use nexora_core::mrpack::archive::open;
use nexora_core::mrpack::export::{ExportEntry, loader_dependency_key, plan_entries, write_pack};
use serde_json::Value;

const SHA512: &str = "b";

fn item(
    rel_path: &str,
    enabled: bool,
    sha1: &str,
    sides: (Option<Side>, Option<Side>),
) -> ContentItem {
    let file_name = rel_path.rsplit('/').next().unwrap().to_string();
    ContentItem {
        kind: ContentKind::Mod,
        rel_path: rel_path.to_string(),
        file_name,
        enabled,
        size: 3,
        sha1: sha1.to_string(),
        provider: Provider::Local,
        project_id: None,
        version_id: None,
        pinned: false,
        installed_as: InstalledAs::Explicit,
        name: None,
        version_number: None,
        description: None,
        icon_url: None,
        has_local_icon: false,
        client_side: sides.0,
        server_side: sides.1,
        categories: Vec::new(),
        dependencies: Vec::new(),
        identified: false,
    }
}

fn file_info(sha1: &str, url: &str) -> FileInfo {
    FileInfo {
        url: url.to_string(),
        filename: "x.jar".into(),
        sha1: sha1.to_string(),
        sha512: Some(SHA512.repeat(128)),
        size: 3,
        primary: true,
    }
}

fn deps(mc: &str, loader: Option<(&str, &str)>) -> BTreeMap<String, String> {
    let mut d = BTreeMap::new();
    d.insert("minecraft".to_string(), mc.to_string());
    if let Some((k, v)) = loader {
        d.insert(k.to_string(), v.to_string());
    }
    d
}

/// 書いた `.mrpack` の索引の JSON をそのまま読む。
fn raw_index(pack: &Path) -> Value {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(pack).unwrap()).unwrap();
    let mut text = String::new();
    zip.by_name("modrinth.index.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn zip_names(pack: &Path) -> Vec<String> {
    let zip = zip::ZipArchive::new(std::fs::File::open(pack).unwrap()).unwrap();
    let mut names: Vec<String> = zip
        .file_names()
        .filter_map(Result::ok)
        .map(|n| n.into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn plan_entries_splits_identified_unidentified_and_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let items = vec![
        item(
            "mods/known.jar",
            true,
            "k1",
            (Some(Side::Required), Some(Side::Required)),
        ),
        item("mods/unknown.jar", true, "u1", (None, None)),
        item("mods/off.jar.disabled", false, "d1", (None, None)),
    ];
    let mut found = HashMap::new();
    found.insert(
        "k1".to_string(),
        file_info("k1", "https://cdn.modrinth.com/data/a/b.jar"),
    );

    let (entries, skips) = plan_entries(&items, &found, dir.path());

    assert_eq!(skips.disabled, 1);
    assert_eq!(entries.len(), 2);
    match &entries[0] {
        ExportEntry::Referenced {
            zip_path,
            file,
            env_client,
            env_server,
        } => {
            assert_eq!(zip_path, "mods/known.jar");
            assert_eq!(file.sha1, "k1");
            assert_eq!(*env_client, Some(Side::Required));
            assert_eq!(*env_server, Some(Side::Required));
        }
        other => panic!("expected a reference, got {other:?}"),
    }
    match &entries[1] {
        ExportEntry::Embedded { zip_path, source } => {
            assert_eq!(zip_path, "mods/unknown.jar");
            assert_eq!(source, &dir.path().join("mods/unknown.jar"));
        }
        other => panic!("expected an embedded file, got {other:?}"),
    }
}

#[test]
fn a_known_file_without_a_sha512_is_embedded_instead() {
    let dir = tempfile::tempdir().unwrap();
    let items = vec![item("mods/a.jar", true, "k1", (None, None))];
    let mut info = file_info("k1", "https://cdn.modrinth.com/data/a/b.jar");
    info.sha512 = None;
    let found = HashMap::from([("k1".to_string(), info)]);

    let (entries, _) = plan_entries(&items, &found, dir.path());

    assert!(matches!(entries[0], ExportEntry::Embedded { .. }));
}

#[test]
fn written_pack_round_trips_through_the_importer_parser() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("mods")).unwrap();
    std::fs::write(dir.path().join("mods/unknown.jar"), b"jar").unwrap();
    let entries = vec![
        ExportEntry::Referenced {
            zip_path: "mods/known.jar".into(),
            file: file_info(
                "a".repeat(40).as_str(),
                "https://cdn.modrinth.com/data/a/b.jar",
            ),
            env_client: Some(Side::Required),
            env_server: Some(Side::Unsupported),
        },
        ExportEntry::Embedded {
            zip_path: "mods/unknown.jar".into(),
            source: dir.path().join("mods/unknown.jar"),
        },
    ];
    let dest = dir.path().join("out.mrpack");

    write_pack(
        &dest,
        "My pack",
        "2.0.0",
        deps("1.21.1", Some(("fabric-loader", "0.16.10"))),
        &entries,
        None,
    )
    .unwrap();

    let mut pack = open(&dest, &Limits::DEFAULT).unwrap();
    let manifest = pack.manifest().unwrap();
    assert_eq!(manifest.name, "My pack");
    assert_eq!(manifest.version_id, "2.0.0");
    let target = manifest.target().unwrap();
    assert_eq!(target.mc_version, "1.21.1");
    assert_eq!(target.loader, LoaderKind::Fabric);
    assert_eq!(manifest.files.len(), 1);
    assert_eq!(manifest.files[0].path, "mods/known.jar");
    assert_eq!(manifest.files[0].sha1, "a".repeat(40));
    assert_eq!(
        manifest.files[0].downloads,
        ["https://cdn.modrinth.com/data/a/b.jar"]
    );
    assert!(zip_names(&dest).contains(&"overrides/mods/unknown.jar".to_string()));
}

#[test]
fn env_is_copied_from_client_and_server_sides() {
    let dir = tempfile::tempdir().unwrap();
    let mk = |client, server| ExportEntry::Referenced {
        zip_path: "mods/a.jar".into(),
        file: file_info("a".repeat(40).as_str(), "https://cdn.modrinth.com/a"),
        env_client: client,
        env_server: server,
    };
    let cases = [
        (
            mk(Some(Side::Required), Some(Side::Optional)),
            Some(("required", "optional")),
        ),
        (
            mk(Some(Side::Unsupported), Some(Side::Required)),
            Some(("unsupported", "required")),
        ),
        (
            mk(Some(Side::Required), None),
            Some(("required", "optional")),
        ),
        (mk(None, None), None),
    ];
    for (entry, expected) in cases {
        let dest = dir.path().join("e.mrpack");
        write_pack(&dest, "P", "1", deps("1.21.1", None), &[entry], None).unwrap();
        let file = &raw_index(&dest)["files"][0];
        match expected {
            Some((client, server)) => {
                assert_eq!(file["env"]["client"], client);
                assert_eq!(file["env"]["server"], server);
            }
            None => assert!(file.get("env").is_none()),
        }
    }
}

#[test]
fn config_is_included_only_when_asked() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    std::fs::create_dir_all(config.join("sub")).unwrap();
    std::fs::write(config.join("a.toml"), b"a").unwrap();
    std::fs::write(config.join("sub/b.toml"), b"b").unwrap();

    let without = dir.path().join("without.mrpack");
    write_pack(&without, "P", "1", deps("1.21.1", None), &[], None).unwrap();
    assert!(
        !zip_names(&without)
            .iter()
            .any(|n| n.starts_with("overrides/"))
    );

    let with = dir.path().join("with.mrpack");
    write_pack(&with, "P", "1", deps("1.21.1", None), &[], Some(&config)).unwrap();
    let names = zip_names(&with);
    assert!(names.contains(&"overrides/config/a.toml".to_string()));
    assert!(names.contains(&"overrides/config/sub/b.toml".to_string()));
}

#[cfg(unix)]
#[test]
fn config_symlinks_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(dir.path().join("secret.txt"), b"s").unwrap();
    std::os::unix::fs::symlink(dir.path().join("secret.txt"), config.join("link.txt")).unwrap();
    let dest = dir.path().join("out.mrpack");
    write_pack(&dest, "P", "1", deps("1.21.1", None), &[], Some(&config)).unwrap();
    assert!(!zip_names(&dest).iter().any(|n| n.ends_with("link.txt")));
}

#[test]
fn a_failed_write_leaves_no_file_at_dest() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.mrpack");
    let entries = vec![ExportEntry::Embedded {
        zip_path: "mods/missing.jar".into(),
        source: dir.path().join("mods/missing.jar"),
    }];

    let err = write_pack(&dest, "P", "1", deps("1.21.1", None), &entries, None).unwrap_err();

    assert_eq!(err.code(), "io.failed");
    let left: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().collect();
    assert!(left.is_empty(), "left behind: {left:?}");
}

#[test]
fn vanilla_instance_exports_without_a_loader_key() {
    assert_eq!(loader_dependency_key(LoaderKind::Vanilla), None);
    assert_eq!(
        loader_dependency_key(LoaderKind::Fabric),
        Some("fabric-loader")
    );
    assert_eq!(
        loader_dependency_key(LoaderKind::Quilt),
        Some("quilt-loader")
    );
    assert_eq!(loader_dependency_key(LoaderKind::Forge), Some("forge"));
    assert_eq!(
        loader_dependency_key(LoaderKind::Neoforge),
        Some("neoforge")
    );

    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("v.mrpack");
    write_pack(&dest, "P", "1", deps("1.20.4", None), &[], None).unwrap();
    let mut pack = open(&dest, &Limits::DEFAULT).unwrap();
    let target = pack.manifest().unwrap().target().unwrap();
    assert_eq!(target.loader, LoaderKind::Vanilla);
}
