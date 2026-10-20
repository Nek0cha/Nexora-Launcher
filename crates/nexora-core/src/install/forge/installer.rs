//! Forge と NeoForge のインストーラ（jar）の取得と読み取り。
//! インストーラは信頼しない入力として扱い、エントリ名が展開先の外を指すものは何も書かずに断る。

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use zip::ZipArchive;

use crate::error::CoreError;
use crate::install::plan::is_safe_relative;
use crate::meta::write_atomic;
use crate::net::hash::{sha1_file, sha1_hex};
use crate::net::{Http, io_err, lock_path};
use crate::paths::AppPaths;

/// 1 エントリとして読む大きさの上限。インストーラの中身は大きくても数十 MiB で、
/// これを超えるものは壊れているか、展開で容量を食いつぶす細工とみなす。
const MAX_ENTRY_SIZE: u64 = 512 * 1024 * 1024;

/// 取得済みのインストーラ。
#[derive(Debug, Clone)]
pub struct Installer {
    pub path: PathBuf,
    /// 照合済みの sha1（小文字の 16 進）。
    pub sha1: String,
}

fn invalid(message: String) -> CoreError {
    CoreError::InvalidMetadata { message }
}

/// 保存先の 1 区切りとして認める値か。英数字と `.`、`_`、`-`、`+` だけからなり、`.` と `..` ではないもの。
/// `%` で符号化されたものも弾く。
fn is_safe_cache_segment(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
}

/// URL を、`installers_dir` からの相対パス（`<host>[_<port>]/<URL のパス>`）にする。http と https だけを認める。
/// ファイル名だけで分けると、Forge と NeoForge の 1.20.1 系（どちらも `forge-1.20.1-...-installer.jar`）が
/// 同じ場所を取り合うため、取得元の全体で分ける。
fn installer_rel_path(url: &str) -> Result<String, CoreError> {
    let unsafe_url = || invalid(format!("installer url {url:?} has an unsafe path"));
    let parsed =
        reqwest::Url::parse(url).map_err(|e| invalid(format!("installer url {url:?}: {e}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(invalid(format!("installer url {url:?} is not http")));
    }
    let host = parsed.host_str().unwrap_or_default();
    let host = match parsed.port() {
        Some(port) => format!("{host}_{port}"),
        None => host.to_string(),
    };
    let mut parts = vec![host];
    parts.extend(
        parsed
            .path_segments()
            .ok_or_else(unsafe_url)?
            .map(str::to_string),
    );
    // ホストと、少なくとも 1 つのファイル名が要る
    if parts.len() < 2 || !parts.iter().all(|s| is_safe_cache_segment(s)) {
        return Err(unsafe_url());
    }
    let rel = parts.join("/");
    if is_safe_relative(&rel) {
        Ok(rel)
    } else {
        Err(unsafe_url())
    }
}

/// `.sha1` の中身から sha1 を取り出す。先頭の空白で区切った最初の語を、大文字小文字を区別せずに読む。
fn parse_sha1(body: &[u8], url: &str) -> Result<String, CoreError> {
    let text = String::from_utf8_lossy(body);
    let hash = text.split_whitespace().next().unwrap_or_default();
    if hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(hash.to_ascii_lowercase())
    } else {
        Err(invalid(format!("{url} does not contain a sha1")))
    }
}

fn remove_if_exists(path: &Path) -> Result<(), CoreError> {
    match std::fs::remove_file(path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(io_err(path, err)),
        _ => Ok(()),
    }
}

/// 取得済みのインストーラと、照合済みの sha1 の記録が一致するなら、その sha1 を返す。
fn cached_sha1(path: &Path, record: &Path) -> Option<String> {
    let recorded = parse_sha1(&std::fs::read(record).ok()?, "").ok()?;
    let actual = sha1_file(path).ok()?;
    (actual == recorded).then_some(actual)
}

/// 取り消されたら `Cancelled` にする。
async fn cancellable<T>(
    token: &CancellationToken,
    fut: impl std::future::Future<Output = Result<T, CoreError>>,
) -> Result<T, CoreError> {
    tokio::select! {
        _ = token.cancelled() => Err(CoreError::Cancelled),
        r = fut => r,
    }
}

/// `cache/installers/<host>[_<port>]/<URL のパス>` に取得する。`<url>.sha1` を取得して照合し、合わなければファイルを残さず `HashMismatch`。
/// 取得済みで sha1 が一致するなら、ネットワークに出ない。`.sha1` の取得に失敗した場合は導入を失敗にする（照合なしでは取り込まない）。
pub async fn fetch_installer(
    http: &Http,
    paths: &AppPaths,
    url: &str,
    token: &CancellationToken,
) -> Result<Installer, CoreError> {
    if token.is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    let rel = installer_rel_path(url)?;
    let path = paths.installers_dir().join(&rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|s| io_err(dir, s))?;
    }
    // 照合済みの sha1 の記録。インストーラと揃って在るときだけ、取得を省く
    let record = paths.installers_dir().join(format!("{rel}.sha1"));

    // 同じインストーラを取りに行くほかのタスクが終わるまで待つ
    let _guard = cancellable(token, async { Ok(lock_path(&path).await) }).await?;
    if let Some(sha1) = cached_sha1(&path, &record) {
        return Ok(Installer { path, sha1 });
    }

    let sidecar_url = format!("{url}.sha1");
    let sidecar = cancellable(token, http.get_bytes(&sidecar_url)).await?;
    let expected = parse_sha1(&sidecar, &sidecar_url)?;
    let bytes = cancellable(token, http.get_bytes(url)).await?;
    if sha1_hex(&bytes) != expected {
        // 前回の取得が残っていても、照合できないものは残さない
        remove_if_exists(&record)?;
        remove_if_exists(&path)?;
        return Err(CoreError::HashMismatch {
            path: path.display().to_string(),
        });
    }
    // 記録はインストーラの後に書く。途中で止まっても、次は取り直しになるだけ
    remove_if_exists(&record)?;
    write_atomic(&path, &bytes)?;
    write_atomic(&record, expected.as_bytes())?;
    Ok(Installer {
        path,
        sha1: expected,
    })
}

