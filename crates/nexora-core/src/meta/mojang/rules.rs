//! version JSON の rules の評価。調査 1 の Rules の節に従う。

use regex::Regex;

use super::version::{Rule, RuleAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsName {
    Windows,
    Osx,
    Linux,
}

impl OsName {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            OsName::Windows
        } else if cfg!(target_os = "macos") {
            OsName::Osx
        } else {
            OsName::Linux
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            OsName::Windows => "windows",
            OsName::Osx => "osx",
            OsName::Linux => "linux",
        }
    }
}

/// ルールの評価に使う環境。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEnv {
    pub os: OsName,
    /// Java の os.version に相当する文字列。
    pub os_version: String,
    /// 使う JVM のアーキテクチャ（std::env::consts::ARCH の値）。
    pub arch: String,
}

impl RuleEnv {
    pub fn current() -> Self {
        let os = OsName::current();
        // Windows 10 と 11 の Java は os.version を "10.0" と報告する。Tauri 2 は Windows 10 以降でしか動かない
        let os_version = match os {
            OsName::Windows => "10.0".to_string(),
            _ => sysinfo::System::os_version().unwrap_or_default(),
        };
        Self {
            os,
            os_version,
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

fn prefix_match(pattern: &str, value: &str) -> bool {
    match Regex::new(&format!("^(?:{pattern})")) {
        Ok(re) => re.is_match(value),
        Err(err) => {
            tracing::warn!(pattern, %err, "ignoring invalid os.version pattern");
            false
        }
    }
}

fn matches(rule: &Rule, env: &RuleEnv) -> bool {
    if let Some(os) = &rule.os {
        if os.name.as_deref().is_some_and(|n| n != env.os.as_str()) {
            return false;
        }
        if os
            .version
            .as_deref()
            .is_some_and(|v| !prefix_match(v, &env.os_version))
        {
            return false;
        }
        if os.arch.as_deref().is_some_and(|a| a != env.arch) {
            return false;
        }
    }
    // フェーズ 2 では、どの feature も有効にしない
    if let Some(features) = &rule.features
        && features.values().any(|wanted| *wanted)
    {
        return false;
    }
    true
}

/// rules がない、または空なら許可。そうでなければ、一致した最後のルールの action に従う。
pub fn allowed(rules: Option<&[Rule]>, env: &RuleEnv) -> bool {
    let Some(rules) = rules.filter(|r| !r.is_empty()) else {
        return true;
    };
    let mut allow = false;
    for rule in rules {
        if matches(rule, env) {
            allow = rule.action == RuleAction::Allow;
        }
    }
    allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::mojang::version::Rule;

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    fn env(os: OsName, version: &str, arch: &str) -> RuleEnv {
        RuleEnv {
            os,
            os_version: version.into(),
            arch: arch.into(),
        }
    }

    #[test]
    fn missing_and_empty_rules_allow() {
        let e = env(OsName::Windows, "10.0", "x86_64");
        assert!(allowed(None, &e));
        assert!(allowed(Some(&[]), &e));
    }

    #[test]
    fn last_matching_rule_wins() {
        // 1.8.9 の lwjgl：macOS 以外で許可
        let r = rules(r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#);
        assert!(allowed(Some(&r), &env(OsName::Windows, "10.0", "x86_64")));
        assert!(!allowed(Some(&r), &env(OsName::Osx, "14.5", "aarch64")));
    }

    #[test]
    fn non_matching_list_disallows() {
        let r = rules(r#"[{"action":"allow","os":{"name":"osx"}}]"#);
        assert!(!allowed(Some(&r), &env(OsName::Linux, "6.8", "x86_64")));
    }

    #[test]
    fn os_version_is_a_prefix_regex() {
        let r = rules(r#"[{"action":"allow","os":{"name":"windows","version":"^10\\."}}]"#);
        assert!(allowed(Some(&r), &env(OsName::Windows, "10.0", "x86_64")));
        assert!(!allowed(Some(&r), &env(OsName::Windows, "6.1", "x86_64")));
        let unanchored = rules(r#"[{"action":"allow","os":{"version":"10\\."}}]"#);
        assert!(!allowed(
            Some(&unanchored),
            &env(OsName::Windows, "5.10.1", "x86_64")
        ));
    }

    #[test]
    fn invalid_regex_does_not_match() {
        let r = rules(r#"[{"action":"allow","os":{"version":"(["}}]"#);
        assert!(!allowed(Some(&r), &env(OsName::Windows, "10.0", "x86_64")));
    }

    #[test]
    fn arch_compares_with_the_jvm_architecture() {
        let r = rules(r#"[{"action":"allow","os":{"arch":"x86"}}]"#);
        assert!(allowed(Some(&r), &env(OsName::Windows, "10.0", "x86")));
        assert!(!allowed(Some(&r), &env(OsName::Windows, "10.0", "x86_64")));
    }

    #[test]
    fn features_are_all_false() {
        let demo = rules(r#"[{"action":"allow","features":{"is_demo_user":true}}]"#);
        assert!(!allowed(
            Some(&demo),
            &env(OsName::Windows, "10.0", "x86_64")
        ));
        let not_demo = rules(r#"[{"action":"allow","features":{"is_demo_user":false}}]"#);
        assert!(allowed(
            Some(&not_demo),
            &env(OsName::Windows, "10.0", "x86_64")
        ));
    }
}
