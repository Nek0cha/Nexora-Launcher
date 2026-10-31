//! インスタンスの中身から `.mrpack` を作る。

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

use crate::content::modrinth::FileInfo;
use crate::content::{ContentItem, Side};
use crate::error::CoreError;
use crate::meta::loader::LoaderKind;
use crate::net::{io_err, part_path};

/// 書き出す 1 件。
#[derive(Debug, Clone)]
pub enum ExportEntry {
    /// Modrinth で特定できたもの。索引に URL とハッシュを書き、ファイルは入れない。
    Referenced {
        /// インスタンスの中での相対パス（`mods/a.jar`）。
        zip_path: String,
        file: FileInfo,
        env_client: Option<Side>,
        env_server: Option<Side>,
    },
    /// 特定できなかったもの。`overrides/` の中に実ファイルとして入れる。
    Embedded { zip_path: String, source: PathBuf },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExportSkips {
    /// 無効（`.disabled`）のため書き出さなかった数。
    pub disabled: u32,
}

/// `.mrpack` の `dependencies` でのローダーのキー。Vanilla は無い。
pub fn loader_dependency_key(kind: LoaderKind) -> Option<&'static str> {
    match kind {
        LoaderKind::Vanilla => None,
        LoaderKind::Fabric => Some("fabric-loader"),
        LoaderKind::Quilt => Some("quilt-loader"),
        LoaderKind::Forge => Some("forge"),
        LoaderKind::Neoforge => Some("neoforge"),
    }
}

/// このインスタンスが実際に使っているファイルは、クライアントで「非対応」とは書かない。
/// Modrinth のプロジェクトがクライアント非対応としていても、書いてしまうと、読み込み直したときに
/// そのファイルが入らなくなる（実在のパックを往復させて確かめた）。
fn client_env(side: Option<Side>) -> Option<Side> {
    match side {
        Some(Side::Unsupported) => Some(Side::Required),
        other => other,
    }
}

/// 有効な中身を、参照にするか実ファイルにするかに振り分ける。
/// `found` は SHA-1 をキーにした Modrinth のファイル情報。`sha512` と URL が揃っているものだけ参照にする。
pub fn plan_entries(
    items: &[ContentItem],
    found: &HashMap<String, FileInfo>,
    game_dir: &Path,
) -> (Vec<ExportEntry>, ExportSkips) {
    let mut entries = Vec::new();
    let mut skips = ExportSkips::default();
    for item in items {
        if !item.enabled {
            skips.disabled += 1;
            continue;
        }
        let referenced = found
            .get(&item.sha1)
            .filter(|f| f.sha512.is_some() && !f.url.is_empty());
        entries.push(match referenced {
            Some(file) => ExportEntry::Referenced {
                zip_path: item.rel_path.clone(),
                file: file.clone(),
                env_client: client_env(item.client_side),
                env_server: item.server_side,
            },
            None => ExportEntry::Embedded {
                zip_path: item.rel_path.clone(),
                source: game_dir.join(&item.rel_path),
            },
        });
    }
    (entries, skips)
}

fn zip_err(err: zip::result::ZipError) -> CoreError {
    CoreError::Unexpected {
        message: format!("failed to write the mrpack: {err}"),
    }
}

/// 片方だけ不明なときは、もう片方に合わせずに `optional`（どちらでもよい）とする。
fn env_json(client: Option<Side>, server: Option<Side>) -> Option<Value> {
    if client.is_none() && server.is_none() {
        return None;
    }
    let side = |s: Option<Side>| s.unwrap_or(Side::Optional).as_str();
    Some(json!({ "client": side(client), "server": side(server) }))
}

fn index_json(
    name: &str,
    version_id: &str,
    dependencies: &BTreeMap<String, String>,
    entries: &[ExportEntry],
) -> Value {
    let files: Vec<Value> = entries
        .iter()
        .filter_map(|entry| match entry {
            ExportEntry::Referenced {
                zip_path,
                file,
                env_client,
                env_server,
            } => {
                let mut v = json!({
                    "path": zip_path,
                    "hashes": { "sha1": file.sha1, "sha512": file.sha512 },
                    "downloads": [file.url],
                    "fileSize": file.size,
                });
                if let Some(env) = env_json(*env_client, *env_server) {
                    v["env"] = env;
                }
                Some(v)
            }
            ExportEntry::Embedded { .. } => None,
        })
        .collect();
    json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": version_id,
        "name": name,
        "files": files,
        "dependencies": dependencies,
    })
}

/// `.mrpack` を `dest` に書く。一時ファイルに書いてから移すので、失敗しても `dest` には何も残らない。
/// `config_dir` があれば、その中身を `overrides/<フォルダー名>/` に入れる（シンボリックリンクは飛ばす）。
pub fn write_pack(
    dest: &Path,
    name: &str,
    version_id: &str,
    dependencies: BTreeMap<String, String>,
    entries: &[ExportEntry],
    config_dir: Option<&Path>,
) -> Result<(), CoreError> {
    let tmp = part_path(dest);
    let result = write_zip(&tmp, name, version_id, &dependencies, entries, config_dir)
        .and_then(|()| std::fs::rename(&tmp, dest).map_err(|e| io_err(dest, e)));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn write_zip(
    path: &Path,
    name: &str,
    version_id: &str,
    dependencies: &BTreeMap<String, String>,
    entries: &[ExportEntry],
    config_dir: Option<&Path>,
) -> Result<(), CoreError> {
    let file = File::create(path).map_err(|e| io_err(path, e))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let index = index_json(name, version_id, dependencies, entries);
    zip.start_file("modrinth.index.json", options)
        .map_err(zip_err)?;
    serde_json::to_writer_pretty(&mut zip, &index).map_err(|e| CoreError::Unexpected {
        message: e.to_string(),
    })?;

    for entry in entries {
        if let ExportEntry::Embedded { zip_path, source } = entry {
            add_file(&mut zip, &format!("overrides/{zip_path}"), source, options)?;
        }
    }
    if let Some(dir) = config_dir {
        let folder = dir.file_name().and_then(|n| n.to_str()).unwrap_or("config");
        add_tree(&mut zip, dir, &format!("overrides/{folder}"), options)?;
    }
    zip.finish().map_err(zip_err)?;
    Ok(())
}

fn add_file(
    zip: &mut ZipWriter<File>,
    zip_name: &str,
    source: &Path,
    options: SimpleFileOptions,
) -> Result<(), CoreError> {
    let mut input = File::open(source).map_err(|e| io_err(source, e))?;
    zip.start_file(zip_name, options).map_err(zip_err)?;
    std::io::copy(&mut input, zip).map_err(|e| io_err(source, e))?;
    Ok(())
}

/// `dir` の中身を、名前の順に `prefix/` の下へ入れる。リンクは飛ばす。
fn add_tree(
    zip: &mut ZipWriter<File>,
    dir: &Path,
    prefix: &str,
    options: SimpleFileOptions,
) -> Result<(), CoreError> {
    let mut children: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| io_err(dir, e))?
        .collect::<Result<_, _>>()
        .map_err(|e| io_err(dir, e))?;
    children.sort_by_key(|c| c.file_name());
    for child in children {
        let path = child.path();
        let meta = std::fs::symlink_metadata(&path).map_err(|e| io_err(&path, e))?;
        if meta.file_type().is_symlink() {
            continue;
        }
        let Some(name) = child.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let zip_name = format!("{prefix}/{name}");
        if meta.is_dir() {
            add_tree(zip, &path, &zip_name, options)?;
        } else if meta.is_file() {
            add_file(zip, &zip_name, &path, options)?;
        }
    }
    Ok(())
}
