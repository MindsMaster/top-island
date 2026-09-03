use serde::{Serialize, Serializer};

/// 应用层统一错误：消息里嵌 i18n 键（error.xxx: 细节），前端 formatError 提取翻译。
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct AppError(String);

impl AppError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }

    pub fn io_at(context: &str, e: &std::io::Error) -> Self {
        use std::io::ErrorKind;
        let key = match e.kind() {
            ErrorKind::PermissionDenied => "error.access_denied",
            ErrorKind::NotFound => "error.not_found",
            _ => "error.io",
        };
        Self(format!("{key}: {context}: {e}"))
    }
}

impl From<String> for AppError {
    fn from(msg: String) -> Self {
        Self(msg)
    }
}

impl From<&str> for AppError {
    fn from(msg: &str) -> Self {
        Self(msg.to_string())
    }
}

impl From<island_windows::WinError> for AppError {
    fn from(e: island_windows::WinError) -> Self {
        Self(format!("error.io: {e}"))
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

pub type AppResult<T> = Result<T, AppError>;
