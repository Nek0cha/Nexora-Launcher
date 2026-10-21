//! 必須の依存の解決。導入する版から必須の依存を深さ優先でたどり、ダウンロードするファイルと、有効にするだけの導入済みのファイルを決める。
//! 全体として成功するか失敗するかのどちらかで、途中の結果は返さない。

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;

use super::modrinth::{self, DependencyKind, FileInfo, FullVersion, primary_file};
use crate::error::CoreError;
pub use crate::error::DependencyCause;
use crate::net::Http;

/// 版を引く先。テストでは偽物に差し替える。
pub trait VersionSource: Sync {
    /// 版の ID から詳細を引く。
    fn version(&self, id: &str) -> impl Future<Output = Result<FullVersion, CoreError>> + Send;
    /// そのインスタンスの Minecraft のバージョンとローダーに合う最新の版。無ければ `None`。
    fn latest_for_project(
        &self,
        project_id: &str,
    ) -> impl Future<Output = Result<Option<FullVersion>, CoreError>> + Send;
}

/// Modrinth の API から引く実装。
#[derive(Clone)]
pub struct ApiSource {
    pub http: Http,
    pub api: String,
    pub mc_version: String,
    pub loaders: Vec<String>,
}

impl VersionSource for ApiSource {
    async fn version(&self, id: &str) -> Result<FullVersion, CoreError> {
        modrinth::versions_by_ids(&self.http, &self.api, &[id.to_string()])
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| CoreError::ContentDependency {
                project_id: id.to_string(),
                cause: DependencyCause::NoVersion,
                name: None,
            })
    }

    async fn latest_for_project(&self, project_id: &str) -> Result<Option<FullVersion>, CoreError> {
        let versions = modrinth::project_versions(
            &self.http,
            &self.api,
            project_id,
            &self.mc_version,
            &self.loaders,
        )
        .await?;
        Ok(modrinth::pick_version(&versions).cloned())
    }
}

/// 導入済みの 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub rel_path: String,
    pub enabled: bool,
    pub project_id: Option<String>,
}

/// ダウンロードして入れる 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub version: FullVersion,
    pub file: FileInfo,
    /// 依存として入れるか（`installed_as = dependency`）。
    pub as_dependency: bool,
    /// 更新で置き換える導入済みの `rel_path`。
    pub replaces: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstallPlan {
    /// 入れるファイル。依存が先に並ぶ。
    pub files: Vec<PlannedFile>,
    /// 有効にするだけの導入済みの `rel_path`。
    pub enable: Vec<String>,
    /// 新しい版が非互換と宣言している導入済みの `rel_path`。
    pub incompatible_installed: Vec<String>,
}

/// `root` を入れるための計画を立てる。`replacing` は更新で置き換える導入済みの `rel_path` で、
/// その行は導入済みに数えず、`root` の `replaces` に入れる。
pub async fn resolve<S: VersionSource>(
    source: &S,
    root: FullVersion,
    installed: &[Installed],
    replacing: Option<&str>,
) -> Result<InstallPlan, CoreError> {
    // 置き換える行は導入済みに数えない
    let mut by_project: HashMap<&str, &Installed> = HashMap::new();
    for row in installed {
        if Some(row.rel_path.as_str()) == replacing {
            continue;
        }
        if let Some(p) = row.project_id.as_deref() {
            // 同じ project が複数あれば、有効なものを優先する
            let keep = by_project.get(p).is_some_and(|old| old.enabled);
            if !keep {
                by_project.insert(p, row);
            }
        }
    }
    let mut state = State {
        source,
        installed: by_project,
        path: Vec::new(),
        planned: HashSet::new(),
        plan: InstallPlan::default(),
    };
    state.visit(root, false).await?;
    // 根は依存のあとに入るので、いつも最後の 1 件になる
    if let Some(root_file) = state.plan.files.last_mut() {
        root_file.replaces = replacing.map(str::to_string);
    }
    Ok(state.plan)
}

/// 解決の途中の状態。
struct State<'a, S> {
    source: &'a S,
    /// project_id から導入済みの行（置き換える行を除く）。
    installed: HashMap<&'a str, &'a Installed>,
    /// 根から今の版までの経路の project_id。循環の検出に使う。
    path: Vec<String>,
    /// 計画に入れた project_id。複数の経路から出ても 1 回だけ入れる。
    planned: HashSet<String>,
    plan: InstallPlan,
}

