//! ログとクラッシュレポートから、原因の手がかりを取り出す。書式は `docs/research/09-crash-log-formats.md`。

use std::sync::LazyLock;

use regex::Regex;

use super::{Clue, CrashReason};

/// これより長い行は読まない（巨大な 1 行で正規表現が重くならないように）。
const MAX_LINE_BYTES: usize = 10_000;
/// 根拠として見せる行の長さ（文字数）。
const MAX_EVIDENCE_CHARS: usize = 200;

/// 除くパッケージ（ゲーム本体、ローダー、ライブラリ）。
const SKIPPED_PACKAGES: [&str; 11] = [
    "java.",
    "javax.",
    "jdk.",
    "sun.",
    "net.minecraft.",
    "com.mojang.",
    "org.spongepowered.",
    "net.fabricmc.",
    "net.minecraftforge.",
    "net.neoforged.",
    "cpw.mods.",
];

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a valid pattern")
}

/// `Mod '名前' (ID)`
static FABRIC_MOD: LazyLock<Regex> = LazyLock::new(|| re(r"[Mm]od '[^']*' \(([^)]+)\)"));
static FORGE_DEP: LazyLock<Regex> =
    LazyLock::new(|| re(r"Mod ID: '([^']+)', Requested by: '([^']+)'.*Actual version: '([^']*)'"));
static ENTRYPOINT: LazyLock<Regex> = LazyLock::new(|| re(r"entrypoint.*provided by '([^']+)'"));
static LOAD_FAILED: LazyLock<Regex> =
    LazyLock::new(|| re(r"\(([A-Za-z0-9_.\-]+)\) has failed to load correctly"));
