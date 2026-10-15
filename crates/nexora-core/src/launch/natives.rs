//! 旧形式の natives の jar を展開する。

use std::path::Path;

use crate::error::CoreError;
use crate::install::plan::NativeJar;
use crate::net::io_err;

/// dest を空にしてから、各 jar の中身を展開する。exclude に前方一致する項目と、dest の外を指す項目は飛ばす。
pub fn extract_natives(jars: &[NativeJar], dest: &Path) -> Result<(), CoreError> {
    if dest.exists() {
        std::fs::remove_dir_all(dest).map_err(|s| io_err(dest, s))?;
    }
    std::fs::create_dir_all(dest).map_err(|s| io_err(dest, s))?;
    for jar in jars {
        let file = std::fs::File::open(&jar.path).map_err(|s| io_err(&jar.path, s))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| CoreError::InvalidMetadata {
            message: format!("{}: {e}", jar.path.display()),
        })?;
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| CoreError::InvalidMetadata {
                    message: format!("{}: {e}", jar.path.display()),
                })?;
            if entry.is_dir() {
                continue;
            }
            let name = entry
                .name()
                .map_err(|e| CoreError::InvalidMetadata {
                    message: format!("{}: {e}", jar.path.display()),
                })?
                .into_owned();
            if jar.exclude.iter().any(|ex| name.starts_with(ex.as_str())) {
                continue;
            }
            let Some(rel) = entry.enclosed_name() else {
                tracing::warn!(entry = %name, "skipping natives entry outside the destination");
                continue;
            };
            let out = dest.join(rel);
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|s| io_err(parent, s))?;
            }
            let mut target = std::fs::File::create(&out).map_err(|s| io_err(&out, s))?;
            std::io::copy(&mut entry, &mut target).map_err(|s| io_err(&out, s))?;
        }
    }
    Ok(())
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

    #[test]
    fn extracts_and_honours_excludes() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("n.jar");
        make_jar(
            &jar,
            &[
                ("lwjgl64.dll", b"dll"),
                ("META-INF/MANIFEST.MF", b"m"),
                ("sub/x.so", b"so"),
            ],
        );
        let dest = temp.path().join("out");
        extract_natives(
            &[NativeJar {
                path: jar,
                exclude: vec!["META-INF/".into()],
            }],
            &dest,
        )
        .unwrap();
        assert_eq!(std::fs::read(dest.join("lwjgl64.dll")).unwrap(), b"dll");
        assert!(dest.join("sub").join("x.so").is_file());
        assert!(!dest.join("META-INF").exists());
    }

    #[test]
    fn previous_contents_are_removed() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("stale.dll"), b"old").unwrap();
        extract_natives(&[], &dest).unwrap();
        assert!(dest.is_dir());
        assert!(!dest.join("stale.dll").exists());
    }

    #[test]
    fn entries_escaping_the_destination_are_skipped() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("evil.jar");
        make_jar(&jar, &[("../evil.dll", b"x"), ("ok.dll", b"y")]);
        let dest = temp.path().join("out");
        extract_natives(
            &[NativeJar {
                path: jar,
                exclude: vec![],
            }],
            &dest,
        )
        .unwrap();
        assert!(!temp.path().join("evil.dll").exists());
        assert!(dest.join("ok.dll").is_file());
    }

    #[test]
    fn corrupt_jar_is_invalid_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("bad.jar");
        std::fs::write(&jar, b"not a zip").unwrap();
        let err = extract_natives(
            &[NativeJar {
                path: jar,
                exclude: vec![],
            }],
            &temp.path().join("out"),
        )
        .unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
    }
}