type VisitFuture<'f> = Pin<Box<dyn Future<Output = Result<(), CoreError>> + Send + 'f>>;

impl<'a, S: VersionSource> State<'a, S> {
    /// 版の依存を先に計画に入れ、そのあと版そのものを入れる（深さ優先）。
    fn visit(&mut self, version: FullVersion, as_dependency: bool) -> VisitFuture<'_> {
        Box::pin(async move {
            self.note_incompatible(&version);
            self.path.push(version.project_id.clone());
            for dep in &version.dependencies {
                if dep.kind != DependencyKind::Required {
                    continue;
                }
                // project_id が分かれば、取りに行く前に済んでいるかを調べる
                if let Some(p) = dep.project_id.as_deref()
                    && self.settled(p)?
                {
                    continue;
                }
                let next = match (&dep.version_id, &dep.project_id) {
                    (Some(vid), _) => self.source.version(vid).await?,
                    (None, Some(p)) => {
                        self.source.latest_for_project(p).await?.ok_or_else(|| {
                            CoreError::ContentDependency {
                                project_id: p.clone(),
                                cause: DependencyCause::NoVersion,
                                name: None,
                            }
                        })?
                    }
                    // どちらも無い依存は辿れないので無視する
                    (None, None) => continue,
                };
                if dep.project_id.is_none() && self.settled(&next.project_id)? {
                    continue;
                }
                self.visit(next, true).await?;
            }
            self.path.pop();
            let file =
                primary_file(&version)
                    .cloned()
                    .ok_or_else(|| CoreError::ContentDependency {
                        project_id: version.project_id.clone(),
                        cause: DependencyCause::NoFile,
                        name: None,
                    })?;
            self.planned.insert(version.project_id.clone());
            self.plan.files.push(PlannedFile {
                version,
                file,
                as_dependency,
                replaces: None,
            });
            Ok(())
        })
    }

    /// 依存 `project_id` がすでに満たされているか（計画済み、導入済みで有効）。
    /// 無効で導入済みなら有効にする対象に足して、満たされたとみなす。経路上にあれば循環のエラー。
    fn settled(&mut self, project_id: &str) -> Result<bool, CoreError> {
        if self.path.iter().any(|p| p == project_id) {
            return Err(CoreError::ContentDependency {
                project_id: project_id.to_string(),
                cause: DependencyCause::Cycle,
                name: None,
            });
        }
        if self.planned.contains(project_id) {
            return Ok(true);
        }
        match self.installed.get(project_id) {
            Some(row) if row.enabled => Ok(true),
            Some(row) => {
                // 無効の MOD は読み込まれないので満たさない。ダウンロードの代わりに有効にする
                if !self.plan.enable.contains(&row.rel_path) {
                    self.plan.enable.push(row.rel_path.clone());
                }
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// 版が非互換と宣言している project のうち、有効で導入済みのものを記録する。
    fn note_incompatible(&mut self, version: &FullVersion) {
        for dep in &version.dependencies {
            if dep.kind != DependencyKind::Incompatible {
                continue;
            }
            let Some(p) = dep.project_id.as_deref() else {
                continue;
            };
            if let Some(row) = self.installed.get(p) {
                // 無効の MOD は読み込まれないので衝突しない
                if row.enabled && !self.plan.incompatible_installed.contains(&row.rel_path) {
                    self.plan.incompatible_installed.push(row.rel_path.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::modrinth::Dependency;

    #[derive(Default)]
    struct Fake {
        /// 版の ID から版。
        versions: HashMap<String, FullVersion>,
        /// project_id から最新の版の ID。
        latest: HashMap<String, String>,
    }

    impl Fake {
        fn add(&mut self, v: FullVersion) {
            self.latest.insert(v.project_id.clone(), v.id.clone());
            self.versions.insert(v.id.clone(), v);
        }
    }

    impl VersionSource for Fake {
        async fn version(&self, id: &str) -> Result<FullVersion, CoreError> {
            self.versions
                .get(id)
                .cloned()
                .ok_or_else(|| CoreError::Unexpected {
                    message: format!("no version {id}"),
                })
        }

        async fn latest_for_project(
            &self,
            project_id: &str,
        ) -> Result<Option<FullVersion>, CoreError> {
            Ok(self
                .latest
                .get(project_id)
                .and_then(|id| self.versions.get(id))
                .cloned())
        }
    }

    fn file(name: &str) -> FileInfo {
        FileInfo {
            url: format!("https://cdn.example.invalid/{name}"),
            filename: name.to_string(),
            sha1: "0".repeat(40),
            sha512: None,
            size: 1,
            primary: true,
        }
    }

    fn dep(project: &str, kind: DependencyKind) -> Dependency {
        Dependency {
            project_id: Some(project.to_string()),
            version_id: None,
            kind,
        }
    }

    /// project `p` の版 `p-v`。ファイルは `p.jar`。
    fn ver(p: &str, deps: Vec<Dependency>) -> FullVersion {
        FullVersion {
            id: format!("{p}-v"),
            project_id: p.to_string(),
            version_number: "1.0.0".into(),
            version_type: "release".into(),
            game_versions: vec!["1.20.1".into()],
            loaders: vec!["fabric".into()],
            files: vec![file(&format!("{p}.jar"))],
            dependencies: deps,
        }
    }

    fn req(p: &str) -> Dependency {
        dep(p, DependencyKind::Required)
    }

    fn installed(rel: &str, enabled: bool, project: &str) -> Installed {
        Installed {
            rel_path: rel.to_string(),
            enabled,
            project_id: Some(project.to_string()),
        }
    }

    fn projects(plan: &InstallPlan) -> Vec<&str> {
        plan.files
            .iter()
            .map(|f| f.version.project_id.as_str())
            .collect()
    }

    fn assert_dependency_error(err: CoreError, project: &str, cause: DependencyCause) {
        match err {
            CoreError::ContentDependency {
                project_id,
                cause: got,
                ..
            } => {
                assert_eq!(project_id, project);
                assert_eq!(got, cause);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn required_dependencies_are_resolved_recursively() {
        let mut s = Fake::default();
        s.add(ver("b", vec![req("c")]));
        s.add(ver("c", vec![]));
        let plan = resolve(&s, ver("a", vec![req("b")]), &[], None)
            .await
            .unwrap();
        assert_eq!(projects(&plan), ["c", "b", "a"]);
        assert!(plan.files[0].as_dependency);
        assert!(plan.files[1].as_dependency);
        assert!(!plan.files[2].as_dependency);
        assert_eq!(plan.files[2].file.filename, "a.jar");
        assert!(plan.files.iter().all(|f| f.replaces.is_none()));
        assert!(plan.enable.is_empty());
        assert!(plan.incompatible_installed.is_empty());
    }

    #[tokio::test]
    async fn same_project_from_two_paths_is_planned_once() {
        let mut s = Fake::default();
        s.add(ver("b", vec![req("d")]));
        s.add(ver("c", vec![req("d")]));
        s.add(ver("d", vec![]));
        let plan = resolve(&s, ver("a", vec![req("b"), req("c")]), &[], None)
            .await
            .unwrap();
        assert_eq!(projects(&plan), ["d", "b", "c", "a"]);
    }

    #[tokio::test]
    async fn optional_and_embedded_dependencies_are_ignored() {
        let mut s = Fake::default();
        s.add(ver("b", vec![]));
        s.add(ver("c", vec![]));
        let root = ver(
            "a",
            vec![
                dep("b", DependencyKind::Optional),
                dep("c", DependencyKind::Embedded),
            ],
        );
        let plan = resolve(&s, root, &[], None).await.unwrap();
        assert_eq!(projects(&plan), ["a"]);
    }

    #[tokio::test]
    async fn already_installed_enabled_dependency_is_not_installed_again() {
        let mut s = Fake::default();
        s.add(ver("b", vec![]));
        let plan = resolve(
            &s,
            ver("a", vec![req("b")]),
            &[installed("mods/b.jar", true, "b")],
            None,
        )
        .await
        .unwrap();
        assert_eq!(projects(&plan), ["a"]);
        assert!(plan.enable.is_empty());
    }

    #[tokio::test]
    async fn disabled_installed_dependency_is_enabled_instead_of_downloaded() {
        let mut s = Fake::default();
        s.add(ver("b", vec![]));
        let plan = resolve(
            &s,
            ver("a", vec![req("b")]),
            &[installed("mods/b.jar.disabled", false, "b")],
            None,
        )
        .await
        .unwrap();
        assert_eq!(plan.enable, ["mods/b.jar.disabled"]);
        assert_eq!(projects(&plan), ["a"]);
    }

    #[tokio::test]
    async fn cycle_is_an_error_naming_the_project() {
        let mut s = Fake::default();
        s.add(ver("b", vec![req("a")]));
        let err = resolve(&s, ver("a", vec![req("b")]), &[], None)
            .await
            .unwrap_err();
        assert_dependency_error(err, "a", DependencyCause::Cycle);
    }

    #[tokio::test]
    async fn version_without_files_is_an_error() {
        let mut s = Fake::default();
        let mut b = ver("b", vec![]);
        b.files.clear();
        s.add(b);
        let err = resolve(&s, ver("a", vec![req("b")]), &[], None)
            .await
            .unwrap_err();
        assert_dependency_error(err, "b", DependencyCause::NoFile);

        // 導入する版そのものにファイルが無いときも同じ
        let mut root = ver("a", vec![]);
        root.files.clear();
        let err = resolve(&Fake::default(), root, &[], None)
            .await
            .unwrap_err();
        assert_dependency_error(err, "a", DependencyCause::NoFile);
    }

    #[tokio::test]
    async fn dependency_without_matching_version_is_an_error() {
        let s = Fake::default();
        let err = resolve(&s, ver("a", vec![req("b")]), &[], None)
            .await
            .unwrap_err();
        assert_dependency_error(err, "b", DependencyCause::NoVersion);
    }

    #[tokio::test]
    async fn incompatible_installed_mod_is_reported() {
        let mut s = Fake::default();
        s.add(ver("b", vec![]));
        let root = ver("a", vec![req("b"), dep("x", DependencyKind::Incompatible)]);
        let plan = resolve(
            &s,
            root,
            &[
                installed("mods/x.jar", true, "x"),
                installed("mods/y.jar", true, "y"),
            ],
            None,
        )
        .await
        .unwrap();
        assert_eq!(plan.incompatible_installed, ["mods/x.jar"]);
        assert_eq!(projects(&plan), ["b", "a"]);
    }

    #[tokio::test]
    async fn replacing_marks_the_replaced_row() {
        let s = Fake::default();
        let plan = resolve(
            &s,
            ver("a", vec![]),
            &[installed("mods/a.jar.disabled", false, "a")],
            Some("mods/a.jar.disabled"),
        )
        .await
        .unwrap();
        assert_eq!(projects(&plan), ["a"]);
        assert_eq!(
            plan.files[0].replaces.as_deref(),
            Some("mods/a.jar.disabled")
        );
        // 置き換える行は導入済みに数えないので、有効にする対象にもならない
        assert!(plan.enable.is_empty());

        // 依存が増えるときは、置き換えは根（最後の 1 件）にだけ付く
        let mut s = Fake::default();
        s.add(ver("b", vec![]));
        let plan = resolve(
            &s,
            ver("a", vec![req("b")]),
            &[installed("mods/a.jar", true, "a")],
            Some("mods/a.jar"),
        )
        .await
        .unwrap();
        assert_eq!(projects(&plan), ["b", "a"]);
        assert!(plan.files[0].replaces.is_none());
        assert_eq!(plan.files[1].replaces.as_deref(), Some("mods/a.jar"));
    }

    #[tokio::test]
    async fn dependency_with_only_version_id_is_fetched_by_version() {
        let mut s = Fake::default();
        // 最新ではない版を指定する
        let mut old = ver("b", vec![]);
        old.id = "b-old".into();
        old.files = vec![file("b-old.jar")];
        s.versions.insert(old.id.clone(), old);
        s.add(ver("b", vec![]));
        let root = ver(
            "a",
            vec![Dependency {
                project_id: None,
                version_id: Some("b-old".into()),
                kind: DependencyKind::Required,
            }],
        );
        let plan = resolve(&s, root, &[], None).await.unwrap();
        assert_eq!(projects(&plan), ["b", "a"]);
        assert_eq!(plan.files[0].version.id, "b-old");
        assert_eq!(plan.files[0].file.filename, "b-old.jar");
        assert!(plan.files[0].as_dependency);
    }
}
