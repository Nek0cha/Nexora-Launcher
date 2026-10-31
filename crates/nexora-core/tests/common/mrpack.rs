//! テスト用の `.mrpack` を組み立てる。

use std::io::Write;
use std::path::Path;

use serde_json::{Value, json};
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

enum Entry {
    File(Vec<u8>),
    Symlink(String),
}

#[derive(Default)]
pub struct PackBuilder {
    entries: Vec<(String, Entry)>,
}

impl PackBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// `modrinth.index.json` を入れる。
    pub fn manifest_json(self, value: Value) -> Self {
        self.file("modrinth.index.json", &serde_json::to_vec(&value).unwrap())
    }

    /// 名前はそのまま ZIP に書く（`overrides/../x` のような危険な名前も書ける）。
    pub fn file(mut self, zip_name: &str, bytes: &[u8]) -> Self {
        self.entries
            .push((zip_name.to_string(), Entry::File(bytes.to_vec())));
        self
    }

    pub fn symlink(mut self, zip_name: &str, target: &str) -> Self {
        self.entries
            .push((zip_name.to_string(), Entry::Symlink(target.to_string())));
        self
    }

    pub fn write(self, path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, entry) in self.entries {
            match entry {
                Entry::File(bytes) => {
                    zip.start_file(name, options).unwrap();
                    zip.write_all(&bytes).unwrap();
                }
                Entry::Symlink(target) => {
                    zip.add_symlink(name, target, options).unwrap();
                }
            }
        }
        zip.finish().unwrap();
    }

    /// 索引の JSON。`loader` は `("fabric-loader", "0.16.10")` のような組。
    pub fn index(mc: &str, loader: Option<(&str, &str)>, files: Vec<Value>) -> Value {
        let mut deps = serde_json::Map::new();
        deps.insert("minecraft".into(), json!(mc));
        if let Some((key, version)) = loader {
            deps.insert(key.into(), json!(version));
        }
        json!({
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1.0.0",
            "name": "Test pack",
            "files": files,
            "dependencies": deps,
        })
    }
}
