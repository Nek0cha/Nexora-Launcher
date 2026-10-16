//! 導入した Java の確認。

use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use crate::error::CoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInfo {
    pub major: u32,
    /// Java の os.arch の値（amd64 など）。
    pub arch: String,
}

/// "1.8.0_51" は 8、"17.0.8" は 17。
pub fn major_from_version(v: &str) -> Option<u32> {
    let mut parts = v.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// `-XshowSettings:properties -version` の出力から、java.version と os.arch を読む。
pub fn parse_properties(output: &str) -> Option<JavaInfo> {
    let value = |key: &str| {
        output.lines().find_map(|line| {
            let (k, v) = line.split_once('=')?;
            (k.trim() == key).then(|| v.trim().to_string())
        })
    };
    Some(JavaInfo {
        major: major_from_version(&value("java.version")?)?,
        arch: value("os.arch")?,
    })
}

fn normalize(arch: &str) -> &str {
    match arch {
        "amd64" | "x86_64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        "x86" | "i386" | "i686" => "x86",
        other => other,
    }
}

/// Java の os.arch と std::env::consts::ARCH が同じアーキテクチャを指すか。
pub fn arch_matches(os_arch: &str, std_arch: &str) -> bool {
    normalize(os_arch) == normalize(std_arch)
}

/// Java を実行して情報を得る。テストでは差し替える。
pub trait JavaProbe: Send + Sync {
    fn probe<'a>(
        &'a self,
        java: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<JavaInfo, CoreError>> + Send + 'a>>;
}

pub struct SystemProbe;

impl JavaProbe for SystemProbe {
    fn probe<'a>(
        &'a self,
        java: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<JavaInfo, CoreError>> + Send + 'a>> {
        Box::pin(async move {
            let mut cmd = tokio::process::Command::new(java);
            cmd.args(["-XshowSettings:properties", "-version"])
                .stdin(std::process::Stdio::null());
            #[cfg(windows)]
            cmd.creation_flags(0x0800_0000);
            let output = cmd
                .output()
                .await
                .map_err(|e| CoreError::JavaVerifyFailed {
                    expected_major: 0,
                    found: e.to_string(),
                })?;
            let text = String::from_utf8_lossy(&output.stderr);
            parse_properties(&text).ok_or_else(|| CoreError::JavaVerifyFailed {
                expected_major: 0,
                found: "unreadable java output".into(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn majors() {
        assert_eq!(major_from_version("1.8.0_51"), Some(8));
        assert_eq!(major_from_version("17.0.8"), Some(17));
        assert_eq!(major_from_version("21"), Some(21));
        assert_eq!(major_from_version("x"), None);
    }

    #[test]
    fn parses_show_settings_output() {
        let out = "Property settings:\n    file.encoding = UTF-8\n    java.version = 1.8.0_51\n    os.arch = amd64\n\njava version \"1.8.0_51\"\n";
        assert_eq!(
            parse_properties(out),
            Some(JavaInfo {
                major: 8,
                arch: "amd64".into()
            })
        );
        assert_eq!(parse_properties("garbage"), None);
    }

    #[test]
    fn arch_aliases() {
        assert!(arch_matches("amd64", "x86_64"));
        assert!(arch_matches("x86_64", "x86_64"));
        assert!(arch_matches("aarch64", "aarch64"));
        assert!(arch_matches("arm64", "aarch64"));
        assert!(arch_matches("x86", "x86"));
        assert!(arch_matches("i386", "x86"));
        assert!(!arch_matches("amd64", "aarch64"));
        assert!(!arch_matches("x86", "x86_64"));
    }
}