fn open(installer: &Path) -> Result<ZipArchive<File>, CoreError> {
    let file = File::open(installer).map_err(|s| io_err(installer, s))?;
    ZipArchive::new(file).map_err(|e| invalid(format!("{}: {e}", installer.display())))
}

/// エントリの中身を、上限まで読む。
fn read_limited(entry: &mut impl Read, installer: &Path, name: &str) -> Result<Vec<u8>, CoreError> {
    let mut bytes = Vec::new();
    entry
        .take(MAX_ENTRY_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| invalid(format!("{}: {name}: {e}", installer.display())))?;
    if bytes.len() as u64 > MAX_ENTRY_SIZE {
        return Err(invalid(format!(
            "{}: {name} is too large",
            installer.display()
        )));
    }
    Ok(bytes)
}

/// 名前のエントリを読む。無ければ `InvalidMetadata`。
fn read_named(
    archive: &mut ZipArchive<File>,
    installer: &Path,
    name: &str,
) -> Result<Vec<u8>, CoreError> {
    let mut entry = archive
        .by_name(name)
        .map_err(|e| invalid(format!("{}: {name}: {e}", installer.display())))?;
    read_limited(&mut entry, installer, name)
}

/// jar の中の 1 エントリを読む。名前の先頭の `/` は取り除く。無ければ `InvalidMetadata`。
pub fn read_entry(installer: &Path, name: &str) -> Result<Vec<u8>, CoreError> {
    let name = name.strip_prefix('/').unwrap_or(name);
    read_named(&mut open(installer)?, installer, name)
}

