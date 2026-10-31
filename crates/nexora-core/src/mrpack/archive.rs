//! `.mrpack`（ZIP）を、上限と安全性の検査つきで読む。

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use zip::ZipArchive;
use zip::result::ZipError;

use super::Limits;
use super::manifest::{Manifest, parse_manifest};
use super::safe_path::{PathGuard, ensure_contained};
use crate::error::CoreError;
use crate::net::io_err;

const MANIFEST_NAME: &str = "modrinth.index.json";

/// 圧縮率の検査を始める大きさ。小さいファイルは、中身が単調だと圧縮率が高くなるため見ない。
const RATIO_CHECK_FROM: u64 = 1024 * 1024;

/// overrides の層。展開は `Common`、`Client` の順（後の層が上書きする）。`Server` は使わないが、名前は検査する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Common,
    Client,
    Server,
}

const LAYERS: [(&str, Layer); 3] = [
    ("overrides/", Layer::Common),
    ("client-overrides/", Layer::Client),
    ("server-overrides/", Layer::Server),
];

/// 検査を通した overrides のエントリー。
#[derive(Debug)]
struct OverrideEntry {
    index: usize,
    layer: Layer,
    rel: PathBuf,
    is_dir: bool,
    compressed: u64,
}

pub struct Pack {
    zip: ZipArchive<File>,
    limits: Limits,
    overrides: Vec<OverrideEntry>,
}

fn invalid(reason: impl Into<String>) -> CoreError {
    CoreError::MrpackInvalid {
        reason: reason.into(),
    }
}

fn limit(reason: impl Into<String>) -> CoreError {
    CoreError::MrpackLimit {
        reason: reason.into(),
    }
}

fn zip_err(err: ZipError) -> CoreError {
    invalid(format!("unreadable archive: {err}"))
}

fn exceeds_ratio(size: u64, compressed: u64, limits: &Limits) -> bool {
    size >= RATIO_CHECK_FROM && size > compressed.max(1).saturating_mul(limits.ratio)
}

/// ZIP を開き、すべてのエントリーの名前と大きさを検査する。何も書き出さない。
pub fn open(path: &Path, limits: &Limits) -> Result<Pack, CoreError> {
    let file = File::open(path).map_err(|e| io_err(path, e))?;
    let mut zip = ZipArchive::new(file).map_err(zip_err)?;
    if zip.len() > limits.entries {
        return Err(limit(format!("more than {} entries", limits.entries)));
    }
    let mut guards = [PathGuard::new(), PathGuard::new(), PathGuard::new()];
    let mut overrides = Vec::new();
    let mut total: u64 = 0;
    for index in 0..zip.len() {
        let entry = zip.by_index(index).map_err(zip_err)?;
        let name = entry.name().map_err(zip_err)?.into_owned();
        if entry.is_symlink() {
            return Err(CoreError::MrpackUnsafePath { path: name });
        }
        let (size, compressed) = (entry.size(), entry.compressed_size());
        total = total.saturating_add(size);
        if total > limits.total_uncompressed {
            return Err(limit("the archive is too large once unpacked"));
        }
        if exceeds_ratio(size, compressed, limits) {
            return Err(limit(format!("{name} is compressed suspiciously well")));
        }
        let Some((layer, rest)) = LAYERS
            .iter()
            .find_map(|(prefix, layer)| name.strip_prefix(prefix).map(|rest| (*layer, rest)))
        else {
            continue;
        };
        let rest = rest.trim_end_matches('/');
        if rest.is_empty() {
            continue;
        }
        let is_dir = entry.is_dir();
        let rel = if is_dir {
            super::safe_path::check_rel_path(rest)?
        } else {
            guards[layer as usize].admit(rest)?
        };
        overrides.push(OverrideEntry {
            index,
            layer,
            rel,
            is_dir,
            compressed,
        });
    }
    Ok(Pack {
        zip,
        limits: *limits,
        overrides,
    })
}

impl Pack {
    /// `modrinth.index.json` を読む。
    pub fn manifest(&mut self) -> Result<Manifest, CoreError> {
        let max = self.limits.manifest_bytes;
        let entry = match self.zip.by_name(MANIFEST_NAME) {
            Ok(entry) => entry,
            Err(ZipError::FileNotFound) => return Err(invalid("modrinth.index.json is missing")),
            Err(err) => return Err(zip_err(err)),
        };
        if entry.size() > max {
            return Err(limit("modrinth.index.json is too large"));
        }
        let mut bytes = Vec::new();
        entry
            .take(max + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| invalid(format!("unreadable modrinth.index.json: {e}")))?;
        if bytes.len() as u64 > max {
            return Err(limit("modrinth.index.json is too large"));
        }
        parse_manifest(&bytes)
    }

    /// `overrides/`、続けて `client-overrides/` を `dest_root` に展開する。戻り値は書いたファイルの数。
    /// 書いた実際のバイト数でも、合計と圧縮率の上限を確かめる。
    pub fn extract_overrides(
        &mut self,
        dest_root: &Path,
        token: &CancellationToken,
    ) -> Result<u32, CoreError> {
        let mut written_files = 0u32;
        let mut budget = self.limits.total_uncompressed;
        for layer in [Layer::Common, Layer::Client] {
            for i in 0..self.overrides.len() {
                if self.overrides[i].layer != layer {
                    continue;
                }
                if token.is_cancelled() {
                    return Err(CoreError::Cancelled);
                }
                let (index, is_dir, compressed) = {
                    let e = &self.overrides[i];
                    (e.index, e.is_dir, e.compressed)
                };
                let dest = dest_root.join(&self.overrides[i].rel);
                ensure_contained(dest_root, &dest)?;
                if is_dir {
                    std::fs::create_dir_all(&dest).map_err(|e| io_err(&dest, e))?;
                    continue;
                }
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
                }
                let mut entry = self.zip.by_index(index).map_err(zip_err)?;
                let mut out = File::create(&dest).map_err(|e| io_err(&dest, e))?;
                let written = copy_bounded(&mut entry, &mut out, budget, &self.limits, compressed)
                    .map_err(|e| match e {
                        CopyError::Limit(reason) => limit(reason),
                        CopyError::Read(e) => invalid(format!("unreadable entry: {e}")),
                        CopyError::Write(e) => io_err(&dest, e),
                    })?;
                budget -= written;
                written_files += 1;
            }
        }
        Ok(written_files)
    }
}

enum CopyError {
    Limit(String),
    Read(std::io::Error),
    Write(std::io::Error),
}

/// `budget` を超えず、圧縮率も超えないように書く。書いたバイト数を返す。
fn copy_bounded(
    input: &mut impl Read,
    output: &mut impl Write,
    budget: u64,
    limits: &Limits,
    compressed: u64,
) -> Result<u64, CopyError> {
    let mut buf = [0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        let n = input.read(&mut buf).map_err(CopyError::Read)?;
        if n == 0 {
            return Ok(written);
        }
        written += n as u64;
        if written > budget {
            return Err(CopyError::Limit(
                "the archive is too large once unpacked".into(),
            ));
        }
        if exceeds_ratio(written, compressed, limits) {
            return Err(CopyError::Limit(
                "an entry is compressed suspiciously well".into(),
            ));
        }
        output.write_all(&buf[..n]).map_err(CopyError::Write)?;
    }
}
