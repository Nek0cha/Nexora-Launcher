//! Mojang の Java runtime のプラットフォームのキー。

/// std::env::consts::{OS, ARCH} から、runtime の索引のキーを求める。
pub fn platform_key(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("windows-x64"),
        ("windows", "aarch64") => Some("windows-arm64"),
        ("macos", "x86_64") => Some("mac-os"),
        ("macos", "aarch64") => Some("mac-os-arm64"),
        ("linux", "x86_64") => Some("linux"),
        ("linux", "x86") => Some("linux-i386"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_for_supported_platforms() {
        assert_eq!(platform_key("windows", "x86_64"), Some("windows-x64"));
        assert_eq!(platform_key("windows", "aarch64"), Some("windows-arm64"));
        assert_eq!(platform_key("macos", "x86_64"), Some("mac-os"));
        assert_eq!(platform_key("macos", "aarch64"), Some("mac-os-arm64"));
        assert_eq!(platform_key("linux", "x86_64"), Some("linux"));
        assert_eq!(platform_key("linux", "x86"), Some("linux-i386"));
        assert_eq!(platform_key("freebsd", "x86_64"), None);
    }
}
