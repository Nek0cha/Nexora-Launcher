//! 起動引数の組み立て。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use secrecy::ExposeSecret;

use super::session::LaunchSession;
use crate::error::CoreError;
use crate::meta::mojang::rules::{OsName, RuleEnv, allowed};
use crate::meta::mojang::version::{ArgValue, VersionJson};

pub const WINDOWS_COMMAND_LIMIT: usize = 32_767;

pub struct ArgInputs<'a> {
    pub version: &'a VersionJson,
    pub session: &'a LaunchSession,
    pub env: &'a RuleEnv,
    pub game_dir: &'a Path,
    pub assets_root: &'a Path,
    pub natives_dir: &'a Path,
    pub library_dir: &'a Path,
    pub classpath: &'a [PathBuf],
    /// `${path}` を置き換え済みのログ設定の引数。
    pub logging_argument: Option<&'a str>,
    pub memory_mb: u32,
    pub launcher_version: &'a str,
}

/// `${name}` を vars の値で置き換える。未知の名前はそのまま残す。
pub fn substitute(template: &str, vars: &BTreeMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                match vars.get(name) {
                    Some(value) => out.push_str(value),
                    None => {
                        tracing::warn!(placeholder = name, "unknown launch placeholder");
                        out.push_str(&rest[start..start + 2 + end + 1]);
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn path_str(p: &Path) -> String {
    p.display().to_string()
}

fn variables<'a>(inputs: &ArgInputs<'a>) -> BTreeMap<&'static str, String> {
    let separator = if inputs.env.os == OsName::Windows {
        ";"
    } else {
        ":"
    };
    let classpath = inputs
        .classpath
        .iter()
        .map(|p| path_str(p))
        .collect::<Vec<_>>()
        .join(separator);
    let s = inputs.session;
    BTreeMap::from([
        ("auth_player_name", s.username.clone()),
        ("auth_uuid", s.uuid.simple().to_string()),
        (
            "auth_access_token",
            s.access_token.expose_secret().to_string(),
        ),
        ("auth_xuid", s.xuid.clone().unwrap_or_default()),
        ("clientid", String::new()),
        ("user_type", s.user_type.arg_value().to_string()),
        ("user_properties", "{}".to_string()),
        ("version_name", inputs.version.id.clone()),
        ("version_type", inputs.version.kind.clone()),
        ("game_directory", path_str(inputs.game_dir)),
        ("assets_root", path_str(inputs.assets_root)),
        ("assets_index_name", inputs.version.asset_index.id.clone()),
        ("natives_directory", path_str(inputs.natives_dir)),
        ("library_directory", path_str(inputs.library_dir)),
        ("classpath", classpath),
        ("classpath_separator", separator.to_string()),
        ("launcher_name", "Nexora".to_string()),
        ("launcher_version", inputs.launcher_version.to_string()),
    ])
}

fn push_values(out: &mut Vec<String>, values: &[ArgValue], env: &RuleEnv) {
    for value in values {
        match value {
            ArgValue::Plain(s) => out.push(s.clone()),
            ArgValue::Conditional { rules, value } => {
                if allowed(Some(rules), env) {
                    out.extend(value.as_slice().iter().cloned());
                }
            }
        }
    }
}

/// JVM 引数、main class、ゲームの引数を返す。
pub fn build_args(inputs: &ArgInputs) -> Vec<String> {
    let v = inputs.version;
    let mut raw = vec![format!("-Xmx{}M", inputs.memory_mb)];
    if let Some(arg) = inputs.logging_argument {
        raw.push(arg.to_string());
    }
    match &v.arguments {
        Some(args) => push_values(&mut raw, &args.jvm, inputs.env),
        None => {
            // 1.12.2 以前は JVM 引数を version JSON に持たないため、ランチャーが補う
            match inputs.env.os {
                OsName::Osx => raw.push("-XstartOnFirstThread".into()),
                OsName::Windows => raw.push(
                    "-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump"
                        .into(),
                ),
                OsName::Linux => {}
            }
            raw.push("-Djava.library.path=${natives_directory}".into());
            raw.push("-cp".into());
            raw.push("${classpath}".into());
        }
    }
    raw.push(v.main_class.clone());
    match (&v.arguments, &v.minecraft_arguments) {
        (Some(args), _) => push_values(&mut raw, &args.game, inputs.env),
        // 空白で分けてから置き換えるため、空白を含むパスも 1 つの引数のまま渡る
        (None, Some(legacy)) => raw.extend(legacy.split_whitespace().map(str::to_string)),
        (None, None) => {}
    }
    let vars = variables(inputs);
    raw.iter().map(|a| substitute(a, &vars)).collect()
}

/// Windows の CreateProcess に渡るコマンドラインの長さの見積もり（引用符と空白を含む）。
pub fn command_length(program: &Path, args: &[String]) -> usize {
    path_str(program).len() + 3 + args.iter().map(|a| a.len() + 3).sum::<usize>()
}

/// Java の @argfile の書式。各引数を二重引用符で囲み、\ と " をエスケープする。
pub fn argfile_contents(args: &[String]) -> String {
    let mut out = String::new();
    for arg in args {
        out.push('"');
        out.push_str(&arg.replace('\\', "\\\\").replace('"', "\\\""));
        out.push_str("\"\n");
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandLine {
    Direct(Vec<String>),
    /// contents を argfile に書いてから args で起動する。
    ArgFile {
        args: Vec<String>,
        contents: String,
    },
}

pub fn fit_command(
    program: &Path,
    args: Vec<String>,
    java_major: u32,
    limit: usize,
    argfile: &Path,
) -> Result<CommandLine, CoreError> {
    let length = command_length(program, &args);
    if length <= limit {
        return Ok(CommandLine::Direct(args));
    }
    if java_major >= 9 {
        return Ok(CommandLine::ArgFile {
            contents: argfile_contents(&args),
            args: vec![format!("@{}", path_str(argfile))],
        });
    }
    Err(CoreError::CommandTooLong { length })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::session::offline_session;
    use crate::meta::mojang::rules::OsName;
    use std::path::PathBuf;

    fn fixture(name: &str) -> VersionJson {
        let path = format!(
            "{}/tests/fixtures/versions/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn args_for(name: &str, os: OsName) -> Vec<String> {
        let version = fixture(name);
        let session = offline_session("Player");
        let env = RuleEnv {
            os,
            os_version: "10.0".into(),
            arch: "x86_64".into(),
        };
        let classpath = vec![PathBuf::from("/lib/a.jar"), PathBuf::from("/v/client.jar")];
        build_args(&ArgInputs {
            version: &version,
            session: &session,
            env: &env,
            game_dir: Path::new("/game"),
            assets_root: Path::new("/assets"),
            natives_dir: Path::new("/natives"),
            library_dir: Path::new("/lib"),
            classpath: &classpath,
            logging_argument: Some("-Dlog4j.configurationFile=/assets/log_configs/client.xml"),
            memory_mb: 2048,
            launcher_version: "0.1.0",
        })
    }

    #[test]
    fn legacy_1_8_9_windows() {
        let args = args_for("1.8.9", OsName::Windows);
        insta::assert_json_snapshot!(args);
        assert!(args.iter().all(|a| !a.contains("${")), "{args:?}");
        assert!(args.contains(&"-Djava.library.path=/natives".to_string()));
        let main = args
            .iter()
            .position(|a| a == "net.minecraft.client.main.Main")
            .unwrap();
        assert!(args.iter().position(|a| a == "-cp").unwrap() < main);
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--username" && w[1] == "Player")
        );
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--userType" && w[1] == "legacy")
        );
    }

    #[test]
    fn legacy_on_macos_starts_on_first_thread() {
        let args = args_for("1.12.2", OsName::Osx);
        assert!(args.contains(&"-XstartOnFirstThread".to_string()));
    }

    #[test]
    fn modern_1_21_5_windows() {
        let args = args_for("1.21.5", OsName::Windows);
        insta::assert_json_snapshot!(args);
        assert!(args.iter().all(|a| !a.contains("${")), "{args:?}");
        assert!(!args.contains(&"--demo".to_string()));
        assert!(!args.contains(&"--width".to_string()));
        assert!(!args.iter().any(|a| a.starts_with("--quickPlay")));
        assert!(args.contains(&"/lib/a.jar;/v/client.jar".to_string()));
    }

    #[test]
    fn classpath_separator_follows_os() {
        let args = args_for("1.21.5", OsName::Linux);
        assert!(args.contains(&"/lib/a.jar:/v/client.jar".to_string()));
    }

    #[test]
    fn unknown_placeholders_are_kept() {
        let vars = BTreeMap::from([("known", "v".to_string())]);
        assert_eq!(
            substitute("a=${known} b=${unknown}", &vars),
            "a=v b=${unknown}"
        );
        assert_eq!(substitute("${unterminated", &vars), "${unterminated");
    }

    #[test]
    fn short_commands_are_direct() {
        let args = vec!["-Xmx2048M".to_string()];
        let line = fit_command(
            Path::new("java"),
            args.clone(),
            8,
            WINDOWS_COMMAND_LIMIT,
            Path::new("/a.args"),
        )
        .unwrap();
        assert_eq!(line, CommandLine::Direct(args));
    }

    #[test]
    fn long_commands_use_an_argfile_on_java_9_and_later() {
        let args = vec!["x".repeat(40_000)];
        let line = fit_command(
            Path::new("java"),
            args,
            17,
            WINDOWS_COMMAND_LIMIT,
            Path::new("/c/a.args"),
        )
        .unwrap();
        match line {
            CommandLine::ArgFile { args, contents } => {
                assert_eq!(args, vec![format!("@{}", Path::new("/c/a.args").display())]);
                assert!(contents.starts_with('"'));
            }
            other => panic!("expected an argfile, got {other:?}"),
        }
    }

    #[test]
    fn long_commands_fail_on_java_8() {
        let args = vec!["x".repeat(40_000)];
        let err = fit_command(
            Path::new("java"),
            args,
            8,
            WINDOWS_COMMAND_LIMIT,
            Path::new("/a.args"),
        )
        .unwrap_err();
        assert_eq!(err.code(), "launch.command_too_long");
    }

    #[test]
    fn argfile_quotes_and_escapes() {
        let contents = argfile_contents(&[r#"C:\a b\c"#.to_string(), r#"say "hi""#.to_string()]);
        assert_eq!(contents, "\"C:\\\\a b\\\\c\"\n\"say \\\"hi\\\"\"\n");
    }
}
