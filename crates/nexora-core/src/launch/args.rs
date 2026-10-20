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
    /// 利用者の JVM 引数（`jvm_args::split` と `strip_memory` を通したもの）。`-Xmx` の直後に入れる。
    pub extra_jvm_args: &'a [String],
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
        // ローダーを重ねたものは、クライアントの jar を持つ Vanilla の id にする
        (
            "version_name",
            inputs
                .version
                .jar
                .as_deref()
                .unwrap_or(&inputs.version.id)
                .to_string(),
        ),
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

/// 起動引数。argfile に書くのは `jvm` だけで、`game`（アクセストークンを含む）はコマンドラインで渡す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchArgs {
    /// JVM 引数と、最後に main class。
    pub jvm: Vec<String>,
    /// ゲームの引数。
    pub game: Vec<String>,
}

impl LaunchArgs {
    fn joined(self) -> Vec<String> {
        let mut all = self.jvm;
        all.extend(self.game);
        all
    }
}

/// JVM 引数と main class、ゲームの引数を返す。
pub fn build_args(inputs: &ArgInputs) -> LaunchArgs {
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
    let mut game = Vec::new();
    match (&v.arguments, &v.minecraft_arguments) {
        (Some(args), _) => push_values(&mut game, &args.game, inputs.env),
        // 空白で分けてから置き換えるため、空白を含むパスも 1 つの引数のまま渡る
        (None, Some(legacy)) => game.extend(legacy.split_whitespace().map(str::to_string)),
        (None, None) => {}
    }
    let vars = variables(inputs);
    let mut jvm: Vec<String> = raw.iter().map(|a| substitute(a, &vars)).collect();
    // 利用者の引数は置き換えない（${auth_access_token} などを argfile に書かないため）。
    // -Xmx の直後に入れ、argfile へは他の JVM 引数と同じ経路で渡る
    jvm.splice(1..1, inputs.extra_jvm_args.iter().cloned());
    LaunchArgs {
        jvm,
        game: game.iter().map(|a| substitute(a, &vars)).collect(),
    }
}

