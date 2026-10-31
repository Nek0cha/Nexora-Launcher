//! `.mrpack` の検査とインポート。Launcher を通して確かめる。
mod common;

use common::mrpack::{PackBuilder, PackHarness};
use nexora_core::meta::loader::LoaderKind;
use serde_json::json;

#[tokio::test]
async fn inspect_returns_the_pack_summary() {
    let h = PackHarness::new().await;
    let a = h.file_entry("mods/a.jar", b"a", "/a.jar", None);
    let server_only = h.file_entry(
        "mods/s.jar",
        b"s",
        "/s.jar",
        Some(json!({ "client": "unsupported", "server": "required" })),
    );
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(PackBuilder::index(
            "1.99-test",
            Some(("fabric-loader", "0.16.10")),
            vec![a, server_only],
        )),
    );

    let info = h.launcher.inspect_mrpack(path).await.unwrap();

    assert_eq!(info.name, "Test pack");
    assert_eq!(info.version_id, "1.0.0");
    assert_eq!(info.mc_version, "1.99-test");
    assert_eq!(info.loader, LoaderKind::Fabric);
    assert_eq!(info.loader_version.as_deref(), Some("0.16.10"));
    assert_eq!(info.file_count, 1);
}

#[tokio::test]
async fn inspect_accepts_a_pack_without_a_loader() {
    let h = PackHarness::new().await;
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(PackBuilder::index("1.99-test", None, vec![])),
    );

    let info = h.launcher.inspect_mrpack(path).await.unwrap();

    assert_eq!(info.loader, LoaderKind::Vanilla);
    assert_eq!(info.loader_version, None);
}

#[tokio::test]
async fn inspect_refuses_a_loader_version_the_installer_does_not_know() {
    let h = PackHarness::new().await;
    let path = h.write_pack(
        "p.mrpack",
        PackBuilder::new().manifest_json(PackBuilder::index(
            "1.99-test",
            Some(("fabric-loader", "9.9.9")),
            vec![],
        )),
    );

    let err = h.launcher.inspect_mrpack(path).await.unwrap_err();

    assert_eq!(err.code(), "mrpack.unsupported");
}

#[tokio::test]
async fn inspect_refuses_a_broken_zip() {
    let h = PackHarness::new().await;
    let path = h.packs.join("broken.mrpack");
    std::fs::write(&path, b"not a zip").unwrap();

    let err = h.launcher.inspect_mrpack(path).await.unwrap_err();

    assert_eq!(err.code(), "mrpack.invalid");
}