static MIXIN_APPLY: LazyLock<Regex> = LazyLock::new(|| re(r"Mixin apply for mod (\S+) failed"));
static PAREN_ID: LazyLock<Regex> = LazyLock::new(|| re(r"\(([^)]+)\)"));
static STACK_FRAME: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\s*at (?:[\w.$-]+//?)?([\w.$]+)\.[\w$<>]+\("));

fn evidence(line: &str) -> String {
    line.trim().chars().take(MAX_EVIDENCE_CHARS).collect()
}

fn clue(mod_id: &str, reason: CrashReason, line: &str, missing: Option<&str>) -> Clue {
    Clue {
        mod_id: mod_id.to_string(),
        reason,
        evidence: evidence(line),
        missing: missing.map(str::to_string),
    }
}

/// 1 行から手がかりを取り出す（疑わしい MOD の欄は `extract_clues` が行をまたいで扱う）。
fn line_clues(line: &str, out: &mut Vec<Clue>) {
    if line.contains(" requires ")
        && (line.contains("which is missing!") || line.contains("but only the wrong version"))
    {
        let reason = if line.contains("which is missing!") {
            CrashReason::MissingDependency
        } else {
            CrashReason::IncompatibleVersion
        };
        push_fabric_pair(line, reason, out);
    } else if line.contains(" is incompatible with ") && line.contains("conflicting version") {
        push_fabric_pair(line, CrashReason::IncompatibleVersion, out);
    }
    if let Some(c) = FORGE_DEP.captures(line) {
        let reason = if &c[3] == "[MISSING]" {
            CrashReason::MissingDependency
        } else {
            CrashReason::IncompatibleVersion
        };
        out.push(clue(&c[2], reason, line, Some(&c[1])));
    }
    if let Some(c) = ENTRYPOINT.captures(line) {
        out.push(clue(&c[1], CrashReason::ModLoadingError, line, None));
    }
    if let Some(c) = LOAD_FAILED.captures(line) {
        out.push(clue(&c[1], CrashReason::ModLoadingError, line, None));
    }
    if let Some(c) = MIXIN_APPLY.captures(line) {
        out.push(clue(&c[1], CrashReason::MixinFailure, line, None));
    }
}

/// Fabric の「A が B を必要とする」形の行。先に出る MOD が原因、次に出る MOD が足りない（合わない）側。
fn push_fabric_pair(line: &str, reason: CrashReason, out: &mut Vec<Clue>) {
    let mut ids = FABRIC_MOD
        .captures_iter(line)
        .map(|c| c.get(1).map_or("", |m| m.as_str()));
    if let Some(first) = ids.next() {
        out.push(clue(first, reason, line, ids.next()));
    }
}

/// 「Suspected Mods:」の欄の 1 行から ID を取り出す。
fn push_suspects(line: &str, out: &mut Vec<Clue>, first_only: bool) {
    let ids = PAREN_ID
        .captures_iter(line)
        .map(|c| c.get(1).map_or("", |m| m.as_str()));
    for id in ids.take(if first_only { 1 } else { usize::MAX }) {
        out.push(clue(id, CrashReason::SuspectedInReport, line, None));
    }
}

/// ログとクラッシュレポートの文字列から手がかりを取り出す（`StackTrace` は含めない）。
pub fn extract_clues(text: &str) -> Vec<Clue> {
    let mut out = Vec::new();
    let mut in_suspects = false;
    for line in text.lines().filter(|l| l.len() <= MAX_LINE_BYTES) {
        if in_suspects {
            if line.starts_with(char::is_whitespace) && !line.trim().is_empty() {
                push_suspects(line, &mut out, true);
                continue;
            }
            in_suspects = false;
        }
        if let Some((_, rest)) = line.split_once("Suspected Mods:") {
            let rest = rest.trim();
            if rest.is_empty() {
                in_suspects = true;
            } else if !rest.eq_ignore_ascii_case("none") {
                push_suspects(line, &mut out, false);
            }
            continue;
        }
        line_clues(line, &mut out);
    }
    out
}

/// スタックトレースに出るクラスのうち、ゲーム本体、ローダー、ライブラリのものを除いたもの。出現順で重複なし。
pub fn stack_classes(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| l.len() <= MAX_LINE_BYTES) {
        let Some(c) = STACK_FRAME.captures(line) else {
            continue;
        };
        // 内部クラスは外側のクラスにそろえる
        let class = c[1].split('$').next().unwrap_or("");
        if class.is_empty() || SKIPPED_PACKAGES.iter().any(|p| class.starts_with(p)) {
            continue;
        }
        if !out.iter().any(|seen| seen == class) {
            out.push(class.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnose::CrashReason;

    fn one(text: &str) -> Clue {
        let mut clues = extract_clues(text);
        assert_eq!(clues.len(), 1, "{clues:?}");
        clues.remove(0)
    }

    #[test]
    fn fabric_missing_dependency_names_the_requesting_mod() {
        let line = "[12:00:01] [main/ERROR] (FabricLoader) \t - Mod 'Foo' (foo) 1.0.0 requires version 2.0 or later of mod 'Bar' (bar), which is missing!";
        let c = one(line);
        assert_eq!(c.mod_id, "foo");
        assert_eq!(c.reason, CrashReason::MissingDependency);
        assert_eq!(c.missing.as_deref(), Some("bar"));
        assert!(c.evidence.contains("which is missing!"));
    }

    #[test]
    fn fabric_wrong_version_is_incompatible() {
        let line = "\t - Mod 'Foo' (foo) 1.0.0 requires version 2.0 or later of mod 'Bar' (bar), but only the wrong version is present: 1.0.0!";
        let c = one(line);
        assert_eq!(c.mod_id, "foo");
        assert_eq!(c.reason, CrashReason::IncompatibleVersion);
        assert_eq!(c.missing.as_deref(), Some("bar"));
    }

    #[test]
    fn fabric_breaks_names_the_first_mod() {
        let line = "\t - Mod 'Foo' (foo) 1.0.0 is incompatible with any version of mod 'Baz' (baz), yet a conflicting version is present: 1.0.0!";
        let c = one(line);
        assert_eq!(c.mod_id, "foo");
        assert_eq!(c.reason, CrashReason::IncompatibleVersion);
        assert_eq!(c.missing.as_deref(), Some("baz"));
    }

    #[test]
    fn fabric_entrypoint_failure_names_the_providing_mod() {
        let c =
            one("Could not execute entrypoint stage 'main' due to errors, provided by 'sodium'!");
        assert_eq!(c.mod_id, "sodium");
        assert_eq!(c.reason, CrashReason::ModLoadingError);
    }

    #[test]
    fn forge_missing_mandatory_dependencies() {
        let text = "[27Oct2023 11:48:12.040] [main/ERROR] [net.minecraftforge.fml.loading.ModSorter/LOADING]: Missing or unsupported mandatory dependencies:\n\tMod ID: 'forge', Requested by: 'puzzleslib', Expected range: '[47.1.71,)', Actual version: '47.1.3'\n\tMod ID: 'bar', Requested by: 'foo', Expected range: '[1,)', Actual version: '[MISSING]'\n";
        let clues = extract_clues(text);
        assert_eq!(clues.len(), 2, "{clues:?}");
        assert_eq!(clues[0].mod_id, "puzzleslib");
        assert_eq!(clues[0].reason, CrashReason::IncompatibleVersion);
        assert_eq!(clues[0].missing.as_deref(), Some("forge"));
        assert_eq!(clues[1].mod_id, "foo");
        assert_eq!(clues[1].reason, CrashReason::MissingDependency);
        assert_eq!(clues[1].missing.as_deref(), Some("bar"));
    }

    #[test]
    fn forge_loading_exception_names_the_mod() {
        let c = one(
            "net.minecraftforge.fml.ModLoadingException: Create (create) has failed to load correctly",
        );
        assert_eq!(c.mod_id, "create");
        assert_eq!(c.reason, CrashReason::ModLoadingError);
    }

    #[test]
    fn mixin_failure_names_the_mod_of_the_mixin_config() {
        let line = "[ERROR] (FabricLoader/Mixin) Mixin apply for mod porting_lib failed porting_lib.mixins.common.json:ItemEntityMixin -> net.minecraft.class_1542";
        let c = one(line);
        assert_eq!(c.mod_id, "porting_lib");
        assert_eq!(c.reason, CrashReason::MixinFailure);
    }

    #[test]
    fn forge_crash_report_suspected_mods() {
        let one_line = one("Suspected Mods: Create (create)");
        assert_eq!(one_line.mod_id, "create");
        assert_eq!(one_line.reason, CrashReason::SuspectedInReport);
        let indented = "\tSuspected Mods: \n\t\tCreate (create), Version: 0.5.1\n\t\tFlywheel (flywheel), Version: 0.6\n\nOther: (not_a_mod)";
        let ids: Vec<_> = extract_clues(indented)
            .into_iter()
            .map(|c| c.mod_id)
            .collect();
        assert_eq!(ids, ["create", "flywheel"]);
    }

    #[test]
    fn suspected_none_and_unrelated_lines_give_no_clues() {
        assert!(extract_clues("Suspected Mods: NONE").is_empty());
        assert!(
            extract_clues("[Render thread/INFO]: Setting user: Dev\n[main/WARN]: nothing here")
                .is_empty()
        );
    }

    #[test]
    fn very_long_lines_are_not_read() {
        let long = format!("Mixin apply for mod foo failed {}", "x".repeat(20_000));
        assert!(extract_clues(&long).is_empty());
    }

    #[test]
    fn stack_classes_skips_game_and_library_packages() {
        let text = "\tat net.minecraft.client.Minecraft.run(Minecraft.java:1)\n\
                    \tat com.example.mod.Foo.bar(Foo.java:10)\n\
                    \tat knot//com.example.mod.Foo.baz(Foo.java:20)\n\
                    \tat java.base/java.lang.Thread.run(Thread.java:833)\n\
                    \tat net.fabricmc.loader.Y.z(Y.java:1)\n\
                    \tat me.other.Baz$Inner.q(Baz.java:5)\n\
                    \tat org.spongepowered.asm.mixin.A.b(A.java:1)\n";
        assert_eq!(stack_classes(text), ["com.example.mod.Foo", "me.other.Baz"]);
    }
}