/// Windows の CreateProcess に渡るコマンドラインの長さの見積もり（引用符と空白を含む）。
pub fn command_length(program: &Path, args: &[String]) -> usize {
    path_str(program).len() + 3 + command_length_of(args)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandLine {
    Direct(Vec<String>),
    /// jvm（JVM 引数と main class）を argfile に書いてから（`argfile::encode`）args で起動する。
    /// args は `@<argfile>` に続けてゲームの引数。
    ArgFile {
        args: Vec<String>,
        jvm: Vec<String>,
    },
}

/// コマンドラインを上限に収める。収まらなければ JVM 引数を argfile に移す（Java 9 以降）。
/// ゲームの引数（アクセストークンを含む）は argfile に書かない。
pub fn fit_command(
    program: &Path,
    args: LaunchArgs,
    java_major: u32,
    limit: usize,
    argfile: &Path,
) -> Result<CommandLine, CoreError> {
    let length = command_length(program, &args.jvm) + command_length_of(&args.game);
    if length <= limit {
        return Ok(CommandLine::Direct(args.joined()));
    }
    if java_major < 9 {
        return Err(CoreError::CommandTooLong { length });
    }
    let mut line = vec![format!("@{}", path_str(argfile))];
    line.extend(args.game);
    let length = command_length(program, &line);
    if length > limit {
        return Err(CoreError::CommandTooLong { length });
    }
    Ok(CommandLine::ArgFile {
        jvm: args.jvm,
        args: line,
    })
}

fn command_length_of(args: &[String]) -> usize {
    args.iter().map(|a| a.len() + 3).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::session::{LaunchSession, UserType};
    use crate::meta::mojang::rules::OsName;
    use secrecy::SecretString;
    use std::path::PathBuf;

    /// 開発用の offline_session("Player") と同じ値。
    /// `session::offline_session` は開発ビルドにしかないため、リリースのテストでも使えるよう固定で作る。
    fn player_session() -> LaunchSession {
        LaunchSession {
            uuid: uuid::Uuid::parse_str("a01e3843e5213998958af459800e4d11").unwrap(),
            username: "Player".to_string(),
            access_token: SecretString::from("0".to_string()),
            xuid: None,
            user_type: UserType::Offline,
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn player_session_matches_the_dev_offline_session() {
        let dev = crate::launch::session::offline_session("Player");
        assert_eq!(player_session().uuid, dev.uuid);
    }

    fn fixture(name: &str) -> VersionJson {
        let path = format!(
            "{}/tests/fixtures/versions/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn args_for(name: &str, os: OsName) -> Vec<String> {
        let LaunchArgs { mut jvm, game } = launch_args_for(name, os, &player_session());
        jvm.extend(game);
        jvm
    }

    fn launch_args_for(name: &str, os: OsName, session: &LaunchSession) -> LaunchArgs {
        launch_args_with(name, os, session, &[])
    }

    fn launch_args_with(
        name: &str,
        os: OsName,
        session: &LaunchSession,
        extra_jvm_args: &[String],
    ) -> LaunchArgs {
        let version = fixture(name);
        let env = RuleEnv {
            os,
            os_version: "10.0".into(),
            arch: "x86_64".into(),
        };
        let classpath = vec![PathBuf::from("/lib/a.jar"), PathBuf::from("/v/client.jar")];
        build_args(&ArgInputs {
            version: &version,
            session,
            env: &env,
            game_dir: Path::new("/game"),
            assets_root: Path::new("/assets"),
            natives_dir: Path::new("/natives"),
            library_dir: Path::new("/lib"),
            classpath: &classpath,
            logging_argument: Some("-Dlog4j.configurationFile=/assets/log_configs/client.xml"),
            memory_mb: 2048,
            extra_jvm_args,
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
    fn extra_jvm_args_follow_xmx_and_precede_the_classpath() {
        let extra = vec!["-XX:+UseG1GC".to_string(), "-Dnexora.test=a b".to_string()];
        let args = launch_args_with("1.21.5", OsName::Windows, &player_session(), &extra);
        insta::assert_json_snapshot!(args.jvm);
        assert_eq!(args.jvm[0], "-Xmx2048M");
        assert_eq!(&args.jvm[1..3], extra.as_slice());
        let cp = args.jvm.iter().position(|a| a == "-cp").unwrap();
        assert!(cp > 2);
        let legacy = launch_args_with("1.8.9", OsName::Windows, &player_session(), &extra);
        assert_eq!(&legacy.jvm[1..3], extra.as_slice());
        let cp = legacy.jvm.iter().position(|a| a == "-cp").unwrap();
        assert!(cp > 2);
    }

    #[test]
    fn extra_jvm_args_are_not_substituted() {
        // 利用者の引数に ${...} があっても置き換えない（トークンが argfile に入らないように）
        let extra = vec!["-Dx=${auth_access_token}".to_string()];
        let mut session = player_session();
        session.access_token = SecretString::from("secret-token-value".to_string());
        let args = launch_args_with("1.21.5", OsName::Windows, &session, &extra);
        assert_eq!(args.jvm[1], "-Dx=${auth_access_token}");
        assert!(args.jvm.iter().all(|a| !a.contains("secret-token-value")));
    }

    #[test]
    fn user_jvm_args_never_produce_a_second_xmx() {
        use crate::launch::jvm_args::{split, strip_memory};
        for input in [
            "",
            "\"",
            "-Xmx8G",
            r#"-Xss1M "-Xmx1G"#,
            r#"-Dfoo="a b -Xmx4G"#,
        ] {
            let (extra, _) = strip_memory(split(input));
            let args = launch_args_with("1.21.5", OsName::Windows, &player_session(), &extra);
            let xmx: Vec<_> = args.jvm.iter().filter(|a| a.starts_with("-Xmx")).collect();
            assert_eq!(xmx, vec!["-Xmx2048M"], "{input:?}");
            assert!(args.jvm.iter().all(|a| !a.is_empty()), "{input:?}");
            assert_eq!(
                args.jvm.last().map(String::as_str),
                Some("net.minecraft.client.main.Main"),
                "{input:?}"
            );
        }
    }

    /// 実物の Forge と NeoForge の version.json を、`id` を `inheritsFrom` に書き換えた
    /// Vanilla（1.21.5 の fixture）に重ね、計画と引数を作る。
    #[test]
    fn real_forge_and_neoforge_profiles_merge_plan_and_resolve_every_placeholder() {
        use crate::install::plan::{RESOURCES_URL, check_profile, plan_game};
        use crate::meta::loader::LoaderProfile;
        use crate::meta::loader::merge::merge_profile;
        use crate::paths::AppPaths;

        let env = RuleEnv {
            os: OsName::Windows,
            os_version: "10.0".into(),
            arch: "x86_64".into(),
        };
        let paths = AppPaths::from_root("/data");
        let index: crate::meta::mojang::version::AssetIndex =
            serde_json::from_str(r#"{"objects":{}}"#).unwrap();
        // (fixture, generated の数)。URL の空のライブラリを持つのは 1.16.5 と Forge 26.x
        for (name, generated) in [
            ("forge-1.16.5", 1),
            ("forge-1.20.1", 0),
            ("neoforge-1.20.1", 0),
            ("neoforge-21.1", 0),
            // 26.x。Forge の最終 jar（`...:client`）は installer の maven/ から置くので 1 件
            ("forge-26.3", 1),
            ("neoforge-26.3", 0),
        ] {
            let path = format!(
                "{}/tests/fixtures/forge/{name}-version.json",
                env!("CARGO_MANIFEST_DIR")
            );
            let profile: LoaderProfile =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            let mut vanilla = fixture("1.21.5");
            vanilla.id = profile.inherits_from.clone();
            let version = merge_profile(&vanilla, &profile).unwrap();
            check_profile(&version).unwrap_or_else(|e| panic!("{name}: {e}"));
            let plan = plan_game(&version, &index, &env, &paths, RESOURCES_URL).unwrap();
            assert_eq!(
                plan.generated.len(),
                generated,
                "{name}: {:?}",
                plan.generated
            );
            let args = build_args(&ArgInputs {
                version: &version,
                session: &player_session(),
                env: &env,
                game_dir: Path::new("/game"),
                assets_root: Path::new("/assets"),
                natives_dir: Path::new("/natives"),
                library_dir: &paths.libraries_dir(),
                classpath: &plan.classpath,
                logging_argument: None,
                memory_mb: 2048,
                extra_jvm_args: &[],
                launcher_version: "0.1.0",
            });
            let all: Vec<&String> = args.jvm.iter().chain(&args.game).collect();
            assert!(all.iter().all(|a| !a.contains("${")), "{name}: {all:?}");
            assert_eq!(
                args.jvm.last().map(String::as_str),
                Some(profile.main_class.as_str()),
                "{name}"
            );
        }
    }

    /// 旧形式の Forge（1.12.2）のプロファイルを、旧形式の Vanilla に重ねても `--tweakClass` が残る。
    #[test]
    fn legacy_forge_profile_keeps_its_tweak_class() {
        use crate::install::plan::{RESOURCES_URL, plan_game};
        use crate::meta::loader::LoaderProfile;
        use crate::meta::loader::merge::merge_profile;
        use crate::paths::AppPaths;

        let env = RuleEnv {
            os: OsName::Windows,
            os_version: "10.0".into(),
            arch: "x86_64".into(),
        };
        let path = format!(
            "{}/tests/fixtures/forge/forge-1.12.2-version.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let profile: LoaderProfile =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut vanilla = fixture("1.8.9");
        vanilla.id = profile.inherits_from.clone();
        let version = merge_profile(&vanilla, &profile).unwrap();
        let paths = AppPaths::from_root("/data");
        let index: crate::meta::mojang::version::AssetIndex =
            serde_json::from_str(r#"{"objects":{}}"#).unwrap();
        let plan = plan_game(&version, &index, &env, &paths, RESOURCES_URL).unwrap();
        // Forge 本体は URL が空で、インストーラが置く
        assert_eq!(plan.generated.len(), 1, "{:?}", plan.generated);
        let args = build_args(&ArgInputs {
            version: &version,
            session: &player_session(),
            env: &env,
            game_dir: Path::new("/game"),
            assets_root: Path::new("/assets"),
            natives_dir: Path::new("/natives"),
            library_dir: &paths.libraries_dir(),
            classpath: &plan.classpath,
            logging_argument: None,
            memory_mb: 2048,
            extra_jvm_args: &[],
            launcher_version: "0.1.0",
        });
        let i = args.game.iter().position(|a| a == "--tweakClass").unwrap();
        assert_eq!(
            args.game[i + 1],
            "net.minecraftforge.fml.common.launcher.FMLTweaker"
        );
        assert_eq!(
            args.jvm.last().map(String::as_str),
            Some("net.minecraft.launchwrapper.Launch")
        );
        let all: Vec<&String> = args.jvm.iter().chain(&args.game).collect();
        assert!(all.iter().all(|a| !a.contains("${")), "{all:?}");
    }

    /// `${version_name}` の値（`--version` の次の引数）。
    fn version_arg(version: &VersionJson) -> String {
        let env = RuleEnv {
            os: OsName::Windows,
            os_version: "10.0".into(),
            arch: "x86_64".into(),
        };
        let args = build_args(&ArgInputs {
            version,
            session: &player_session(),
            env: &env,
            game_dir: Path::new("/game"),
            assets_root: Path::new("/assets"),
            natives_dir: Path::new("/natives"),
            library_dir: Path::new("/lib"),
            classpath: &[],
            logging_argument: None,
            memory_mb: 2048,
            extra_jvm_args: &[],
            launcher_version: "0.1.0",
        });
        let i = args.game.iter().position(|a| a == "--version").unwrap();
        args.game[i + 1].clone()
    }

    #[test]
    fn version_name_is_the_jar_owner_id() {
        let mut v = fixture("1.21.5");
        v.id = "1.20.1-forge-47.3.0".into();
        v.jar = Some("1.20.1".into());
        assert_eq!(version_arg(&v), "1.20.1");
        // jar が無ければ id のまま
        v.jar = None;
        assert_eq!(version_arg(&v), "1.20.1-forge-47.3.0");
        assert_eq!(version_arg(&fixture("1.21.5")), "1.21.5");
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

    fn launch(jvm: &[&str], game: &[&str]) -> LaunchArgs {
        LaunchArgs {
            jvm: jvm.iter().map(|s| s.to_string()).collect(),
            game: game.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// argfile にすれば収まり、直接では収まらない上限。
    fn argfile_only_limit(args: &LaunchArgs) -> usize {
        let mut with_argfile = vec!["@a.args".to_string()];
        with_argfile.extend(args.game.iter().cloned());
        command_length(Path::new("java"), &with_argfile)
    }

    #[test]
    fn main_class_ends_the_jvm_part() {
        let args = launch_args_for("1.21.5", OsName::Windows, &player_session());
        assert_eq!(
            args.jvm.last().map(String::as_str),
            Some("net.minecraft.client.main.Main")
        );
        assert_eq!(args.game.first().map(String::as_str), Some("--username"));
        let legacy = launch_args_for("1.8.9", OsName::Windows, &player_session());
        assert_eq!(
            legacy.jvm.last().map(String::as_str),
            Some("net.minecraft.client.main.Main")
        );
        assert!(legacy.game.iter().all(|a| !a.contains("${")));
    }

    #[test]
    fn short_commands_are_direct() {
        let line = fit_command(
            Path::new("java"),
            launch(&["-Xmx2048M", "Main"], &["--demo"]),
            8,
            WINDOWS_COMMAND_LIMIT,
            Path::new("/a.args"),
        )
        .unwrap();
        assert_eq!(
            line,
            CommandLine::Direct(vec!["-Xmx2048M".into(), "Main".into(), "--demo".into()])
        );
    }

    #[test]
    fn long_commands_use_an_argfile_on_java_9_and_later() {
        let cp = "x".repeat(40_000);
        let line = fit_command(
            Path::new("java"),
            launch(&["-cp", &cp, "Main"], &["--username", "Player"]),
            17,
            WINDOWS_COMMAND_LIMIT,
            Path::new("/c/a.args"),
        )
        .unwrap();
        match line {
            CommandLine::ArgFile { args, jvm } => {
                assert_eq!(
                    args,
                    vec![
                        format!("@{}", Path::new("/c/a.args").display()),
                        "--username".to_string(),
                        "Player".to_string()
                    ]
                );
                assert_eq!(jvm, vec!["-cp".to_string(), cp, "Main".to_string()]);
            }
            other => panic!("expected an argfile, got {other:?}"),
        }
    }

    #[test]
    fn long_commands_fail_on_java_8() {
        let cp = "x".repeat(40_000);
        let err = fit_command(
            Path::new("java"),
            launch(&["-cp", &cp, "Main"], &[]),
            8,
            WINDOWS_COMMAND_LIMIT,
            Path::new("/a.args"),
        )
        .unwrap_err();
        assert_eq!(err.code(), "launch.command_too_long");
    }

    #[test]
    fn argfile_never_contains_the_access_token() {
        let mut session = player_session();
        session.access_token = SecretString::from("secret-token-value".to_string());
        let args = launch_args_for("1.21.5", OsName::Windows, &session);
        let limit = argfile_only_limit(&args);
        let out = fit_command(Path::new("java"), args, 21, limit, Path::new("a.args")).unwrap();
        let CommandLine::ArgFile { args, jvm } = out else {
            panic!("expected argfile")
        };
        let contents = String::from_utf8(crate::launch::argfile::encode(&jvm).unwrap()).unwrap();
        assert!(!contents.contains("secret-token-value"));
        assert_eq!(args[0], "@a.args");
        assert!(args.iter().any(|a| a == "secret-token-value"));
    }

    #[test]
    fn argfile_ends_with_the_main_class() {
        let args = launch_args_for("1.21.5", OsName::Windows, &player_session());
        let game = args.game.clone();
        let limit = argfile_only_limit(&args);
        let out = fit_command(Path::new("java"), args, 21, limit, Path::new("a.args")).unwrap();
        let CommandLine::ArgFile { args, jvm } = out else {
            panic!("expected argfile")
        };
        assert_eq!(
            jvm.last().map(String::as_str),
            Some("net.minecraft.client.main.Main")
        );
        assert_eq!(&args[1..], game.as_slice());
    }

    #[test]
    fn too_long_even_with_argfile_is_an_error() {
        let token = "t".repeat(40_000);
        let err = fit_command(
            Path::new("java"),
            launch(&["-cp", "a.jar", "Main"], &["--accessToken", &token]),
            21,
            WINDOWS_COMMAND_LIMIT,
            Path::new("a.args"),
        )
        .unwrap_err();
        assert_eq!(err.code(), "launch.command_too_long");
    }
}
