//! ハッシュの計算。

use std::io::Read;
use std::path::Path;

use sha1::{Digest, Sha1};

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha1_hex(bytes: &[u8]) -> String {
    hex(&Sha1::digest(bytes))
}

/// ファイルを 64 KiB ずつ読んで sha1 を求める。
pub fn sha1_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_sha1() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn file_hash_matches_bytes_hash() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("f");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(sha1_file(&path).unwrap(), sha1_hex(b"abc"));
    }
}
