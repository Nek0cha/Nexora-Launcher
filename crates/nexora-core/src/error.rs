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
    #[error("no Java runtime {component} is available for {platform}")]
    NoCompatibleRuntime { component: String, platform: String },
    #[error("installed Java did not match: expected major {expected_major}, found {found}")]
    JavaVerifyFailed { expected_major: u32, found: String },
    #[error("instance {instance_id} is already running")]
    AlreadyRunning { instance_id: String },
    #[error("command line is too long ({length} characters)")]
    CommandTooLong { length: usize },
    #[error("failed to start the game: {message}")]
    SpawnFailed { message: String },
    #[error("no account is available to launch the game")]
    NoAccount,
    #[error("instance {instance_id} was not found")]
    InstanceNotFound { instance_id: String },
    #[error("unexpected error: {message}")]
    Unexpected { message: String },
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
            CoreError::NoCompatibleRuntime { .. } => "java.no_compatible_runtime",
            CoreError::JavaVerifyFailed { .. } => "java.verify_failed",
            CoreError::AlreadyRunning { .. } => "launch.already_running",
            CoreError::CommandTooLong { .. } => "launch.command_too_long",
            CoreError::SpawnFailed { .. } => "launch.spawn_failed",
            CoreError::NoAccount => "launch.no_account",
            CoreError::InstanceNotFound { .. } => "instance.not_found",
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
            CoreError::CommandTooLong { length } => {
                params.insert("length".into(), ParamValue::Num(*length as f64));
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
            (CoreError::NoAccount, "launch.no_account"),
            (
                CoreError::InstanceNotFound {
                    instance_id: "i".into(),
                },
                "instance.not_found",
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
}
