//! core 全体で使うエラー型と、UI に渡すエラーの形。

use std::collections::BTreeMap;

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("the OS data directory could not be determined")]
    DataDirUnavailable,
    #[error("I/O error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("database migration failed: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("invalid value for setting {key}")]
    InvalidSetting { key: String },
    #[error("task {task_id} was not found")]
    TaskNotFound { task_id: String },
    #[error("the operation was cancelled")]
    Cancelled,
    #[error("network unavailable: {message}")]
    Offline { message: String },
    #[error("HTTP {status} from {url}")]
    HttpStatus { status: u16, url: String },
    #[error("downloaded file did not match its expected hash: {path}")]
    HashMismatch { path: String },
    #[error("invalid metadata: {message}")]
    InvalidMetadata { message: String },
    #[error("version {id} was not found in the version manifest")]
    VersionNotFound { id: String },
    #[error("unsupported version profile: {reason}")]
    UnsupportedProfile { reason: String },
    /// 表示（UI の「詳細」に出る `detail`）には終了コードとログの末尾を含める。
    #[error("loader install processor {processor} failed {}{}", exit_text(*.code), log_text(.log_tail))]
    ProcessorFailed {
        processor: String,
        code: Option<i32>,
        log_tail: String,
    },
    #[error("loader files are missing: {path}")]
    LoaderFilesMissing { path: String },
    #[error("no Java runtime {component} is available for {platform}")]
    NoCompatibleRuntime { component: String, platform: String },
    #[error("installed Java did not match: expected major {expected_major}, found {found}")]
    JavaVerifyFailed { expected_major: u32, found: String },
    #[error("the Java executable set for the instance was not found: {path}")]
    JavaPathMissing { path: String },
    #[error("instance {instance_id} is already running")]
    AlreadyRunning { instance_id: String },
    #[error("command line is too long ({length} characters)")]
    CommandTooLong { length: usize },
    #[error("failed to start the game: {message}")]
    SpawnFailed { message: String },
    #[error("no account is available to launch the game")]
    AuthNoAccount,
    #[error("sign-in is required for account {account_id}")]
    LoginRequired { account_id: String },
    #[error("sign-in failed because the network is unavailable")]
    AuthOffline { offline_available: bool },
    #[error("sign-in timed out")]
    AuthTimeout,
    #[error("the sign-in callback was invalid")]
    AuthCallbackInvalid,
    #[error("sign-in was denied")]
    AuthDenied,
    #[error("Xbox returned error {xerr}")]
    AuthXbox { xerr: u64 },
    #[error("the app is not approved for the Minecraft API")]
    AppNotApproved,
    #[error("the account does not own Minecraft: Java Edition")]
    NotOwned,
    #[error("Minecraft ownership could not be determined")]
    OwnershipUnknown,
    #[error("the account has no Minecraft profile")]
    NoProfile,
    #[error("no Microsoft client ID is embedded in this build")]
    NoClientId,
    #[error("account {account_id} was not found")]
    AccountNotFound { account_id: String },
    #[error("account {account_id} is in use by a running game")]
    AccountInUse { account_id: String },
    #[error("a path contains characters that cannot be passed to the game")]
    PathNotEncodable,
    #[error("instance {instance_id} was not found")]
    InstanceNotFound { instance_id: String },
    #[error("instance {instance_id} is busy")]
    InstanceBusy { instance_id: String },
    #[error("invalid instance setting: {field}")]
    InvalidInstanceSetting { field: String },
    #[error("a content file already exists: {path}")]
    ContentExists { path: String },
    /// 必須の依存を解決できなかった。何も導入、更新しない。
    /// `name` は分かれば project の表示名（UI の `name` には、無ければ project_id を渡す）。
    #[error("dependency {project_id} could not be resolved: {cause}")]
    ContentDependency {
        project_id: String,
        cause: DependencyCause,
        name: Option<String>,
    },
    /// 新しい版が非互換と宣言している MOD が導入済み。導入は確認なしでは、更新はいつも入れない。
    #[error("installed mods are declared incompatible: {names}")]
    ContentIncompatible { names: String },
    /// 同じ project の MOD が有効で導入済み。2 つ目は入れない。
    #[error("the mod is already installed: {name}")]
    ContentAlreadyInstalled { name: String },
    /// 指定の中身の行が無い（一覧が古いなど）。
    #[error("the content was not found: {path}")]
    ContentNotFound { path: String },
    /// 指定のスクリーンショットが無い（一覧が古い、または名前がフォルダ直下の画像ではない）。
    #[error("the screenshot was not found")]
    ScreenshotNotFound,
    /// ローダーの無い（バニラの）インスタンスには MOD を入れられない。
    #[error("the instance has no mod loader")]
    ContentNoLoader,
    /// 版を切り替えられない。Modrinth で特定されていない行か、選んだ版がその project の
    /// このインスタンスに合う版ではない。
    #[error("the version of {name} cannot be changed to the chosen one")]
    ContentVersionUnavailable { name: String },
    #[error("invalid skin image: {reason}")]
    InvalidSkin { reason: String },
    /// 保存するスキンの名前が空。
    #[error("the skin name is empty")]
    SkinNameEmpty,
    /// 指定した保存済みスキンが無い。
    #[error("the saved skin was not found")]
    SkinNotFound,
    /// options.txt が無い（ゲームを一度も起動していない）ので、共通設定を作れない。
    #[error("the instance has no options.txt yet")]
    SyncNoOptions,
    /// 共通設定が見つからない。
    #[error("the sync profile was not found")]
    SyncProfileNotFound,
    /// インスタンスが共通設定に参加していない。
    #[error("the instance is not in a sync profile")]
    SyncNotMember,
    /// 解決する提案が無い。
    #[error("there is no such sync proposal")]
    SyncNoProposal,
    /// 競合している項目で、採用する値が選ばれていない、または提案にない値だった。
    #[error("a proposed value must be chosen for the conflicting field")]
    SyncNeedsChosenValue,
    /// 共通設定の名前が空、または長すぎる。
    #[error("the sync profile name is invalid")]
    SyncInvalidName,
    #[error("unexpected error: {message}")]
    Unexpected { message: String },
}

