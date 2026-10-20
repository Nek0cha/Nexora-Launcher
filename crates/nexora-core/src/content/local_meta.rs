//! mod の jar やリソースパックの zip の中にあるメタデータ（名前、版、説明、アイコン）を読む。

use std::fs::File;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

use super::ContentKind;

/// zip の 1 エントリから読む上限（zip 爆弾への備え）。
const MAX_ENTRY_BYTES: u64 = 4 * 1024 * 1024;
/// 採用するアイコンの上限。
const MAX_ICON_BYTES: usize = 1024 * 1024;
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// ファイルの中から読めたメタデータ。読めなかった項目は `None`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalMeta {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub icon_png: Option<Vec<u8>>,
}

/// メタデータを読む。失敗はしない（読めない、壊れている、無い場合は空の `LocalMeta`）。
pub fn read_local_meta(kind: ContentKind, path: &Path) -> LocalMeta {
    let Ok(file) = File::open(path) else {
        return LocalMeta::default();
    };
    let Ok(mut zip) = ZipArchive::new(file) else {
        return LocalMeta::default();
    };
    match kind {
        ContentKind::Mod => read_mod(&mut zip),
        ContentKind::ResourcePack => read_resource_pack(&mut zip),
        ContentKind::Shader => LocalMeta::default(),
    }
}

/// エントリを 4 MiB までだけ読む。上限を超えるものは読まない。
fn read_entry(zip: &mut ZipArchive<File>, name: &str) -> Option<Vec<u8>> {
    let entry = zip.by_name(name).ok()?;
    if entry.size() > MAX_ENTRY_BYTES {
        return None;
    }
    let mut buf = Vec::new();
    entry.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > MAX_ENTRY_BYTES {
        return None;
    }
    Some(buf)
}

/// png のシグネチャを持ち、1 MiB 以下のものだけ返す。
fn read_icon(zip: &mut ZipArchive<File>, name: &str) -> Option<Vec<u8>> {
    let name = name.trim_start_matches("./").trim_start_matches('/');
    let bytes = read_entry(zip, name)?;
    (bytes.len() <= MAX_ICON_BYTES && bytes.starts_with(&PNG_SIGNATURE)).then_some(bytes)
}

fn non_empty(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

/// `${file.jarVersion}` のような置き換え前の版は捨てる。
fn clean_version(s: Option<&str>) -> Option<String> {
    non_empty(s).filter(|v| !v.contains("${"))
}

fn read_mod(zip: &mut ZipArchive<File>) -> LocalMeta {
    if let Some(bytes) = read_entry(zip, "fabric.mod.json")
        && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes)
    {
        let icon = v.get("icon").and_then(|i| i.as_str()).map(String::from);
        return LocalMeta {
            name: non_empty(v.get("name").and_then(|x| x.as_str())),
            version: clean_version(v.get("version").and_then(|x| x.as_str())),
            description: non_empty(v.get("description").and_then(|x| x.as_str())),
            icon_png: icon.and_then(|p| read_icon(zip, &p)),
        };
    }
    if let Some(bytes) = read_entry(zip, "quilt.mod.json")
        && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes)
    {
        let loader = v.get("quilt_loader");
        let meta = loader.and_then(|l| l.get("metadata"));
        let field = |key: &str| meta.and_then(|m| m.get(key)).and_then(|x| x.as_str());
        let icon = field("icon").map(String::from);
        return LocalMeta {
            name: non_empty(field("name")),
            version: clean_version(
                loader
                    .and_then(|l| l.get("version"))
                    .and_then(|x| x.as_str()),
            ),
            description: non_empty(field("description")),
            icon_png: icon.and_then(|p| read_icon(zip, &p)),
        };
    }
    for entry in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        let Some(bytes) = read_entry(zip, entry) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        let Ok(table) = text.parse::<toml::Table>() else {
            continue;
        };
        let first = table
            .get("mods")
            .and_then(|m| m.as_array())
            .and_then(|a| a.first())
            .and_then(|m| m.as_table());
        let Some(first) = first else {
            continue;
        };
        let field = |key: &str| first.get(key).and_then(|x| x.as_str());
        let icon = field("logoFile").map(String::from);
        return LocalMeta {
            name: non_empty(field("displayName")),
            version: clean_version(field("version")),
            description: non_empty(field("description")),
            icon_png: icon.and_then(|p| read_icon(zip, &p)),
        };
    }
    LocalMeta::default()
}

