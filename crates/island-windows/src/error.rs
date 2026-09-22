#[derive(Debug)]
pub enum WinError {
    Api {
        context: &'static str,
        message: String,
    },
    Io {
        context: &'static str,
        source: std::io::Error,
    },
    Sqlite {
        context: &'static str,
        message: String,
    },
}

impl WinError {
    pub fn api(context: &'static str, e: impl std::fmt::Display) -> Self {
        Self::Api {
            context,
            message: e.to_string(),
        }
    }

    pub fn io(context: &'static str, source: std::io::Error) -> Self {
        Self::Io { context, source }
    }

    pub fn sqlite(context: &'static str, e: rusqlite::Error) -> Self {
        Self::Sqlite {
            context,
            message: e.to_string(),
        }
    }
}

impl std::fmt::Display for WinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api { context, message } => write!(f, "{context}: {message}"),
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::Sqlite { context, message } => write!(f, "{context}: {message}"),
        }
    }
}

impl std::error::Error for WinError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, WinError>;
