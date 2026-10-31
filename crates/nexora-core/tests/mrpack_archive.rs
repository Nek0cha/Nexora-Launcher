//! `.mrpack` の ZIP を、上限と安全性の検査つきで読む。
mod common;

use common::mrpack::PackBuilder;
use nexora_core::mrpack::Limits;
use nexora_core::mrpack::archive::open;
use tokio_util::sync::CancellationToken;

fn simple_index() -> serde_json::Value {
    PackBuilder::index("1.21.1", Some(("fabric-loader", "0.16.10")), vec![])
}

fn pack_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join("test.mrpack")
}

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn reads_the_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let path = pack_path(&dir);
    PackBuilder::new()
        .manifest_json(simple_index())
        .write(&path);

    let mut pack = open(&path, &Limits::DEFAULT).unwrap();
    let manifest = pack.manifest().unwrap();
    assert_eq!(manifest.name, "Test pack");
}

#[test]
fn extracts_overrides_then_client_overrides_with_the_later_layer_winning() {
    let dir = tempfile::tempdir().unwrap();
    let path = pack_path(&dir);
    PackBuilder::new()
        .manifest_json(simple_index())
        .file("overrides/config/a.txt", b"common")
        .file("overrides/config/only-common.txt", b"c")
        .file("client-overrides/config/a.txt", b"client")
        .file("server-overrides/config/server.txt", b"server")
        .write(&path);
    let dest = dir.path().join("inst");
    std::fs::create_dir_all(&dest).unwrap();

    let mut pack = open(&path, &Limits::DEFAULT).unwrap();
    let written = pack
        .extract_overrides(&dest, &CancellationToken::new())
        .unwrap();

    assert_eq!(written, 3);
    assert_eq!(read(&dest.join("config/a.txt")), "client");
    assert_eq!(read(&dest.join("config/only-common.txt")), "c");
    assert!(!dest.join("config/server.txt").exists());
}

#[test]
fn rejects_zip_slip_entries_before_writing_anything() {
    for name in [
        "overrides/../evil.txt",
        "overrides/a/../../evil.txt",
        "client-overrides/..\\evil.txt",
        "overrides/C:/evil.txt",
        "server-overrides/../evil.txt",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = pack_path(&dir);
        PackBuilder::new()
            .manifest_json(simple_index())
            .file("overrides/ok.txt", b"ok")
            .file(name, b"x")
            .write(&path);

        let err = open(&path, &Limits::DEFAULT).err().expect(name);
        assert_eq!(err.code(), "mrpack.unsafe_path", "{name}");
        assert!(!dir.path().join("evil.txt").exists(), "{name}");
    }
}

#[test]
fn rejects_symlink_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = pack_path(&dir);
    PackBuilder::new()
        .manifest_json(simple_index())
        .symlink("overrides/link", "/etc")
        .write(&path);
    let err = open(&path, &Limits::DEFAULT).err().unwrap();
    assert_eq!(err.code(), "mrpack.unsafe_path");
}

#[test]
fn rejects_case_colliding_entries_in_one_layer() {
    let dir = tempfile::tempdir().unwrap();
    let path = pack_path(&dir);
    PackBuilder::new()
        .manifest_json(simple_index())
        .file("overrides/config/A.txt", b"1")
        .file("overrides/config/a.txt", b"2")
        .write(&path);
    let err = open(&path, &Limits::DEFAULT).err().unwrap();
    assert_eq!(err.code(), "mrpack.unsafe_path");
}

#[test]
fn enforces_limits() {
    let big = vec![b'a'; 1024 * 1024];
    let base = PackBuilder::new;

    let cases: Vec<(&str, PackBuilder, Limits)> = vec![
        (
            "entries",
            base()
                .manifest_json(simple_index())
                .file("overrides/1", b"1")
                .file("overrides/2", b"2")
                .file("overrides/3", b"3"),
            Limits {
                entries: 3,
                ..Limits::DEFAULT
            },
        ),
        (
            "total size",
            base()
                .manifest_json(simple_index())
                .file("overrides/big", &big),
            Limits {
                total_uncompressed: 1024,
                ..Limits::DEFAULT
            },
        ),
        (
            "ratio",
            base()
                .manifest_json(simple_index())
                .file("overrides/big", &big),
            Limits {
                ratio: 10,
                ..Limits::DEFAULT
            },
        ),
    ];
    for (label, builder, limits) in cases {
        let dir = tempfile::tempdir().unwrap();
        let path = pack_path(&dir);
        builder.write(&path);
        let err = open(&path, &limits).err().expect(label);
        assert_eq!(err.code(), "mrpack.limit", "{label}");
    }
}

#[test]
fn manifest_larger_than_the_limit_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = pack_path(&dir);
    PackBuilder::new()
        .manifest_json(simple_index())
        .write(&path);
    let limits = Limits {
        manifest_bytes: 10,
        ..Limits::DEFAULT
    };
    let mut pack = open(&path, &limits).unwrap();
    assert_eq!(pack.manifest().err().unwrap().code(), "mrpack.limit");
}

#[test]
fn missing_manifest_is_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let path = pack_path(&dir);
    PackBuilder::new()
        .file("overrides/a.txt", b"a")
        .write(&path);
    let mut pack = open(&path, &Limits::DEFAULT).unwrap();
    assert_eq!(pack.manifest().err().unwrap().code(), "mrpack.invalid");
}

#[test]
fn a_file_that_is_not_a_zip_is_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let path = pack_path(&dir);
    std::fs::write(&path, b"this is not a zip").unwrap();
    assert_eq!(
        open(&path, &Limits::DEFAULT).err().unwrap().code(),
        "mrpack.invalid"
    );
}

#[test]
fn extraction_stops_when_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let path = pack_path(&dir);
    PackBuilder::new()
        .manifest_json(simple_index())
        .file("overrides/a.txt", b"a")
        .write(&path);
    let dest = dir.path().join("inst");
    std::fs::create_dir_all(&dest).unwrap();
    let token = CancellationToken::new();
    token.cancel();
    let mut pack = open(&path, &Limits::DEFAULT).unwrap();
    let err = pack.extract_overrides(&dest, &token).unwrap_err();
    assert_eq!(err.code(), "task.cancelled");
}