/// `maven/<path>` のエントリを `libraries_dir/<path>` に展開し、インストーラが置くパス（書いたもの、または既に在ったもの）を返す。
/// 既存のファイルは上書きしない（同じ版の再導入のため）。取り消されたら、次のエントリの前に `Cancelled` にする。
pub fn extract_maven(
    installer: &Path,
    libraries_dir: &Path,
    token: &CancellationToken,
) -> Result<Vec<PathBuf>, CoreError> {
    let mut archive = open(installer)?;
    // 先にすべての名前を確かめ、1 つでも安全でなければ何も書かない
    let mut files = Vec::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| invalid(format!("{}: {e}", installer.display())))?;
        let name = entry
            .name()
            .map_err(|e| invalid(format!("{}: {e}", installer.display())))?
            .into_owned();
        let Some(rel) = name.strip_prefix("maven/") else {
            continue;
        };
        let is_dir = entry.is_dir();
        let rel = if is_dir {
            rel.strip_suffix('/').unwrap_or(rel)
        } else {
            rel
        };
        if is_dir && rel.is_empty() {
            continue;
        }
        if !is_safe_relative(rel) {
            return Err(invalid(format!(
                "{}: entry {name:?} has an unsafe path",
                installer.display()
            )));
        }
        if !is_dir {
            files.push((name.clone(), rel.to_string()));
        }
    }

    // 書いたもの、または既に在ったもの
    let mut provided = Vec::new();
    for (name, rel) in files {
        if token.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let dest = libraries_dir.join(&rel);
        if std::fs::symlink_metadata(&dest).is_err() {
            let bytes = read_named(&mut archive, installer, &name)?;
            write_atomic(&dest, &bytes)?;
        }
        provided.push(dest);
    }
    Ok(provided)
}

/// jar の中の 1 エントリを `dest` に展開する。既にあるファイルは上書きしない（同じ版の再導入のため）。
/// 名前が安全でない、またはエントリが無ければ、何も書かずに `InvalidMetadata`。
pub fn extract_entry_to(installer: &Path, entry: &str, dest: &Path) -> Result<(), CoreError> {
    if !is_safe_relative(entry) {
        return Err(invalid(format!(
            "{}: entry {entry:?} has an unsafe path",
            installer.display()
        )));
    }
    let bytes = read_named(&mut open(installer)?, installer, entry)?;
    if std::fs::symlink_metadata(dest).is_err() {
        write_atomic(dest, &bytes)?;
    }
    Ok(())
}

