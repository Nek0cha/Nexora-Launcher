//! Forge と NeoForge のバージョン一覧（公式 Maven のメタデータ）とインストーラの URL。

use std::cmp::Ordering;

use quick_xml::Reader;
use quick_xml::events::Event;

use super::{LoaderKind, check_version, compare_parts};
use crate::error::CoreError;

pub const FORGE_MAVEN_URL: &str = "https://maven.minecraftforge.net";
pub const FORGE_PROMOTIONS_URL: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
pub const NEOFORGE_MAVEN_URL: &str = "https://maven.neoforged.net/releases";

/// `maven-metadata.xml` の `<versioning><versions><version>` を並びのまま返す。
/// 実物の並びは新しい順でも版の順でもないため、呼び出し側で並べ直すこと。
pub fn parse_maven_versions(xml: &[u8]) -> Result<Vec<String>, CoreError> {
    let invalid = |message: String| CoreError::InvalidMetadata { message };
    let text = std::str::from_utf8(xml).map_err(|e| invalid(format!("maven metadata: {e}")))?;
    let mut reader = Reader::from_str(text);
    reader.config_mut().check_end_names = true;
    let mut out = Vec::new();
    // `<versions>` の中の `<version>` だけを集める
    let mut in_versions = false;
    let mut in_version = false;
    loop {
        match reader
            .read_event()
            .map_err(|e| invalid(format!("maven metadata: {e}")))?
        {
            Event::Start(e) => match e.name().as_ref() {
                "versions" => in_versions = true,
                "version" if in_versions => in_version = true,
                _ => {}
            },
            Event::End(e) => match e.name().as_ref() {
                "versions" => in_versions = false,
                "version" => in_version = false,
                _ => {}
            },
            Event::Text(t) if in_version => {
                let v = t.as_ref().trim();
                if !v.is_empty() {
                    out.push(v.to_string());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

/// Forge の完全な版（`1.20.1-47.4.10`）の Minecraft のバージョン（最初の `-` の前）。
pub fn forge_mc_version(full: &str) -> Option<&str> {
    full.split_once('-')
        .map(|(mc, _)| mc)
        .filter(|mc| !mc.is_empty())
}

/// Forge の完全な版から Minecraft のバージョンと、ビルド番号の本体（`47.4.10`）を取り出す。
/// `<mc>-<数字の組>[-...]` の形でないものは `None`（実物の約 7% がこの形ではない）。
pub fn forge_parts(full: &str) -> Option<(&str, &str)> {
    let (mc, rest) = full.split_once('-')?;
    let build = rest.split('-').next()?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if mc.is_empty() || !build.split('.').all(digits) {
        return None;
    }
    Some((mc, build))
}

/// ビルド番号の本体。形が合わなければ空。
pub fn forge_build(full: &str) -> &str {
    forge_parts(full).map_or("", |(_, build)| build)
}

/// Forge の完全な版を新しい順に並べるための比較（`a` が新しければ `Less`）。
/// 同じビルド番号なら文字列で決め、並びを安定させる。
pub fn compare_forge_newest_first(a: &str, b: &str) -> Ordering {
    compare_parts(forge_build(b), forge_build(a)).then_with(|| b.cmp(a))
}

/// NeoForge の版の Minecraft のバージョン。`1.20.1-47.1.106` は `1.20.1`、
/// `21.1.172` は `1.21.1`、`20.4.237` は `1.20.4`、`21.0.5` は `1.21`。
/// 数字で始まらない、または先頭の数が 26 以上のものは `None`。
pub fn neoforge_mc_version(version: &str) -> Option<String> {
    let core = version.split('-').next()?;
    let mut parts = core.split('.');
    let first: u64 = parts.next()?.parse().ok()?;
    if first == 1 {
        // 旧系統（`net.neoforged:forge`）は先頭が Minecraft のバージョンそのもの
        return version.contains('-').then(|| core.to_string());
    }
    if !(2..26).contains(&first) {
        return None;
    }
    let minor: u64 = parts.next()?.parse().ok()?;
    Some(if minor == 0 {
        format!("1.{first}")
    } else {
        format!("1.{first}.{minor}")
    })
}

/// `1.20.1` のような数だけの版を数の列にする。数でない部分があれば `None`（スナップショットなど）。
fn numeric_parts(mc: &str) -> Option<Vec<u64>> {
    mc.split('.').map(|p| p.parse().ok()).collect()
}

/// 先頭が 1（`26.x` などの新しい命名を除く）で、`min` 以上の Minecraft のバージョンか。
fn at_least(mc: &str, min: &[u64]) -> bool {
    let Some(mut parts) = numeric_parts(mc) else {
        return false;
    };
    if parts.first() != Some(&1) {
        return false;
    }
    let len = parts.len().max(min.len());
    parts.resize(len, 0);
    let mut min = min.to_vec();
    min.resize(len, 0);
    parts >= min
}

/// Forge の対応範囲（1.16.5 以降。数で比べ、スナップショットは対象外）。
pub fn forge_supported(mc: &str) -> bool {
    at_least(mc, &[1, 16, 5])
}

/// NeoForge の対応範囲（1.20.1 以降。数で比べ、スナップショットは対象外）。
pub fn neoforge_supported(mc: &str) -> bool {
    at_least(mc, &[1, 20, 1])
}

/// インストーラの URL。`kind` が Forge と NeoForge 以外、または `version` が安全でないときは `InvalidMetadata`。
pub fn installer_url(kind: LoaderKind, base: &str, version: &str) -> Result<String, CoreError> {
    check_version("loader version", version)?;
    let base = base.trim_end_matches('/');
    let (group, artifact) = match kind {
        LoaderKind::Forge => ("net/minecraftforge", "forge"),
        // NeoForge の旧系統（1.20.1）は `net.neoforged:forge`
        LoaderKind::Neoforge if version.starts_with("1.20.1-") => ("net/neoforged", "forge"),
        LoaderKind::Neoforge => ("net/neoforged", "neoforge"),
        _ => {
            return Err(CoreError::InvalidMetadata {
                message: format!("{} has no installer", kind.as_str()),
            });
        }
    };
    Ok(format!(
        "{base}/{group}/{artifact}/{version}/{artifact}-{version}-installer.jar"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORGE_XML: &[u8] =
        include_bytes!("../../../tests/fixtures/forge/forge-maven-metadata.xml");

    #[test]
    fn maven_versions_keep_file_order_and_skip_latest() {
        let vs = parse_maven_versions(FORGE_XML).unwrap();
        assert_eq!(vs.first().unwrap(), "1.21-51.0.33");
        assert_eq!(vs.last().unwrap(), "26.3-66.0.13");
        assert_eq!(vs.len(), 32);
        assert!(!vs.iter().any(|v| v.is_empty()));
    }

    #[test]
    fn maven_versions_reject_broken_xml() {
        let err = parse_maven_versions(b"<metadata><versions><version>1</versio").unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
    }

    #[test]
    fn forge_mc_version_is_before_first_dash() {
        assert_eq!(forge_mc_version("1.20.1-47.4.10"), Some("1.20.1"));
        assert_eq!(
            forge_mc_version("1.7.10-10.13.4.1614-1.7.10"),
            Some("1.7.10")
        );
        assert_eq!(forge_mc_version("nodash"), None);
    }

    #[test]
    fn forge_parts_drops_odd_shapes() {
        assert_eq!(forge_parts("1.20.1-47.4.10"), Some(("1.20.1", "47.4.10")));
        assert_eq!(
            forge_parts("1.12.2-14.23.4.2720-4627"),
            Some(("1.12.2", "14.23.4.2720"))
        );
        assert_eq!(forge_parts("1.7.10-pre4"), None);
        assert_eq!(forge_parts("1.7.10-"), None);
        assert_eq!(forge_parts("1.7.10"), None);
    }

    #[test]
    fn neoforge_mc_versions() {
        let cases = [
            ("1.20.1-47.1.106", Some("1.20.1")),
            ("21.1.172", Some("1.21.1")),
            ("20.4.237", Some("1.20.4")),
            ("21.0.5", Some("1.21")),
            ("20.2.3-beta", Some("1.20.2")),
            ("21.0.0-beta", Some("1.21")),
            ("26.1.0.0-alpha.1+snapshot-1", None),
            ("26.3.0.95", None),
            ("0.25w14craftmine.3-beta", None),
        ];
        for (input, want) in cases {
            assert_eq!(neoforge_mc_version(input).as_deref(), want, "{input}");
        }
    }

    #[test]
    fn support_ranges() {
        assert!(forge_supported("1.16.5"));
        assert!(!forge_supported("1.16.4"));
        assert!(!forge_supported("1.12.2"));
        assert!(forge_supported("1.21.1"));
        assert!(forge_supported("1.21"));
        assert!(forge_supported("1.21.11"));
        assert!(!forge_supported("25w14a"));
        assert!(!forge_supported("26.3"));
        assert!(!forge_supported("26.1.2"));
        assert!(neoforge_supported("1.20.1"));
        assert!(!neoforge_supported("1.19.4"));
        assert!(neoforge_supported("1.21"));
        assert!(!neoforge_supported("26.3"));
    }

    #[test]
    fn installer_urls() {
        assert_eq!(
            installer_url(LoaderKind::Forge, "https://f.example/", "1.20.1-47.4.10").unwrap(),
            "https://f.example/net/minecraftforge/forge/1.20.1-47.4.10/forge-1.20.1-47.4.10-installer.jar"
        );
        assert_eq!(
            installer_url(LoaderKind::Neoforge, "https://n.example", "1.20.1-47.1.106").unwrap(),
            "https://n.example/net/neoforged/forge/1.20.1-47.1.106/forge-1.20.1-47.1.106-installer.jar"
        );
        assert_eq!(
            installer_url(LoaderKind::Neoforge, "https://n.example", "21.1.172").unwrap(),
            "https://n.example/net/neoforged/neoforge/21.1.172/neoforge-21.1.172-installer.jar"
        );
    }

    #[test]
    fn installer_url_rejects_unsafe_and_other_kinds() {
        for kind in [LoaderKind::Forge, LoaderKind::Neoforge] {
            let err = installer_url(kind, "https://x.example", "../x").unwrap_err();
            assert_eq!(err.code(), "meta.invalid");
        }
        let err = installer_url(LoaderKind::Fabric, "https://x.example", "1.0").unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
    }

    #[test]
    fn forge_sort_is_numeric_newest_first() {
        let mut vs = vec![
            "1.21-51.0.9",
            "1.21-51.0.33",
            "1.21-51.0.100",
            "1.21-50.9.9",
        ];
        vs.sort_by(|a, b| compare_forge_newest_first(a, b));
        assert_eq!(
            vs,
            vec![
                "1.21-51.0.100",
                "1.21-51.0.33",
                "1.21-51.0.9",
                "1.21-50.9.9"
            ]
        );
    }
}