/// 依存を解決できなかった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyCause {
    /// 依存が循環している。
    Cycle,
    /// 版にファイルが無い。
    NoFile,
    /// 条件に合う版が見つからない。
    NoVersion,
}

impl DependencyCause {
    /// UI に渡す引数の値。
    pub fn as_str(self) -> &'static str {
        match self {
            DependencyCause::Cycle => "cycle",
            DependencyCause::NoFile => "no_file",
            DependencyCause::NoVersion => "no_version",
        }
    }
}

impl std::fmt::Display for DependencyCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            DependencyCause::Cycle => "the dependencies form a cycle",
            DependencyCause::NoFile => "the version has no files",
            DependencyCause::NoVersion => "no matching version was found",
        })
    }
}

/// processor の終了の書き方。コードが無いのは、Java を起動できなかったか、コードなしで終わったとき。
fn exit_text(code: Option<i32>) -> String {
    match code {
        Some(code) => format!("with exit code {code}"),
        None => "because it could not start Java or Java ended without an exit code".to_string(),
    }
}

/// ログの末尾を、空でなければ次の行から足す。
fn log_text(tail: &str) -> String {
    if tail.is_empty() {
        String::new()
    } else {
        format!("\n{tail}")
    }
}

impl CoreError {
    /// UI の翻訳キー（errors.<code>）に使う、変わらないコード。
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::DataDirUnavailable => "app.data_dir_unavailable",
            CoreError::Io { .. } => "io.failed",
            CoreError::Db(_) => "db.failed",
            CoreError::Migration(_) => "db.migration_failed",
            CoreError::InvalidSetting { .. } => "settings.invalid_value",
            CoreError::TaskNotFound { .. } => "task.not_found",
            CoreError::Cancelled => "task.cancelled",
            CoreError::Offline { .. } => "net.offline",
            CoreError::HttpStatus { .. } => "net.http_status",
            CoreError::HashMismatch { .. } => "net.hash_mismatch",
            CoreError::InvalidMetadata { .. } => "meta.invalid",
            CoreError::VersionNotFound { .. } => "meta.version_not_found",
            CoreError::UnsupportedProfile { .. } => "install.unsupported_profile",
            CoreError::ProcessorFailed { .. } => "install.processor_failed",
            CoreError::LoaderFilesMissing { .. } => "install.loader_files_missing",
            CoreError::NoCompatibleRuntime { .. } => "java.no_compatible_runtime",
            CoreError::JavaVerifyFailed { .. } => "java.verify_failed",
            CoreError::JavaPathMissing { .. } => "java.path_missing",
            CoreError::AlreadyRunning { .. } => "launch.already_running",
            CoreError::CommandTooLong { .. } => "launch.command_too_long",
            CoreError::SpawnFailed { .. } => "launch.spawn_failed",
            CoreError::AuthNoAccount => "auth.no_account",
            CoreError::LoginRequired { .. } => "auth.login_required",
            CoreError::AuthOffline { .. } => "auth.offline",
            CoreError::AuthTimeout => "auth.timeout",
            CoreError::AuthCallbackInvalid => "auth.callback_invalid",
            CoreError::AuthDenied => "auth.denied",
            CoreError::AuthXbox { .. } => "auth.xbox",
            CoreError::AppNotApproved => "auth.app_not_approved",
            CoreError::NotOwned => "auth.not_owned",
            CoreError::OwnershipUnknown => "auth.ownership_unknown",
            CoreError::NoProfile => "auth.no_profile",
            CoreError::NoClientId => "auth.no_client_id",
            CoreError::AccountNotFound { .. } => "auth.account_not_found",
            CoreError::AccountInUse { .. } => "auth.account_in_use",
            CoreError::PathNotEncodable => "launch.path_not_encodable",
            CoreError::InstanceNotFound { .. } => "instance.not_found",
            CoreError::InstanceBusy { .. } => "instance.busy",
            CoreError::InvalidInstanceSetting { .. } => "instance.invalid_setting",
            CoreError::ContentExists { .. } => "content.exists",
            CoreError::ContentDependency { .. } => "content.dependency",
            CoreError::ContentIncompatible { .. } => "content.incompatible",
            CoreError::ContentAlreadyInstalled { .. } => "content.already_installed",
            CoreError::ContentNotFound { .. } => "content.not_found",
            CoreError::ContentNoLoader => "content.no_loader",
            CoreError::ContentVersionUnavailable { .. } => "content.version_unavailable",
            CoreError::InvalidSkin { .. } => "skin.invalid",
            CoreError::SkinNameEmpty => "skin.name_empty",
            CoreError::SkinNotFound => "skin.not_found",
            CoreError::ScreenshotNotFound => "screenshot.not_found",
            CoreError::SyncNoOptions => "sync.no_options",
            CoreError::SyncProfileNotFound => "sync.profile_not_found",
            CoreError::SyncNotMember => "sync.not_member",
            CoreError::SyncNoProposal => "sync.no_proposal",
            CoreError::SyncNeedsChosenValue => "sync.needs_chosen_value",
            CoreError::SyncInvalidName => "sync.invalid_name",
            CoreError::Unexpected { .. } => "app.unexpected",
        }
    }

    fn params(&self) -> BTreeMap<String, ParamValue> {
        let mut params = BTreeMap::new();
        match self {
            CoreError::Io { path, .. } => {
                params.insert("path".into(), ParamValue::Str(path.clone()));
            }
            CoreError::InvalidSetting { key } => {
                params.insert("key".into(), ParamValue::Str(key.clone()));
            }
            CoreError::TaskNotFound { task_id } => {
                params.insert("taskId".into(), ParamValue::Str(task_id.clone()));
            }
            CoreError::HttpStatus { status, .. } => {
                params.insert("status".into(), ParamValue::Num(f64::from(*status)));
            }
            CoreError::HashMismatch { path } => {
                params.insert("path".into(), ParamValue::Str(path.clone()));
            }
            CoreError::VersionNotFound { id } => {
                params.insert("id".into(), ParamValue::Str(id.clone()));
            }
            CoreError::UnsupportedProfile { reason } => {
                params.insert("reason".into(), ParamValue::Str(reason.clone()));
            }
            CoreError::ProcessorFailed {
                processor,
                code,
                log_tail,
            } => {
                params.insert("processor".into(), ParamValue::Str(processor.clone()));
                if let Some(code) = code {
                    params.insert("code".into(), ParamValue::Num(f64::from(*code)));
                }
                params.insert("logTail".into(), ParamValue::Str(log_tail.clone()));
            }
            CoreError::LoaderFilesMissing { path } => {
                params.insert("path".into(), ParamValue::Str(path.clone()));
            }
            CoreError::NoCompatibleRuntime {
                component,
                platform,
            } => {
                params.insert("component".into(), ParamValue::Str(component.clone()));
                params.insert("platform".into(), ParamValue::Str(platform.clone()));
            }
            CoreError::JavaVerifyFailed {
                expected_major,
                found,
            } => {
                params.insert(
                    "expectedMajor".into(),
                    ParamValue::Num(f64::from(*expected_major)),
                );
                params.insert("found".into(), ParamValue::Str(found.clone()));
            }
            CoreError::JavaPathMissing { path } => {
                params.insert("path".into(), ParamValue::Str(path.clone()));
            }
            CoreError::InvalidInstanceSetting { field } => {
                params.insert("field".into(), ParamValue::Str(field.clone()));
            }
            CoreError::ContentExists { path } => {
                params.insert("path".into(), ParamValue::Str(path.clone()));
            }
            CoreError::ContentDependency {
                project_id,
                cause,
                name,
            } => {
                params.insert("projectId".into(), ParamValue::Str(project_id.clone()));
                params.insert("cause".into(), ParamValue::Str(cause.as_str().into()));
                let shown = name.clone().unwrap_or_else(|| project_id.clone());
                params.insert("name".into(), ParamValue::Str(shown));
            }
            CoreError::ContentAlreadyInstalled { name }
            | CoreError::ContentVersionUnavailable { name } => {
                params.insert("name".into(), ParamValue::Str(name.clone()));
            }
            CoreError::ContentNotFound { path } => {
                params.insert("path".into(), ParamValue::Str(path.clone()));
            }
            CoreError::ContentIncompatible { names } => {
                params.insert("names".into(), ParamValue::Str(names.clone()));
            }
            CoreError::InvalidSkin { reason } => {
                params.insert("reason".into(), ParamValue::Str(reason.clone()));
            }
            CoreError::CommandTooLong { length } => {
                params.insert("length".into(), ParamValue::Num(*length as f64));
            }
            CoreError::LoginRequired { account_id } => {
                params.insert("accountId".into(), ParamValue::Str(account_id.clone()));
            }
            CoreError::AuthOffline { offline_available } => {
                params.insert(
                    "offlineAvailable".into(),
                    ParamValue::Bool(*offline_available),
                );
            }
            CoreError::AuthXbox { xerr } => {
                params.insert("xerr".into(), ParamValue::Num(*xerr as f64));
            }
            _ => {}
        }
        params
    }

    pub fn to_payload(&self) -> ErrorPayload {
        ErrorPayload {
            code: self.code().to_string(),
            params: self.params(),
            detail: Some(self.to_string()),
        }
    }
}

