//! クライアント jar からワールドの DataVersion を読む。

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// jar のルートにある `version.json` の `world_version` を返す。
///
/// jar が無い、zip が壊れている、`version.json` が無い、JSON が不正、キーが無い、
/// 整数でない、のいずれの場合も `None` を返す。
pub fn read_data_version(client_jar: &Path) -> Option<i64> {
    let file = File::open(client_jar).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let mut entry = zip.by_name("version.json").ok()?;
    let mut text = String::new();
    entry.read_to_string(&mut text).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("world_version")?.as_i64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    /// `entries` を持つ一時 jar を作り、そのパスを返す。
    fn make_jar(dir: &Path, entries: &[(&str, &str)]) -> std::path::PathBuf {
        let path = dir.join("client.jar");
        let file = File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, body) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn reads_world_version() {
        let temp = tempfile::tempdir().unwrap();
        let jar = make_jar(
            temp.path(),
            &[("version.json", r#"{"id":"1.21.1","world_version":3955}"#)],
        );
        assert_eq!(read_data_version(&jar), Some(3955));
    }

    #[test]
    fn missing_version_json_is_none() {
        let temp = tempfile::tempdir().unwrap();
        let jar = make_jar(temp.path(), &[("META-INF/MANIFEST.MF", "x")]);
        assert_eq!(read_data_version(&jar), None);
    }

    #[test]
    fn corrupt_zip_is_none() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("client.jar");
        std::fs::write(&jar, b"not a zip file").unwrap();
        assert_eq!(read_data_version(&jar), None);
    }

    #[test]
    fn missing_file_is_none() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(read_data_version(&temp.path().join("nope.jar")), None);
    }

    #[test]
    fn invalid_json_is_none() {
        let temp = tempfile::tempdir().unwrap();
        let jar = make_jar(temp.path(), &[("version.json", "{not json")]);
        assert_eq!(read_data_version(&jar), None);
    }

    #[test]
    fn missing_or_non_integer_key_is_none() {
        let temp = tempfile::tempdir().unwrap();
        let no_key = make_jar(temp.path(), &[("version.json", r#"{"id":"x"}"#)]);
        assert_eq!(read_data_version(&no_key), None);

        let temp2 = tempfile::tempdir().unwrap();
        let text = make_jar(
            temp2.path(),
            &[("version.json", r#"{"world_version":"3955"}"#)],
        );
        assert_eq!(read_data_version(&text), None);
    }
}
