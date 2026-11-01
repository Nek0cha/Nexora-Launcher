//! jar の中のメタデータから MOD ID を読む。診断で、ログに出た ID と入っている jar を結びつけるために使う。

use std::fs::File;
use std::path::Path;

use serde_json::Value;
use zip::ZipArchive;

use crate::content::local_meta::{read_entry, strip_bom};

fn str_of(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// `provides` は文字列か、`{"id": ...}` の配列。
fn push_provides(ids: &mut Vec<String>, provides: Option<&Value>) {
    for item in provides.and_then(Value::as_array).into_iter().flatten() {
        let id = match item {
            Value::String(s) => Some(s.trim()),
            other => str_of(other.get("id")),
        };
        if let Some(id) = id.filter(|s| !s.is_empty()) {
            ids.push(id.to_string());
        }
    }
}

fn read_json(zip: &mut ZipArchive<File>, name: &str) -> Option<Value> {
    let bytes = read_entry(zip, name)?;
    serde_json::from_slice(strip_bom(&bytes)).ok()
}

/// jar に入っている MOD ID。読めない、壊れている、メタデータが無い場合は空。
pub fn read_mod_ids(jar: &Path) -> Vec<String> {
    let Ok(file) = File::open(jar) else {
        return Vec::new();
    };
    let Ok(mut zip) = ZipArchive::new(file) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    if let Some(v) = read_json(&mut zip, "fabric.mod.json") {
        if let Some(id) = str_of(v.get("id")) {
            ids.push(id.to_string());
        }
        push_provides(&mut ids, v.get("provides"));
        return ids;
    }
    if let Some(v) = read_json(&mut zip, "quilt.mod.json") {
        let loader = v.get("quilt_loader");
        if let Some(id) = str_of(loader.and_then(|l| l.get("id"))) {
            ids.push(id.to_string());
        }
        push_provides(&mut ids, loader.and_then(|l| l.get("provides")));
        return ids;
    }
    for entry in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        let Some(bytes) = read_entry(&mut zip, entry) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(strip_bom(&bytes)) else {
            continue;
        };
        let Ok(table) = text.parse::<toml::Table>() else {
            continue;
        };
        let mods = table.get("mods").and_then(|m| m.as_array());
        for m in mods.into_iter().flatten() {
            if let Some(id) = str_of(m.get("modId")) {
                ids.push(id.to_string());
            }
        }
        if !ids.is_empty() {
            return ids;
        }
    }
    if let Some(v) = read_json(&mut zip, "mcmod.info") {
        // 配列か、`modList` を持つ辞書
        let list = match &v {
            Value::Array(list) => Some(list),
            other => other.get("modList").and_then(Value::as_array),
        };
        for m in list.into_iter().flatten() {
            if let Some(id) = str_of(m.get("modid")) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

/// `a.b.C` のクラスが jar の `a/b/C.class` にあるか。読めなければ false。
pub fn jar_defines_class(jar: &Path, class_name: &str) -> bool {
    let Ok(file) = File::open(jar) else {
        return false;
    };
    let Ok(mut zip) = ZipArchive::new(file) else {
        return false;
    };
    let entry = format!("{}.class", class_name.replace('.', "/"));
    zip.by_name(&entry).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make_jar(entries: &[(&str, &[u8])]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.jar");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, body) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
        (dir, path)
    }

    #[test]
    fn reads_fabric_id_and_provides() {
        let (_d, jar) = make_jar(&[(
            "fabric.mod.json",
            br#"{"id":"sodium","provides":["rubidium","indium"],"version":"1"}"#,
        )]);
        assert_eq!(read_mod_ids(&jar), ["sodium", "rubidium", "indium"]);
    }

    #[test]
    fn reads_quilt_id_and_provides() {
        let (_d, jar) = make_jar(&[(
            "quilt.mod.json",
            br#"{"quilt_loader":{"id":"qsl","provides":[{"id":"qfapi"},"x"]}}"#,
        )]);
        assert_eq!(read_mod_ids(&jar), ["qsl", "qfapi", "x"]);
    }

    #[test]
    fn reads_every_mod_id_of_a_forge_mods_toml() {
        let toml = b"modLoader=\"javafml\"\n[[mods]]\nmodId=\"create\"\n[[mods]]\nmodId=\"flywheel\"\n";
        let (_d, jar) = make_jar(&[("META-INF/mods.toml", toml)]);
        assert_eq!(read_mod_ids(&jar), ["create", "flywheel"]);
    }

    #[test]
    fn reads_neoforge_mods_toml() {
        let toml = b"[[mods]]\nmodId=\"neomod\"\n";
        let (_d, jar) = make_jar(&[("META-INF/neoforge.mods.toml", toml)]);
        assert_eq!(read_mod_ids(&jar), ["neomod"]);
    }

    #[test]
    fn reads_legacy_mcmod_info_array() {
        let (_d, jar) = make_jar(&[("mcmod.info", br#"[{"modid":"oldmod"},{"modid":"second"}]"#)]);
        assert_eq!(read_mod_ids(&jar), ["oldmod", "second"]);
    }

    #[test]
    fn broken_or_missing_jar_gives_no_ids() {
        let dir = tempfile::tempdir().unwrap();
        let broken = dir.path().join("broken.jar");
        std::fs::write(&broken, b"not a zip").unwrap();
        assert!(read_mod_ids(&broken).is_empty());
        assert!(read_mod_ids(&dir.path().join("missing.jar")).is_empty());
        let (_d, no_meta) = make_jar(&[("readme.txt", b"hi")]);
        assert!(read_mod_ids(&no_meta).is_empty());
    }

    #[test]
    fn jar_defines_class_matches_the_entry_name() {
        let (_d, jar) = make_jar(&[("net/example/Foo.class", b"x")]);
        assert!(jar_defines_class(&jar, "net.example.Foo"));
        assert!(!jar_defines_class(&jar, "net.example.Bar"));
        assert!(!jar_defines_class(&jar.with_extension("none"), "net.example.Foo"));
    }
}
