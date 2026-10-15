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
    #[error("demo task failed at {percent}%")]
    DemoFailed { percent: u8 },
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
            CoreError::DemoFailed { .. } => "demo.failed",
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
            CoreError::DemoFailed { percent } => {
                params.insert("percent".into(), ParamValue::Num(f64::from(*percent)));
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
        assert_eq!(CoreError::DemoFailed { percent: 40 }.code(), "demo.failed");
    }

    #[test]
    fn payload_shape_is_stable() {
        let payload = CoreError::DemoFailed { percent: 40 }.to_payload();
        insta::assert_json_snapshot!(payload, @r#"
        {
          "code": "demo.failed",
          "params": {
            "percent": 40.0
          },
          "detail": "demo task failed at 40%"
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
}