/// `/data/client.lzma` のような資源を `dest_dir/<名前>` に展開してパスを返す。
pub fn extract_resource(
    installer: &Path,
    name: &str,
    dest_dir: &Path,
) -> Result<PathBuf, CoreError> {
    let rel = name.strip_prefix('/').unwrap_or(name);
    if !is_safe_relative(rel) {
        return Err(invalid(format!(
            "{}: resource {name:?} has an unsafe path",
            installer.display()
        )));
    }
    let bytes = read_named(&mut open(installer)?, installer, rel)?;
    let dest = dest_dir.join(rel);
    write_atomic(&dest, &bytes)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make_jar(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, body) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    /// dir の下にあるファイルを、dir からの相対パス（/ 区切り）で返す。
    fn files_under(dir: &Path) -> Vec<String> {
        fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(base, &path, out);
                } else {
                    let rel = path.strip_prefix(base).unwrap();
                    out.push(
                        rel.components()
                            .map(|c| c.as_os_str().to_string_lossy().into_owned())
                            .collect::<Vec<_>>()
                            .join("/"),
                    );
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    #[test]
    fn read_entry_accepts_a_leading_slash() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("i.jar");
        make_jar(
            &jar,
            &[("version.json", b"{}"), ("install_profile.json", b"[]")],
        );
        assert_eq!(read_entry(&jar, "/version.json").unwrap(), b"{}");
        assert_eq!(read_entry(&jar, "version.json").unwrap(), b"{}");
        assert_eq!(read_entry(&jar, "install_profile.json").unwrap(), b"[]");
    }

    #[test]
    fn read_entry_missing_or_corrupt_is_invalid_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("i.jar");
        make_jar(&jar, &[("version.json", b"{}")]);
        assert_eq!(
            read_entry(&jar, "/missing.json").unwrap_err().code(),
            "meta.invalid"
        );
        let bad = temp.path().join("bad.jar");
        std::fs::write(&bad, b"not a zip").unwrap();
        assert_eq!(
            read_entry(&bad, "version.json").unwrap_err().code(),
            "meta.invalid"
        );
    }

    #[test]
    fn extract_maven_writes_into_the_libraries_dir() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("i.jar");
        make_jar(
            &jar,
            &[
                ("maven/a/b/c.jar", b"lib"),
                ("data/client.lzma", b"patch"),
                ("version.json", b"{}"),
            ],
        );
        let libs = temp.path().join("libraries");
        let written = extract_maven(&jar, &libs, &CancellationToken::new()).unwrap();
        assert_eq!(written, vec![libs.join("a").join("b").join("c.jar")]);
        assert_eq!(std::fs::read(&written[0]).unwrap(), b"lib");
        assert_eq!(files_under(&libs), vec!["a/b/c.jar"]);
    }

    #[test]
    fn extract_maven_does_not_overwrite_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("i.jar");
        make_jar(&jar, &[("maven/a/b/c.jar", b"new")]);
        let libs = temp.path().join("libraries");
        let target = libs.join("a").join("b").join("c.jar");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"old").unwrap();
        extract_maven(&jar, &libs, &CancellationToken::new()).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
    }

    #[test]
    fn extract_maven_rejects_unsafe_entries_without_writing() {
        for evil in [
            "maven/../../evil",
            r"maven/a\b",
            "maven//abs",
            "maven/C:/evil",
            "maven/C:evil",
            "maven/a/./b",
            "maven/a/../b",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let jar = temp.path().join("i.jar");
            // 安全なエントリを先に置き、それも書かれないことを確かめる
            make_jar(&jar, &[("maven/ok/ok.jar", b"ok"), (evil, b"evil")]);
            let libs = temp.path().join("root").join("libraries");
            let err = extract_maven(&jar, &libs, &CancellationToken::new()).unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "{evil}");
            assert!(files_under(&temp.path().join("root")).is_empty(), "{evil}");
            assert!(!temp.path().join("evil").exists(), "{evil}");
        }
    }

    #[test]
    fn extract_resource_writes_under_dest() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("i.jar");
        make_jar(&jar, &[("data/client.lzma", b"patch")]);
        let dest = temp.path().join("work");
        let path = extract_resource(&jar, "/data/client.lzma", &dest).unwrap();
        assert_eq!(path, dest.join("data").join("client.lzma"));
        assert_eq!(std::fs::read(&path).unwrap(), b"patch");
    }

    #[test]
    fn extract_resource_rejects_unsafe_names_without_writing() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("i.jar");
        make_jar(
            &jar,
            &[
                ("../evil", b"x"),
                (r"data\evil", b"x"),
                ("C:/evil", b"x"),
                ("data/client.lzma", b"ok"),
            ],
        );
        let dest = temp.path().join("root").join("work");
        for name in [
            "../evil",
            "/../evil",
            r"data\evil",
            "C:/evil",
            "//abs",
            "",
            "/",
        ] {
            let err = extract_resource(&jar, name, &dest).unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "{name:?}");
        }
        assert!(files_under(&temp.path().join("root")).is_empty());
        assert!(!temp.path().join("evil").exists());
    }

    #[test]
    fn extract_resource_missing_entry_is_invalid_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("i.jar");
        make_jar(&jar, &[("data/client.lzma", b"ok")]);
        let err = extract_resource(&jar, "/data/server.lzma", &temp.path().join("w")).unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
    }

    #[test]
    fn extract_entry_to_writes_the_entry() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("i.jar");
        make_jar(&jar, &[("forge-x-universal.jar", b"jar"), ("other", b"no")]);
        let dest = temp.path().join("libs/net/x/forge-x.jar");
        extract_entry_to(&jar, "forge-x-universal.jar", &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"jar");
    }

    #[test]
    fn extract_entry_to_keeps_an_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("i.jar");
        make_jar(&jar, &[("a.jar", b"new")]);
        let dest = temp.path().join("a.jar.out");
        std::fs::write(&dest, b"old").unwrap();
        extract_entry_to(&jar, "a.jar", &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"old");
    }

    #[test]
    fn extract_entry_to_rejects_unsafe_and_missing_entries() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("i.jar");
        make_jar(&jar, &[("../x", b"x"), ("ok.jar", b"ok")]);
        let dest = temp.path().join("out/f.jar");
        for name in ["../x", "/x", r"a", "C:/x", "", "missing.jar"] {
            let err = extract_entry_to(&jar, name, &dest).unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "{name:?}");
        }
        assert!(!dest.exists());
    }
}
