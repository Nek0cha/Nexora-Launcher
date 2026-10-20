//! `install_profile.json` の `data` を解決し、クライアントで実行する processor を計画する。副作用を持たない。

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use crate::error::CoreError;
use crate::install::forge::profile::InstallProfile;

/// `data` とトークンの解決に使う、導入先の場所。
pub struct DataContext<'a> {
    pub libraries_dir: &'a Path,
    /// `{ROOT}`。`shared/`。
    pub root: &'a Path,
    pub installer: &'a Path,
    /// `{MINECRAFT_JAR}`。Vanilla のクライアントの jar。
    pub minecraft_jar: &'a Path,
    pub minecraft_version: &'a str,
    /// 取り出した資源（`/data/client.lzma` など、キーは `data` の値そのまま）の置き場所。
    pub resources: &'a BTreeMap<String, PathBuf>,
}

/// processor が作るはずのファイルと、期待する sha1。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedOutput {
    pub path: PathBuf,
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedProcessor {
    /// ログとエラーの表示用（processor の `jar` の座標）。
    pub name: String,
    pub jar: PathBuf,
    pub classpath: Vec<PathBuf>,
    pub args: Vec<String>,
    pub outputs: Vec<ExpectedOutput>,
}

/// クライアントの導入に必要な資源の名前（`data` のうち `client` の値が `/` で始まるもの）。
pub fn client_resources(profile: &InstallProfile) -> Vec<String> {
    let mut names: Vec<String> = profile
        .data
        .values()
        .filter(|v| v.client.starts_with('/'))
        .map(|v| v.client.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// `sides` が無いか `client` を含む processor だけを、宣言の順に計画する。
pub fn plan_processors(
    profile: &InstallProfile,
    ctx: &DataContext,
) -> Result<Vec<PlannedProcessor>, CoreError> {
    let resolver = Resolver { profile, ctx };
    profile
        .processors
        .iter()
        .filter(|p| {
            p.sides
                .as_ref()
                .is_none_or(|sides| sides.iter().any(|s| s == "client"))
        })
        .map(|p| {
            let jar = resolver.library(&p.jar)?;
            let classpath = p
                .classpath
                .iter()
                .map(|c| resolver.library(c))
                .collect::<Result<Vec<_>, _>>()?;
            let args = p
                .args
                .iter()
                .map(|a| resolver.argument(a))
                .collect::<Result<Vec<_>, _>>()?;
            let outputs = p
                .outputs
                .iter()
                .map(|(path, sha1)| {
                    Ok(ExpectedOutput {
                        path: resolver.output_path(path)?,
                        sha1: resolver.output_sha1(sha1)?,
                    })
                })
                .collect::<Result<Vec<_>, CoreError>>()?;
            Ok(PlannedProcessor {
                name: p.jar.clone(),
                jar,
                classpath,
                args,
                outputs,
            })
        })
        .collect()
}

/// `data` の `client` の値のうち、`[座標]` の形のもののライブラリのパス（名前の順）。
/// processor が作るもの（`{PATCHED}` など）が入る。ファイルがあるかは確かめない。
pub fn data_libraries(
    profile: &InstallProfile,
    ctx: &DataContext,
) -> Result<Vec<PathBuf>, CoreError> {
    let resolver = Resolver { profile, ctx };
    profile
        .data
        .values()
        .filter_map(|v| coordinate(&v.client))
        .map(|coord| resolver.library(coord))
        .collect()
}

/// 側が `client` のときのトークン `{SIDE}` の値。
const SIDE: &str = "client";

fn invalid(message: String) -> CoreError {
    CoreError::InvalidMetadata { message }
}

/// トークンや `data` を解決した値。
enum Value {
    /// ファイルのパス。
    Path(PathBuf),
    /// パスでない文字列（`'...'` の中身、`{SIDE}`、`{MINECRAFT_VERSION}`）。
    Text(String),
}

/// 1 つの文字列を展開した結果。
struct Expanded {
    text: String,
    /// パスのトークンを 1 つ以上含む。
    has_path: bool,
    /// profile が書いた部分（トークンの外の文字列と、`'...'` などの文字列の値）の、
    /// パスのトークンで区切られた連なり。値と文字列が続けて `..` を作る場合も 1 つとして見る。
    profile_runs: Vec<String>,
    /// 最初のパスのトークンより後に、profile が書いた部分があり、そこに `:` を含む。
    colon_after_path: bool,
}

impl Expanded {
    /// パスのトークンを含むとき、profile が書いた部分が `..` で上に出たり、
    /// `:`（Windows のドライブや代替データストリーム）で別の場所を指したりしないか。
    fn escapes(&self) -> bool {
        self.has_path
            && (self.colon_after_path || self.profile_runs.iter().any(|r| has_parent_segment(r)))
    }
}

/// `[group:artifact:version[:classifier][@ext]]` の形なら、括弧の中身。
fn coordinate(s: &str) -> Option<&str> {
    s.strip_prefix('[')?.strip_suffix(']')
}

/// `'...'` の形なら、引用符の中身。
fn quoted(s: &str) -> Option<&str> {
    s.strip_prefix('\'')?.strip_suffix('\'')
}

fn path_str(path: &Path) -> Result<&str, CoreError> {
    path.to_str().ok_or(CoreError::PathNotEncodable)
}

/// `..` の区切りを含むか（`/` と `\` のどちらも区切りとみなす）。
fn has_parent_segment(s: &str) -> bool {
    s.split(['/', '\\']).any(|seg| seg == "..")
}

struct Resolver<'a> {
    profile: &'a InstallProfile,
    ctx: &'a DataContext<'a>,
}

impl Resolver<'_> {
    /// Maven の座標を `libraries_dir` の下のパスにする。`root` の外に出るものは `InvalidMetadata`。
    fn library(&self, coord: &str) -> Result<PathBuf, CoreError> {
        let rel = crate::install::plan::maven_path(coord)
            .filter(|rel| crate::install::plan::is_safe_relative(rel))
            .ok_or_else(|| invalid(format!("install profile has a bad coordinate {coord:?}")))?;
        let path = self.ctx.libraries_dir.join(rel);
        if !path.starts_with(self.ctx.root) {
            return Err(invalid(format!(
                "install profile library {coord:?} is outside the shared directory"
            )));
        }
        Ok(path)
    }

    /// `{名前}` の値。組み込みのトークンを `data` より先に見る。
    fn token(&self, name: &str) -> Result<Value, CoreError> {
        let ctx = self.ctx;
        Ok(match name {
            "SIDE" => Value::Text(SIDE.into()),
            "MINECRAFT_VERSION" => Value::Text(ctx.minecraft_version.into()),
            "MINECRAFT_JAR" => Value::Path(ctx.minecraft_jar.into()),
            "ROOT" => Value::Path(ctx.root.into()),
            "INSTALLER" => Value::Path(ctx.installer.into()),
            "LIBRARY_DIR" => Value::Path(ctx.libraries_dir.into()),
            _ => {
                let value = self.profile.data.get(name).ok_or_else(|| {
                    invalid(format!("install profile uses an unknown token {{{name}}}"))
                })?;
                self.data(name, &value.client)?
            }
        })
    }

    /// `data` の `client` の値を解決する。
    fn data(&self, name: &str, raw: &str) -> Result<Value, CoreError> {
        if let Some(coord) = coordinate(raw) {
            Ok(Value::Path(self.library(coord)?))
        } else if let Some(text) = quoted(raw) {
            Ok(Value::Text(text.into()))
        } else if raw.starts_with('/') {
            self.ctx
                .resources
                .get(raw)
                .map(|p| Value::Path(p.clone()))
                .ok_or_else(|| invalid(format!("installer resource {raw} was not extracted")))
        } else {
            Err(invalid(format!(
                "install profile data {name} has an unsupported value {raw:?}"
            )))
        }
    }

    /// `{名前}` を（文字列のどこにあっても）1 回の走査で展開する。展開した値の中はもう一度展開しない。
    /// パスに profile の文字列を続けたもの（`{ROOT}/run.sh`）は、Windows では `\` と `/` が混ざるが、
    /// どちらも区切りとして扱われる。
    fn expand(&self, s: &str) -> Result<Expanded, CoreError> {
        let mut out = Expanded {
            text: String::new(),
            has_path: false,
            profile_runs: Vec::new(),
            colon_after_path: false,
        };
        let mut run = String::new();
        // profile が書いた部分を足す
        let push_profile = |out: &mut Expanded, run: &mut String, text: &str| {
            if out.has_path && text.contains(':') {
                out.colon_after_path = true;
            }
            run.push_str(text);
            out.text.push_str(text);
        };
        let mut rest = s;
        while let Some(start) = rest.find('{') {
            let (literal, after) = rest.split_at(start);
            let Some(end) = after.find('}') else {
                return Err(invalid(format!(
                    "install profile has an unclosed token in {s:?}"
                )));
            };
            push_profile(&mut out, &mut run, literal);
            match self.token(&after[1..end])? {
                Value::Path(path) => {
                    out.profile_runs.push(std::mem::take(&mut run));
                    out.has_path = true;
                    out.text.push_str(path_str(&path)?);
                }
                Value::Text(text) => push_profile(&mut out, &mut run, &text),
            }
            rest = &after[end + 1..];
        }
        push_profile(&mut out, &mut run, rest);
        out.profile_runs.push(run);
        Ok(out)
    }

    /// processor の引数。`[座標]` はライブラリのパスにし、パスのトークンを含むものは
    /// profile が書いた部分が上に出ないことを確かめる。パスのトークンを含まない引数はそのまま。
    fn argument(&self, arg: &str) -> Result<String, CoreError> {
        if let Some(coord) = coordinate(arg) {
            return Ok(path_str(&self.library(coord)?)?.to_string());
        }
        let expanded = self.expand(arg)?;
        if expanded.escapes() {
            return Err(invalid(format!(
                "install profile argument {arg:?} leaves its directory"
            )));
        }
        Ok(expanded.text)
    }

    /// `outputs` のキー。`root` の下の通常の区切りだけからなるパスでなければ `InvalidMetadata`。
    /// `Path::starts_with` は `..` を解決しないため、`root` から先の各部分を確かめる。
    fn output_path(&self, key: &str) -> Result<PathBuf, CoreError> {
        if let Some(coord) = coordinate(key) {
            return self.library(coord);
        }
        let expanded = self.expand(key)?;
        let path = PathBuf::from(&expanded.text);
        let inside = !expanded.escapes()
            && path.strip_prefix(self.ctx.root).is_ok_and(|rel| {
                rel.components().next().is_some()
                    && rel.components().all(|c| match c {
                        Component::Normal(part) => part.to_str().is_some_and(|p| !p.contains(':')),
                        _ => false,
                    })
            });
        if !inside {
            return Err(invalid(format!(
                "install profile output {key:?} is outside the shared directory"
            )));
        }
        Ok(path)
    }

    /// `outputs` の値。展開した後の 40 桁の 16 進数で、空なら `None`。
    fn output_sha1(&self, value: &str) -> Result<Option<String>, CoreError> {
        let text = match quoted(value) {
            Some(text) => text.to_string(),
            None => {
                let expanded = self.expand(value)?;
                if expanded.has_path {
                    return Err(invalid(format!(
                        "install profile output hash {value:?} is a path"
                    )));
                }
                expanded.text
            }
        };
        if text.is_empty() {
            return Ok(None);
        }
        if text.len() != 40 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid(format!(
                "install profile output hash {value:?} is not a sha1"
            )));
        }
        Ok(Some(text.to_ascii_lowercase()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::forge::profile::parse_install_profile;
    use crate::install::plan::maven_path;

    fn fixture(name: &str) -> InstallProfile {
        let path = format!(
            "{}/tests/fixtures/forge/{name}-install_profile.json",
            env!("CARGO_MANIFEST_DIR")
        );
        parse_install_profile(&std::fs::read(path).unwrap()).unwrap()
    }

    /// テスト用の導入先。`root` の下に `libraries/` と `versions/` を置く。
    struct Env {
        root: PathBuf,
        libraries: PathBuf,
        installer: PathBuf,
        minecraft_jar: PathBuf,
        resources: BTreeMap<String, PathBuf>,
    }

    impl Env {
        fn new(base: &Path) -> Self {
            let root = base.join("shared");
            let mut resources = BTreeMap::new();
            resources.insert(
                "/data/client.lzma".to_string(),
                base.join("staging").join("client.lzma"),
            );
            Self {
                libraries: root.join("libraries"),
                installer: base.join("cache").join("installers").join("forge.jar"),
                minecraft_jar: root.join("versions").join("1.20.1").join("1.20.1.jar"),
                resources,
                root,
            }
        }

        fn ctx(&self) -> DataContext<'_> {
            DataContext {
                libraries_dir: &self.libraries,
                root: &self.root,
                installer: &self.installer,
                minecraft_jar: &self.minecraft_jar,
                minecraft_version: "1.20.1",
                resources: &self.resources,
            }
        }

        fn library(&self, coord: &str) -> PathBuf {
            self.libraries.join(maven_path(coord).unwrap())
        }
    }

    fn env() -> Env {
        Env::new(&PathBuf::from("nexora"))
    }

    fn s(path: &Path) -> String {
        path.to_str().unwrap().to_string()
    }

    fn profile(json: serde_json::Value) -> InstallProfile {
        parse_install_profile(json.to_string().as_bytes()).unwrap()
    }

    /// 1 つの processor だけを持つ profile。
    fn single(data: serde_json::Value, processor: serde_json::Value) -> InstallProfile {
        profile(serde_json::json!({
            "spec": 1,
            "minecraft": "1.20.1",
            "json": "/version.json",
            "data": data,
            "processors": [processor],
        }))
    }

    fn names(planned: &[PlannedProcessor]) -> Vec<&str> {
        planned.iter().map(|p| p.name.as_str()).collect()
    }

    #[test]
    fn forge_1_16_5_plans_four_client_processors() {
        let env = env();
        let planned = plan_processors(&fixture("forge-1.16.5"), &env.ctx()).unwrap();
        assert_eq!(
            names(&planned),
            [
                "net.minecraftforge:installertools:1.2.6",
                "net.minecraftforge:jarsplitter:1.1.4",
                "net.md-5:SpecialSource:1.8.5",
                "net.minecraftforge:binarypatcher:1.0.12",
            ]
        );
        for p in &planned {
            assert_eq!(p.jar, env.library(&p.name));
        }
        assert_eq!(planned[0].classpath.len(), 17);
        assert!(
            planned
                .iter()
                .all(|p| p.classpath.iter().all(|c| c.starts_with(&env.libraries)))
        );

        // jarsplitter は slim と extra の 2 つ（BTreeMap の順）
        assert_eq!(
            planned[1].outputs,
            [
                ExpectedOutput {
                    path: env.library("net.minecraft:client:1.16.5-20210115.111550:extra"),
                    sha1: Some("4aef4168f94d97bc73bf510213b2518942f931d1".into()),
                },
                ExpectedOutput {
                    path: env.library("net.minecraft:client:1.16.5-20210115.111550:slim"),
                    sha1: Some("045d6edac8fe1b004e159c49704b5b729f551079".into()),
                },
            ]
        );
        assert!(planned[0].outputs.is_empty());
        assert!(planned[2].outputs.is_empty());
        assert_eq!(
            planned[3].outputs,
            [ExpectedOutput {
                path: env.libraries.join(
                    "net/minecraftforge/forge/1.16.5-36.2.39/forge-1.16.5-36.2.39-client.jar"
                ),
                sha1: Some("07678ff3edd6990a75bf0707ad2dbb2315b8000f".into()),
            }]
        );
        // `{MINECRAFT_JAR}` は与えた jar のパス
        assert_eq!(planned[1].args[0], "--input");
        assert_eq!(planned[1].args[1], s(&env.minecraft_jar));
    }

    #[test]
    fn data_libraries_are_the_coordinate_values() {
        let env = env();
        let paths = data_libraries(&fixture("forge-1.20.1"), &env.ctx()).unwrap();
        // PATCHED は processor が作る jar。`/data/...` や `'...'` の値は入らない
        assert!(
            paths.contains(
                &env.libraries
                    .join("net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-client.jar")
            )
        );
        assert!(!paths.is_empty());
        assert!(paths.iter().all(|p| p.starts_with(&env.libraries)));
        let profile = single(
            serde_json::json!({
                "A": { "client": "/data/client.lzma", "server": "" },
                "B": { "client": "'text'", "server": "" },
                "C": { "client": "[a:b:1]", "server": "" },
            }),
            serde_json::json!({ "jar": "a:b:1", "args": [] }),
        );
        assert_eq!(
            data_libraries(&profile, &env.ctx()).unwrap(),
            [env.library("a:b:1")]
        );
    }

    #[test]
    fn server_only_processors_are_skipped() {
        let env = env();
        for name in ["forge-1.20.1", "neoforge-1.20.1", "neoforge-21.1"] {
            let profile = fixture(name);
            let planned = plan_processors(&profile, &env.ctx()).unwrap();
            // 10 のうち server だけの 4 つを除く
            assert_eq!(planned.len(), 6, "{name}");
            let tasks: Vec<_> = planned
                .iter()
                .map(|p| p.args.get(1).cloned().unwrap_or_default())
                .collect();
            assert!(
                !tasks
                    .iter()
                    .any(|t| t == "EXTRACT_FILES" || t == "BUNDLER_EXTRACT"),
                "{name}: {tasks:?}"
            );
            // client の jarsplitter は Vanilla の jar を入力にする（server のは `{MC_UNPACKED}`）
            let splitter = planned
                .iter()
                .find(|p| p.name.contains("jarsplitter"))
                .unwrap();
            assert_eq!(splitter.args[1], s(&env.minecraft_jar), "{name}");
        }
        let planned = plan_processors(&fixture("forge-1.20.1"), &env.ctx()).unwrap();
        assert_eq!(
            names(&planned),
            [
                "net.minecraftforge:installertools:1.4.1",
                "net.minecraftforge:installertools:1.4.1",
                "net.minecraftforge:installertools:1.4.1",
                "net.minecraftforge:jarsplitter:1.1.4",
                "net.minecraftforge:ForgeAutoRenamingTool:0.1.22:all",
                "net.minecraftforge:binarypatcher:1.1.1",
            ]
        );
        // 1.20.1 で `outputs` を持つのは jarsplitter だけ
        assert_eq!(
            planned[3].outputs,
            [
                ExpectedOutput {
                    path: env.library("net.minecraft:client:1.20.1-20230612.114412:extra"),
                    sha1: Some("8c5a95cbce940cfdb304376ae9fea47968d02587".into()),
                },
                ExpectedOutput {
                    path: env.library("net.minecraft:client:1.20.1-20230612.114412:slim"),
                    sha1: Some("de86b035d2da0f78940796bb95c39a932ed84834".into()),
                },
            ]
        );
        assert!(planned[5].outputs.is_empty());
        // `--side {SIDE}` は client
        assert_eq!(
            planned[1].args[..6],
            [
                "--task",
                "DOWNLOAD_MOJMAPS",
                "--version",
                "1.20.1",
                "--side",
                "client"
            ]
        );
    }

    #[test]
    fn processor_without_sides_runs_on_client() {
        let env = env();
        let profile = profile(serde_json::json!({
            "spec": 1,
            "minecraft": "1.20.1",
            "json": "/version.json",
            "processors": [
                {"jar": "a:none:1", "args": ["1"]},
                {"sides": ["server"], "jar": "a:server:1", "args": ["2"]},
                {"sides": ["client", "server"], "jar": "a:both:1", "args": ["3"]},
                {"sides": ["client"], "jar": "a:client:1", "args": ["4"]},
            ],
        }));
        let planned = plan_processors(&profile, &env.ctx()).unwrap();
        assert_eq!(names(&planned), ["a:none:1", "a:both:1", "a:client:1"]);
        assert_eq!(planned[0].args, ["1"]);
        assert!(planned[0].classpath.is_empty());
    }

    #[test]
    fn neoforge_21_1_processors_have_no_output_hashes() {
        let env = env();
        let planned = plan_processors(&fixture("neoforge-21.1"), &env.ctx()).unwrap();
        assert_eq!(planned.len(), 6);
        assert!(planned.iter().all(|p| p.outputs.is_empty()));
        assert_eq!(
            planned.last().unwrap().name,
            "net.neoforged.installertools:binarypatcher:2.1.2:fatjar"
        );
        assert_eq!(
            planned.last().unwrap().args[3],
            s(&env.library("net.neoforged:neoforge:21.1.172:client"))
        );
    }

    #[test]
    fn literal_quotes_are_removed() {
        let env = env();
        let profile = single(
            serde_json::json!({
                "VERSION": {"client": "'20230612.114412'", "server": ""},
                "OUT": {"client": "[a:out:1]", "server": ""},
                "OUT_SHA": {"client": "'de86b035d2da0f78940796bb95c39a932ed84834'", "server": ""},
                // 引用符の中身は展開しない（1 回の走査）
                "RAW": {"client": "'{ROOT}'", "server": ""},
            }),
            serde_json::json!({
                "jar": "a:tool:1",
                "args": ["--version", "{VERSION}", "--raw", "{RAW}", "--mc={MINECRAFT_VERSION}"],
                "outputs": {
                    "{OUT}": "{OUT_SHA}",
                    "[a:other:1]": "'4b6c41d915c857a006634a12b6cdf9e08451a4fd'",
                    "{ROOT}/empty.txt": "",
                },
            }),
        );
        let planned = plan_processors(&profile, &env.ctx()).unwrap();
        assert_eq!(
            planned[0].args,
            [
                "--version",
                "20230612.114412",
                "--raw",
                "{ROOT}",
                "--mc=1.20.1"
            ]
        );
        let mut outputs = planned[0].outputs.clone();
        outputs.sort_by(|a, b| a.path.cmp(&b.path));
        let mut expected = vec![
            ExpectedOutput {
                path: env.library("a:out:1"),
                sha1: Some("de86b035d2da0f78940796bb95c39a932ed84834".into()),
            },
            ExpectedOutput {
                path: env.library("a:other:1"),
                sha1: Some("4b6c41d915c857a006634a12b6cdf9e08451a4fd".into()),
            },
            ExpectedOutput {
                path: PathBuf::from(format!("{}/empty.txt", s(&env.root))),
                sha1: None,
            },
        ];
        expected.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(outputs, expected);
    }

    #[test]
    fn bracket_arguments_resolve_to_library_paths() {
        let env = env();
        let planned = plan_processors(&fixture("forge-1.16.5"), &env.ctx()).unwrap();
        assert_eq!(
            planned[0].args,
            [
                "--task".to_string(),
                "MCP_DATA".into(),
                "--input".into(),
                s(&env.libraries.join(
                    "de/oceanlabs/mcp/mcp_config/1.16.5-20210115.111550/mcp_config-1.16.5-20210115.111550.zip"
                )),
                "--output".into(),
                s(&env.libraries.join(
                    "de/oceanlabs/mcp/mcp_config/1.16.5-20210115.111550/mcp_config-1.16.5-20210115.111550-mappings.txt"
                )),
                "--key".into(),
                "mappings".into(),
            ]
        );
    }

    #[test]
    fn resource_arguments_use_extracted_path() {
        let env = env();
        let profile = fixture("forge-1.16.5");
        let planned = plan_processors(&profile, &env.ctx()).unwrap();
        let patcher = &planned[3];
        assert_eq!(patcher.args[4], "--apply");
        assert_eq!(patcher.args[5], s(&env.resources["/data/client.lzma"]));

        // 取り出していない資源は `meta.invalid`
        let mut missing = Env::new(&PathBuf::from("nexora"));
        missing.resources.clear();
        let err = plan_processors(&profile, &missing.ctx()).unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
        assert!(err.to_string().contains("/data/client.lzma"), "{err}");
    }

    #[test]
    fn client_resources_lists_lzma() {
        for name in [
            "forge-1.16.5",
            "forge-1.20.1",
            "neoforge-21.1",
            "neoforge-1.20.1",
        ] {
            assert_eq!(
                client_resources(&fixture(name)),
                ["/data/client.lzma"],
                "{name}"
            );
        }
    }

    #[test]
    fn unknown_token_is_invalid() {
        let env = env();
        for args in [
            serde_json::json!(["{NOPE}"]),
            serde_json::json!(["--x={NOPE}/a"]),
            serde_json::json!(["{}"]),
            // 閉じていない
            serde_json::json!(["{ROOT"]),
        ] {
            let profile = single(
                serde_json::json!({}),
                serde_json::json!({"jar": "a:tool:1", "args": args}),
            );
            let err = plan_processors(&profile, &env.ctx()).unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "{args}");
        }
        // `outputs` の値の未知のトークンも同じ
        let profile = single(
            serde_json::json!({}),
            serde_json::json!({"jar": "a:tool:1", "outputs": {"{ROOT}/a": "{NOPE}"}}),
        );
        let err = plan_processors(&profile, &env.ctx()).unwrap_err();
        assert_eq!(err.code(), "meta.invalid");
    }

    #[test]
    fn output_outside_root_is_invalid() {
        let env = env();
        let cases = [
            serde_json::json!({"jar": "a:tool:1", "outputs": {"{ROOT}/../x": ""}}),
            serde_json::json!({"jar": "a:tool:1", "outputs": {"{ROOT}/a/../../x": ""}}),
            // root の外の与えられたパスへの出力
            serde_json::json!({"jar": "a:tool:1", "outputs": {"{INSTALLER}": ""}}),
            // パスでない出力
            serde_json::json!({"jar": "a:tool:1", "outputs": {"x.jar": ""}}),
            serde_json::json!({"jar": "a:tool:1", "outputs": {"[a:..:1]": ""}}),
            // sha1 でない値
            serde_json::json!({"jar": "a:tool:1", "outputs": {"{ROOT}/a": "'abc'"}}),
            // 引数の `..` と、壊れた座標
            serde_json::json!({"jar": "a:tool:1", "args": ["{ROOT}/../x"]}),
            serde_json::json!({"jar": "a:tool:1", "args": ["--to={ROOT}/a/../../x"]}),
            serde_json::json!({"jar": "a:tool:1", "args": ["[a:c:1/../../x]"]}),
            serde_json::json!({"jar": "a:tool:1", "args": ["[broken]"]}),
            serde_json::json!({"jar": "a:..:1"}),
            serde_json::json!({"jar": "broken"}),
            serde_json::json!({"jar": "a:tool:1", "classpath": ["a:c:1@x/y"]}),
        ];
        for processor in cases {
            let profile = single(serde_json::json!({}), processor.clone());
            let err = plan_processors(&profile, &env.ctx()).unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "{processor}");
        }
        // `data` の引用符の値も profile が書いたもので、`..` や `:` を含めば root の外に出うる
        let data = serde_json::json!({
            "UP": {"client": "'..'", "server": ""},
            "DOT": {"client": "'.'", "server": ""},
            "ESC": {"client": "'/../x'", "server": ""},
            "DRIVE": {"client": "'C:'", "server": ""},
        });
        for processor in [
            serde_json::json!({"jar": "a:tool:1", "outputs": {"{ROOT}/{UP}/{UP}/evil.jar": ""}}),
            serde_json::json!({"jar": "a:tool:1", "outputs": {"{ROOT}{ESC}": ""}}),
            serde_json::json!({"jar": "a:tool:1", "outputs": {"{ROOT}/.{DOT}/x": ""}}),
            serde_json::json!({"jar": "a:tool:1", "outputs": {"{ROOT}/{DRIVE}/x": ""}}),
            serde_json::json!({"jar": "a:tool:1", "outputs": {"{ROOT}/a:b": ""}}),
            serde_json::json!({"jar": "a:tool:1", "args": ["{ROOT}/{UP}/x"]}),
            serde_json::json!({"jar": "a:tool:1", "args": ["--to={ROOT}{ESC}"]}),
            serde_json::json!({"jar": "a:tool:1", "args": ["{ROOT}/.{DOT}/x"]}),
            serde_json::json!({"jar": "a:tool:1", "args": ["{ROOT}/{DRIVE}/x"]}),
            serde_json::json!({"jar": "a:tool:1", "args": ["{ROOT}/a:b"]}),
        ] {
            let profile = single(data.clone(), processor.clone());
            let err = plan_processors(&profile, &env.ctx()).unwrap_err();
            assert_eq!(err.code(), "meta.invalid", "{processor}");
        }
        // パスのトークンを含まない引数は、`..` を含む値を展開してもそのまま
        let profile = single(
            data,
            serde_json::json!({"jar": "a:tool:1", "args": ["{UP}", "--from={UP}/x", "{DRIVE}"]}),
        );
        assert_eq!(
            plan_processors(&profile, &env.ctx()).unwrap()[0].args,
            ["..", "--from=../x", "C:"]
        );
        // `data` の座標が壊れていても同じ
        let profile = single(
            serde_json::json!({"BAD": {"client": "[a:c:..]", "server": ""}}),
            serde_json::json!({"jar": "a:tool:1", "args": ["{BAD}"]}),
        );
        assert_eq!(
            plan_processors(&profile, &env.ctx()).unwrap_err().code(),
            "meta.invalid"
        );
    }

    #[test]
    fn non_path_arguments_pass_through() {
        let env = env();
        let args = serde_json::json!([
            "--task",
            "EXTRACT_FILES",
            "--from",
            "data/run.sh",
            "--from",
            "../not/a/path",
            "1.20.1",
            "--ann-fix",
            "-Dx=y",
            "",
        ]);
        let profile = single(
            serde_json::json!({}),
            serde_json::json!({"jar": "a:tool:1", "args": args.clone()}),
        );
        let planned = plan_processors(&profile, &env.ctx()).unwrap();
        let expected: Vec<String> = serde_json::from_value(args).unwrap();
        assert_eq!(planned[0].args, expected);
        // root の中に収まる `{ROOT}` の下のパスはそのまま
        let profile = single(
            serde_json::json!({}),
            serde_json::json!({"jar": "a:tool:1", "args": ["--to={ROOT}/libraries/win_args.txt", "{INSTALLER}"]}),
        );
        let planned = plan_processors(&profile, &env.ctx()).unwrap();
        assert_eq!(
            planned[0].args,
            [
                format!("--to={}/libraries/win_args.txt", s(&env.root)),
                s(&env.installer),
            ]
        );
    }

    #[test]
    fn argument_with_spaces_stays_one_argument() {
        let env = Env::new(&PathBuf::from(r"C:\Users\山田 太郎\nexora"));
        let planned = plan_processors(&fixture("forge-1.20.1"), &env.ctx()).unwrap();
        let splitter = &planned[3];
        assert_eq!(splitter.args.len(), 8);
        assert_eq!(splitter.args[1], s(&env.minecraft_jar));
        assert_eq!(
            splitter.args[3],
            s(&env.library("net.minecraft:client:1.20.1-20230612.114412:slim"))
        );
        assert!(splitter.args[3].contains("山田 太郎"));
        let patcher = &planned[5];
        assert_eq!(patcher.args.len(), 6);
        assert!(patcher.args[3].contains("山田 太郎"), "{:?}", patcher.args);
        assert_eq!(
            patcher.jar,
            env.library("net.minecraftforge:binarypatcher:1.1.1")
        );
    }
}
