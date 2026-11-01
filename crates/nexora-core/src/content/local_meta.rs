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
pub(crate) fn read_entry(zip: &mut ZipArchive<File>, name: &str) -> Option<Vec<u8>> {
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

/// 先頭の UTF-8 BOM（EF BB BF）を外す。メモ帳などで保存されたメタデータに付いていることがある。
pub(crate) fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

/// png のシグネチャを持ち、1 MiB 以下のものだけ返す。
fn read_icon(zip: &mut ZipArchive<File>, name: &str) -> Option<Vec<u8>> {
    let name = name.trim_start_matches("./").trim_start_matches('/');
    let bytes = read_entry(zip, name)?;
    (bytes.len() <= MAX_ICON_BYTES && bytes.starts_with(&PNG_SIGNATURE)).then_some(bytes)
}

/// アイコンの指定を読む。文字列ならそのパス、`{"16": "...", "128": "..."}` のようなサイズ別の辞書なら
/// 大きいものから順に試して、最初に読めたものを使う。
fn read_icon_spec(zip: &mut ZipArchive<File>, spec: &serde_json::Value) -> Option<Vec<u8>> {
    match spec {
        serde_json::Value::String(path) => read_icon(zip, path),
        serde_json::Value::Object(map) => {
            let mut sized: Vec<(u32, &str)> = map
                .iter()
                .filter_map(|(size, path)| Some((size.parse().ok()?, path.as_str()?)))
                .collect();
            sized.sort_by_key(|&(size, _)| std::cmp::Reverse(size));
            sized.into_iter().find_map(|(_, path)| read_icon(zip, path))
        }
        _ => None,
    }
}

/// アイコンが宣言されていない（または読めない）jar で、画像が置かれていることの多い場所を探す。
fn fallback_icon(zip: &mut ZipArchive<File>, mod_id: Option<&str>) -> Option<Vec<u8>> {
    let mut candidates = Vec::new();
    if let Some(id) = mod_id.map(str::trim).filter(|id| !id.is_empty()) {
        candidates.push(format!("assets/{id}/icon.png"));
        candidates.push(format!("assets/{id}/logo.png"));
    }
    candidates.extend(["icon.png", "logo.png", "pack.png"].map(String::from));
    candidates.iter().find_map(|path| read_icon(zip, path))
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
        && let Ok(v) = serde_json::from_slice::<serde_json::Value>(strip_bom(&bytes))
    {
        let icon = v
            .get("icon")
            .and_then(|spec| read_icon_spec(zip, spec))
            .or_else(|| fallback_icon(zip, v.get("id").and_then(|x| x.as_str())));
        return LocalMeta {
            name: non_empty(v.get("name").and_then(|x| x.as_str())),
            version: clean_version(v.get("version").and_then(|x| x.as_str())),
            description: non_empty(v.get("description").and_then(|x| x.as_str())),
            icon_png: icon,
        };
    }
    if let Some(bytes) = read_entry(zip, "quilt.mod.json")
        && let Ok(v) = serde_json::from_slice::<serde_json::Value>(strip_bom(&bytes))
    {
        let loader = v.get("quilt_loader");
        let meta = loader.and_then(|l| l.get("metadata"));
        let field = |key: &str| meta.and_then(|m| m.get(key)).and_then(|x| x.as_str());
        let icon = meta
            .and_then(|m| m.get("icon"))
            .and_then(|spec| read_icon_spec(zip, spec))
            .or_else(|| {
                fallback_icon(
                    zip,
                    loader.and_then(|l| l.get("id")).and_then(|x| x.as_str()),
                )
            });
        return LocalMeta {
            name: non_empty(field("name")),
            version: clean_version(
                loader
                    .and_then(|l| l.get("version"))
                    .and_then(|x| x.as_str()),
            ),
            description: non_empty(field("description")),
            icon_png: icon,
        };
    }
    for entry in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        let Some(bytes) = read_entry(zip, entry) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(strip_bom(&bytes)) else {
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
        let icon = field("logoFile")
            .and_then(|path| read_icon(zip, path))
            .or_else(|| fallback_icon(zip, field("modId")));
        return LocalMeta {
            name: non_empty(field("displayName")),
            version: clean_version(field("version")),
            description: non_empty(field("description")),
            icon_png: icon,
        };
    }
    // 1.12 以前の Forge。配列か、`modList` を持つ辞書
    if let Some(bytes) = read_entry(zip, "mcmod.info")
        && let Ok(v) = serde_json::from_slice::<serde_json::Value>(strip_bom(&bytes))
    {
        let first = match &v {
            serde_json::Value::Array(list) => list.first(),
            other => other
                .get("modList")
                .and_then(|list| list.as_array())
                .and_then(|list| list.first()),
        };
        if let Some(first) = first {
            let field = |key: &str| first.get(key).and_then(|x| x.as_str());
            let icon = field("logoFile")
                .and_then(|path| read_icon(zip, path))
                .or_else(|| fallback_icon(zip, field("modid")));
            return LocalMeta {
                name: non_empty(field("name")),
                version: clean_version(field("version")),
                description: non_empty(field("description")),
                icon_png: icon,
            };
        }
    }
    LocalMeta::default()
}

fn read_resource_pack(zip: &mut ZipArchive<File>) -> LocalMeta {
    let description = read_entry(zip, "pack.mcmeta")
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(strip_bom(&b)).ok())
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
    fn fabric_icon_given_per_size_uses_the_largest_one_that_loads() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.jar");
        let (small, large) = (png(1), png(2));
        make_zip(
            &p,
            &[
                (
                    "fabric.mod.json",
                    br#"{"id":"m","name":"N","icon":{"16":"i16.png","128":"i128.png","x":"bad.png"}}"#,
                ),
                ("i16.png", &small),
                ("i128.png", &large),
            ],
        );
        assert_eq!(read_local_meta(ContentKind::Mod, &p).icon_png, Some(large));

        // 大きい方が読めなければ、次に大きいものを使う
        make_zip(
            &p,
            &[
                (
                    "fabric.mod.json",
                    br#"{"id":"m","name":"N","icon":{"16":"i16.png","128":"missing.png"}}"#,
                ),
                ("i16.png", &small),
            ],
        );
        assert_eq!(read_local_meta(ContentKind::Mod, &p).icon_png, Some(small));
    }

    #[test]
    fn quilt_icon_given_per_size_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.jar");
        let icon = png(3);
        make_zip(
            &p,
            &[
                (
                    "quilt.mod.json",
                    br#"{"quilt_loader":{"id":"q","version":"1.0","metadata":{"name":"Q","icon":{"64":"q.png"}}}}"#,
                ),
                ("q.png", &icon),
            ],
        );
        assert_eq!(read_local_meta(ContentKind::Mod, &p).icon_png, Some(icon));
    }

    #[test]
    fn a_mod_without_a_declared_icon_falls_back_to_the_usual_places() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.jar");
        let (under_assets, root) = (png(4), png(5));
        let json = br#"{"id":"mymod","name":"N"}"#;

        make_zip(
            &p,
            &[
                ("fabric.mod.json", json),
                ("assets/mymod/icon.png", &under_assets),
                ("icon.png", &root),
            ],
        );
        assert_eq!(
            read_local_meta(ContentKind::Mod, &p).icon_png,
            Some(under_assets)
        );

        make_zip(&p, &[("fabric.mod.json", json), ("logo.png", &root)]);
        assert_eq!(
            read_local_meta(ContentKind::Mod, &p).icon_png,
            Some(root.clone())
        );

        // 宣言したアイコンが読めれば、そちらを使う
        let declared = png(6);
        make_zip(
            &p,
            &[
                (
                    "fabric.mod.json",
                    br#"{"id":"mymod","name":"N","icon":"d.png"}"#,
                ),
                ("d.png", &declared),
                ("icon.png", &root),
            ],
        );
        assert_eq!(
            read_local_meta(ContentKind::Mod, &p).icon_png,
            Some(declared)
        );

        // PNG でないものは使わない
        make_zip(&p, &[("fabric.mod.json", json), ("icon.png", b"not a png")]);
        assert_eq!(read_local_meta(ContentKind::Mod, &p).icon_png, None);
    }

    #[test]
    fn reads_legacy_forge_mcmod_info_as_a_list_or_as_a_mod_list() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.jar");
        let icon = png(7);
        let list = br#"[{"modid":"old","name":"Old Mod","version":"1.2","description":"Very old","logoFile":"logo.png"}]"#;
        make_zip(&p, &[("mcmod.info", list), ("logo.png", &icon)]);
        let m = read_local_meta(ContentKind::Mod, &p);
        assert_eq!(m.name.as_deref(), Some("Old Mod"));
        assert_eq!(m.version.as_deref(), Some("1.2"));
        assert_eq!(m.description.as_deref(), Some("Very old"));
        assert_eq!(m.icon_png, Some(icon.clone()));

        let wrapped = br#"{"modListVersion":2,"modList":[{"modid":"old","name":"Wrapped","version":"${version}"}]}"#;
        make_zip(
            &p,
            &[("mcmod.info", wrapped), ("assets/old/logo.png", &icon)],
        );
        let m = read_local_meta(ContentKind::Mod, &p);
        assert_eq!(m.name.as_deref(), Some("Wrapped"));
        assert_eq!(m.version, None);
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

    fn with_bom(body: &[u8]) -> Vec<u8> {
        let mut v = b"\xEF\xBB\xBF".to_vec();
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn reads_json_metadata_with_a_utf8_bom() {
        let dir = tempfile::tempdir().unwrap();
        let fabric = dir.path().join("f.jar");
        make_zip(
            &fabric,
            &[("fabric.mod.json", &with_bom(br#"{"name":"Fab"}"#))],
        );
        assert_eq!(
            read_local_meta(ContentKind::Mod, &fabric).name.as_deref(),
            Some("Fab")
        );

        let quilt = dir.path().join("q.jar");
        make_zip(
            &quilt,
            &[(
                "quilt.mod.json",
                &with_bom(br#"{"quilt_loader":{"metadata":{"name":"Qui"}}}"#),
            )],
        );
        assert_eq!(
            read_local_meta(ContentKind::Mod, &quilt).name.as_deref(),
            Some("Qui")
        );

        let pack = dir.path().join("r.zip");
        make_zip(
            &pack,
            &[(
                "pack.mcmeta",
                &with_bom(br#"{"pack":{"description":"Packed"}}"#),
            )],
        );
        assert_eq!(
            read_local_meta(ContentKind::ResourcePack, &pack)
                .description
                .as_deref(),
            Some("Packed")
        );
    }

    #[test]
    fn reads_toml_metadata_with_a_utf8_bom() {
        let dir = tempfile::tempdir().unwrap();
        let body = with_bom(b"[[mods]]\ndisplayName = \"Tom\"\n");
        for entry in ["META-INF/mods.toml", "META-INF/neoforge.mods.toml"] {
            let p = dir.path().join("t.jar");
            make_zip(&p, &[(entry, &body)]);
            assert_eq!(
                read_local_meta(ContentKind::Mod, &p).name.as_deref(),
                Some("Tom"),
                "{entry}"
            );
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