/// エラーの引数の値。文字列、数値、真偽値のいずれか。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum ParamValue {
    Str(String),
    Num(f64),
    Bool(bool),
}

/// UI に渡すエラー。文言は UI 側で `errors.<code>` から組み立てる。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ErrorPayload {
    pub code: String,
    pub params: BTreeMap<String, ParamValue>,
    pub detail: Option<String>,
}

impl ErrorPayload {
    pub fn new(code: &str) -> Self {
        Self {
            code: code.to_string(),
            params: BTreeMap::new(),
            detail: None,
        }
    }
}

impl From<CoreError> for ErrorPayload {
    fn from(err: CoreError) -> Self {
        err.to_payload()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable() {
        assert_eq!(
            CoreError::DataDirUnavailable.code(),
            "app.data_dir_unavailable"
        );
        assert_eq!(
            CoreError::InvalidSetting {
                key: "theme".into()
            }
            .code(),
            "settings.invalid_value"
        );
        assert_eq!(CoreError::Cancelled.code(), "task.cancelled");
        assert_eq!(
            CoreError::ProcessorFailed {
                processor: "p".into(),
                code: Some(1),
                log_tail: "t".into(),
            }
            .code(),
            "install.processor_failed"
        );
        assert_eq!(
            CoreError::LoaderFilesMissing { path: "p".into() }.code(),
            "install.loader_files_missing"
        );
    }

    #[test]
    fn phase5_processor_params() {
        let p = CoreError::ProcessorFailed {
            processor: "net.minecraftforge:binarypatcher:1.0".into(),
            code: Some(3),
            log_tail: "boom".into(),
        }
        .to_payload();
        assert_eq!(
            p.params.get("processor"),
            Some(&ParamValue::Str(
                "net.minecraftforge:binarypatcher:1.0".into()
            ))
        );
        assert_eq!(p.params.get("code"), Some(&ParamValue::Num(3.0)));
        assert_eq!(
            p.params.get("logTail"),
            Some(&ParamValue::Str("boom".into()))
        );
        let p = CoreError::ProcessorFailed {
            processor: "x".into(),
            code: None,
            log_tail: String::new(),
        }
        .to_payload();
        assert!(!p.params.contains_key("code"));
        let p = CoreError::LoaderFilesMissing {
            path: "a/b.jar".into(),
        }
        .to_payload();
        assert_eq!(
            p.params.get("path"),
            Some(&ParamValue::Str("a/b.jar".into()))
        );
    }

    #[test]
    fn processor_failure_detail_has_the_exit_code_and_log_tail() {
        let detail = CoreError::ProcessorFailed {
            processor: "net.minecraftforge:binarypatcher:1.0".into(),
            code: Some(3),
            log_tail: "Exception in thread main\nboom".into(),
        }
        .to_payload()
        .detail
        .unwrap();
        assert!(
            detail.starts_with("loader install processor net.minecraftforge:binarypatcher:1.0"),
            "{detail}"
        );
        assert!(detail.contains("exit code 3"), "{detail}");
        assert!(
            detail.ends_with("Exception in thread main\nboom"),
            "{detail}"
        );
        assert!(!detail.contains("Some("), "{detail}");
        assert!(!detail.contains("Forge"), "{detail}");

        let detail = CoreError::ProcessorFailed {
            processor: "x".into(),
            code: None,
            log_tail: "failed to start java: not found".into(),
        }
        .to_string();
        assert!(detail.contains("could not start Java"), "{detail}");
        assert!(!detail.contains("None"), "{detail}");
        assert!(
            detail.contains("failed to start java: not found"),
            "{detail}"
        );

        // ログが空なら末尾に何も足さない
        let detail = CoreError::ProcessorFailed {
            processor: "x".into(),
            code: Some(1),
            log_tail: String::new(),
        }
        .to_string();
        assert_eq!(detail, "loader install processor x failed with exit code 1");
    }

    #[test]
    fn payload_shape_is_stable() {
        let payload = CoreError::HttpStatus {
            status: 503,
            url: "https://example.invalid/x".into(),
        }
        .to_payload();
        insta::assert_json_snapshot!(payload, @r#"
        {
          "code": "net.http_status",
          "params": {
            "status": 503.0
          },
          "detail": "HTTP 503 from https://example.invalid/x"
        }
        "#);
    }

    #[test]
    fn payload_new_has_no_params_or_detail() {
        let payload = ErrorPayload::new("app.not_ready");
        assert_eq!(payload.code, "app.not_ready");
        assert!(payload.params.is_empty());
        assert!(payload.detail.is_none());
    }

    #[test]
    fn phase2_codes_are_stable() {
        let cases: Vec<(CoreError, &str)> = vec![
            (
                CoreError::Offline {
                    message: "x".into(),
                },
                "net.offline",
            ),
            (
                CoreError::HttpStatus {
                    status: 404,
                    url: "u".into(),
                },
                "net.http_status",
            ),
            (
                CoreError::HashMismatch { path: "p".into() },
                "net.hash_mismatch",
            ),
            (
                CoreError::InvalidMetadata {
                    message: "m".into(),
                },
                "meta.invalid",
            ),
            (
                CoreError::VersionNotFound { id: "1.0".into() },
                "meta.version_not_found",
            ),
            (
                CoreError::UnsupportedProfile { reason: "r".into() },
                "install.unsupported_profile",
            ),
            (
                CoreError::NoCompatibleRuntime {
                    component: "c".into(),
                    platform: "p".into(),
                },
                "java.no_compatible_runtime",
            ),
            (
                CoreError::JavaVerifyFailed {
                    expected_major: 8,
                    found: "21 x86_64".into(),
                },
                "java.verify_failed",
            ),
            (
                CoreError::AlreadyRunning {
                    instance_id: "i".into(),
                },
                "launch.already_running",
            ),
            (
                CoreError::CommandTooLong { length: 40_000 },
                "launch.command_too_long",
            ),
            (
                CoreError::SpawnFailed {
                    message: "m".into(),
                },
                "launch.spawn_failed",
            ),
            (
                CoreError::InstanceNotFound {
                    instance_id: "i".into(),
                },
                "instance.not_found",
            ),
            (
                CoreError::InstanceBusy {
                    instance_id: "i".into(),
                },
                "instance.busy",
            ),
            (
                CoreError::InvalidInstanceSetting { field: "f".into() },
                "instance.invalid_setting",
            ),
        ];
        for (err, code) in cases {
            assert_eq!(err.code(), code);
        }
    }

    #[test]
    fn phase2_params_shape() {
        let payload = CoreError::HttpStatus {
            status: 503,
            url: "https://x".into(),
        }
        .to_payload();
        assert_eq!(payload.params.get("status"), Some(&ParamValue::Num(503.0)));
        let payload = CoreError::JavaVerifyFailed {
            expected_major: 8,
            found: "17 x86_64".into(),
        }
        .to_payload();
        assert_eq!(
            payload.params.get("expectedMajor"),
            Some(&ParamValue::Num(8.0))
        );
        assert_eq!(
            payload.params.get("found"),
            Some(&ParamValue::Str("17 x86_64".into()))
        );
        let payload = CoreError::NoCompatibleRuntime {
            component: "jre-legacy".into(),
            platform: "mac-os-arm64".into(),
        }
        .to_payload();
        assert_eq!(
            payload.params.get("component"),
            Some(&ParamValue::Str("jre-legacy".into()))
        );
        assert_eq!(
            payload.params.get("platform"),
            Some(&ParamValue::Str("mac-os-arm64".into()))
        );
    }

    #[test]
    fn phase3_codes_and_params() {
        let cases: Vec<(CoreError, &str)> = vec![
            (CoreError::AuthNoAccount, "auth.no_account"),
            (
                CoreError::LoginRequired {
                    account_id: "a".into(),
                },
                "auth.login_required",
            ),
            (
                CoreError::AuthOffline {
                    offline_available: true,
                },
                "auth.offline",
            ),
            (CoreError::AuthTimeout, "auth.timeout"),
            (CoreError::AuthCallbackInvalid, "auth.callback_invalid"),
            (CoreError::AuthDenied, "auth.denied"),
            (CoreError::AuthXbox { xerr: 1 }, "auth.xbox"),
            (CoreError::AppNotApproved, "auth.app_not_approved"),
            (CoreError::NotOwned, "auth.not_owned"),
            (CoreError::OwnershipUnknown, "auth.ownership_unknown"),
            (CoreError::NoProfile, "auth.no_profile"),
            (CoreError::NoClientId, "auth.no_client_id"),
            (
                CoreError::AccountNotFound {
                    account_id: "a".into(),
                },
                "auth.account_not_found",
            ),
            (
                CoreError::AccountInUse {
                    account_id: "a".into(),
                },
                "auth.account_in_use",
            ),
            (CoreError::PathNotEncodable, "launch.path_not_encodable"),
        ];
        for (err, code) in cases {
            assert_eq!(err.code(), code);
        }
        let p = CoreError::LoginRequired {
            account_id: "abc".into(),
        }
        .to_payload();
        assert_eq!(
            p.params.get("accountId"),
            Some(&ParamValue::Str("abc".into()))
        );
        let p = CoreError::AuthOffline {
            offline_available: true,
        }
        .to_payload();
        assert_eq!(
            p.params.get("offlineAvailable"),
            Some(&ParamValue::Bool(true))
        );
        let p = CoreError::AuthXbox { xerr: 2148916233 }.to_payload();
        assert_eq!(p.params.get("xerr"), Some(&ParamValue::Num(2148916233.0)));
        assert!(CoreError::AuthTimeout.to_payload().params.is_empty());
        assert!(
            CoreError::AccountNotFound {
                account_id: "a".into()
            }
            .to_payload()
            .params
            .is_empty()
        );
    }

    #[test]
    fn skin_library_codes_are_stable() {
        assert_eq!(CoreError::SkinNameEmpty.code(), "skin.name_empty");
        assert_eq!(CoreError::SkinNotFound.code(), "skin.not_found");
        assert_eq!(CoreError::ScreenshotNotFound.code(), "screenshot.not_found");
    }

    #[test]
    fn phase4_invalid_skin() {
        let err = CoreError::InvalidSkin {
            reason: "dimensions".into(),
        };
        assert_eq!(err.code(), "skin.invalid");
        assert_eq!(
            err.to_payload().params.get("reason"),
            Some(&ParamValue::Str("dimensions".into()))
        );
    }

    #[test]
    fn phase5_content_exists() {
        let err = CoreError::ContentExists {
            path: "mods/a.jar.disabled".into(),
        };
        assert_eq!(err.code(), "content.exists");
        assert_eq!(
            err.to_payload().params.get("path"),
            Some(&ParamValue::Str("mods/a.jar.disabled".into()))
        );
    }

    #[test]
    fn phase5_content_dependency() {
        let err = CoreError::ContentDependency {
            project_id: "P7dR8mSH".into(),
            cause: DependencyCause::Cycle,
            name: None,
        };
        assert_eq!(err.code(), "content.dependency");
        let p = err.to_payload();
        assert_eq!(
            p.params.get("projectId"),
            Some(&ParamValue::Str("P7dR8mSH".into()))
        );
        assert_eq!(
            p.params.get("cause"),
            Some(&ParamValue::Str("cycle".into()))
        );
        assert_eq!(
            p.detail.as_deref(),
            Some("dependency P7dR8mSH could not be resolved: the dependencies form a cycle")
        );
        // 表示名が分からなければ、name には project_id を入れる
        assert_eq!(
            p.params.get("name"),
            Some(&ParamValue::Str("P7dR8mSH".into()))
        );
        let p = CoreError::ContentDependency {
            project_id: "P7dR8mSH".into(),
            cause: DependencyCause::NoVersion,
            name: Some("Fabric API".into()),
        }
        .to_payload();
        assert_eq!(
            p.params.get("name"),
            Some(&ParamValue::Str("Fabric API".into()))
        );
    }

    #[test]
    fn phase5_content_install_refusals() {
        let err = CoreError::ContentAlreadyInstalled {
            name: "Sodium".into(),
        };
        assert_eq!(err.code(), "content.already_installed");
        assert_eq!(
            err.to_payload().params.get("name"),
            Some(&ParamValue::Str("Sodium".into()))
        );
        let err = CoreError::ContentNotFound {
            path: "mods/a.jar".into(),
        };
        assert_eq!(err.code(), "content.not_found");
        assert_eq!(
            err.to_payload().params.get("path"),
            Some(&ParamValue::Str("mods/a.jar".into()))
        );
        assert_eq!(CoreError::ContentNoLoader.code(), "content.no_loader");
        assert_eq!(CoreError::SyncNoOptions.code(), "sync.no_options");
        assert_eq!(
            CoreError::SyncProfileNotFound.code(),
            "sync.profile_not_found"
        );
        assert_eq!(CoreError::SyncNotMember.code(), "sync.not_member");
        assert_eq!(CoreError::SyncNoProposal.code(), "sync.no_proposal");
        assert_eq!(
            CoreError::SyncNeedsChosenValue.code(),
            "sync.needs_chosen_value"
        );
        assert_eq!(CoreError::SyncInvalidName.code(), "sync.invalid_name");
        let err = CoreError::ContentVersionUnavailable {
            name: "Sodium".into(),
        };
        assert_eq!(err.code(), "content.version_unavailable");
        assert_eq!(
            err.to_payload().params.get("name"),
            Some(&ParamValue::Str("Sodium".into()))
        );
    }

    #[test]
    fn phase4_java_path_missing() {
        let err = CoreError::JavaPathMissing {
            path: "C:/jdk/bin/javaw.exe".into(),
        };
        assert_eq!(err.code(), "java.path_missing");
        assert_eq!(
            err.to_payload().params.get("path"),
            Some(&ParamValue::Str("C:/jdk/bin/javaw.exe".into()))
        );
    }
}