fn read_resource_pack(zip: &mut ZipArchive<File>) -> LocalMeta {
    let description = read_entry(zip, "pack.mcmeta")
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| {
            let d = v.get("pack")?.get("description")?;
            let text = match d {
                serde_json::Value::String(s) => s.as_str(),
                other => other.get("text")?.as_str()?,
            };
            non_empty(text.lines().next())
        });
    LocalMeta {
        description,
        icon_png: read_icon(zip, "pack.png"),
        ..LocalMeta::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, body) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    fn png(extra: usize) -> Vec<u8> {
        let mut v = PNG_SIGNATURE.to_vec();
        v.extend(std::iter::repeat_n(0u8, extra));
        v
    }

    #[test]
    fn reads_fabric_mod_json() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.jar");
        let icon = png(4);
        make_zip(
            &p,
            &[
                (
                    "fabric.mod.json",
                    br#"{"name":"Sodium","version":"0.6.0","description":"Fast","icon":"assets/i.png"}"#,
                ),
                ("assets/i.png", &icon),
            ],
        );
        let m = read_local_meta(ContentKind::Mod, &p);
        assert_eq!(m.name.as_deref(), Some("Sodium"));
        assert_eq!(m.version.as_deref(), Some("0.6.0"));
        assert_eq!(m.description.as_deref(), Some("Fast"));
        assert_eq!(m.icon_png, Some(icon));
    }

    #[test]
    fn reads_quilt_mod_json() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("q.jar");
        make_zip(
            &p,
            &[(
                "quilt.mod.json",
                br#"{"quilt_loader":{"version":"1.2.3","metadata":{"name":"Qm","description":"D"}}}"#,
            )],
        );
        let m = read_local_meta(ContentKind::Mod, &p);
        assert_eq!(m.name.as_deref(), Some("Qm"));
        assert_eq!(m.version.as_deref(), Some("1.2.3"));
        assert_eq!(m.description.as_deref(), Some("D"));
        assert_eq!(m.icon_png, None);
    }

    #[test]
    fn reads_mods_toml_and_neoforge_mods_toml() {
        let dir = tempfile::tempdir().unwrap();
        let toml = br#"
modLoader = "javafml"
[[mods]]
modId = "x"
displayName = "Xmod"
version = "2.0"
description = "Desc"
logoFile = "logo.png"
[[mods]]
modId = "y"
displayName = "Second"
"#;
        let icon = png(2);
        for entry in ["META-INF/mods.toml", "META-INF/neoforge.mods.toml"] {
            let p = dir.path().join("f.jar");
            make_zip(&p, &[(entry, toml), ("logo.png", &icon)]);
            let m = read_local_meta(ContentKind::Mod, &p);
            assert_eq!(m.name.as_deref(), Some("Xmod"), "{entry}");
            assert_eq!(m.version.as_deref(), Some("2.0"));
            assert_eq!(m.description.as_deref(), Some("Desc"));
            assert_eq!(m.icon_png.as_deref(), Some(icon.as_slice()));
        }
    }

    #[test]
    fn drops_placeholder_versions() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("p.jar");
        make_zip(
            &p,
            &[(
                "META-INF/mods.toml",
                b"[[mods]]\ndisplayName = \"P\"\nversion = \"${file.jarVersion}\"\n",
            )],
        );
        let m = read_local_meta(ContentKind::Mod, &p);
        assert_eq!(m.name.as_deref(), Some("P"));
        assert_eq!(m.version, None);
    }

    #[test]
    fn reads_resource_pack_mcmeta_and_icon() {
        let dir = tempfile::tempdir().unwrap();
        let icon = png(3);
        let p = dir.path().join("r.zip");
        make_zip(
            &p,
            &[
                (
                    "pack.mcmeta",
                    br#"{"pack":{"pack_format":15,"description":"Line one\nLine two"}}"#,
                ),
                ("pack.png", &icon),
            ],
        );
        let m = read_local_meta(ContentKind::ResourcePack, &p);
        assert_eq!(m.description.as_deref(), Some("Line one"));
        assert_eq!(m.icon_png, Some(icon));

        let p2 = dir.path().join("r2.zip");
        make_zip(
            &p2,
            &[(
                "pack.mcmeta",
                br#"{"pack":{"description":{"text":"Obj\nsecond"}}}"#,
            )],
        );
        let m2 = read_local_meta(ContentKind::ResourcePack, &p2);
        assert_eq!(m2.description.as_deref(), Some("Obj"));
        assert_eq!(m2.icon_png, None);
    }

    #[test]
    fn shader_reads_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.zip");
        make_zip(&p, &[("pack.mcmeta", br#"{"pack":{"description":"x"}}"#)]);
        assert_eq!(
            read_local_meta(ContentKind::Shader, &p),
            LocalMeta::default()
        );
    }

    #[test]
    fn broken_zip_gives_empty_meta() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("b.jar");
        std::fs::write(&p, b"not a zip").unwrap();
        assert_eq!(read_local_meta(ContentKind::Mod, &p), LocalMeta::default());
        assert_eq!(
            read_local_meta(ContentKind::Mod, &dir.path().join("missing.jar")),
            LocalMeta::default()
        );
    }

    #[test]
    fn oversized_or_non_png_icon_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let json = br#"{"name":"N","icon":"i.png"}"#;
        let big = png(MAX_ICON_BYTES);
        let p = dir.path().join("big.jar");
        make_zip(&p, &[("fabric.mod.json", json), ("i.png", &big)]);
        let m = read_local_meta(ContentKind::Mod, &p);
        assert_eq!(m.name.as_deref(), Some("N"));
        assert_eq!(m.icon_png, None);

        let p2 = dir.path().join("jpg.jar");
        make_zip(
            &p2,
            &[("fabric.mod.json", json), ("i.png", b"\xff\xd8\xff")],
        );
        assert_eq!(read_local_meta(ContentKind::Mod, &p2).icon_png, None);
    }

    #[test]
    fn oversized_entry_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("o.jar");
        let mut body = br#"{"name":"Huge"}"#.to_vec();
        body.resize(MAX_ENTRY_BYTES as usize + 10, b' ');
        make_zip(&p, &[("fabric.mod.json", &body)]);
        assert_eq!(read_local_meta(ContentKind::Mod, &p), LocalMeta::default());
    }
}
