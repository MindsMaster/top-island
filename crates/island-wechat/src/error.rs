#[derive(Debug)]
pub enum WeChatError {
    NotRunning,
    NoAccount,
    NoValidKey,
    InvalidKey,
    Io {
        context: &'static str,
        source: std::io::Error,
    },
    Api {
        context: &'static str,
        message: String,
    },
    Sqlite {
        context: &'static str,
        message: String,
    },
}

impl WeChatError {
    pub fn io(context: &'static str, source: std::io::Error) -> Self {
        Self::Io { context, source }
    }

    pub fn api(context: &'static str, e: impl std::fmt::Display) -> Self {
        Self::Api {
            context,
            message: e.to_string(),
        }
    }

    pub fn sqlite(context: &'static str, e: rusqlite::Error) -> Self {
        Self::Sqlite {
            context,
            message: e.to_string(),
        }
    }
}

impl std::fmt::Display for WeChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRunning => write!(f, "WeChat/Weixin 未在运行"),
            Self::NoAccount => write!(f, "未找到微信 4.x 账号目录"),
            Self::NoValidKey => write!(f, "进程内存中未找到有效密钥"),
            Self::InvalidKey => write!(f, "密钥无法解密该库"),
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::Api { context, message } => write!(f, "{context}: {message}"),
            Self::Sqlite { context, message } => write!(f, "{context}: {message}"),
        }
    }
}

impl std::error::Error for WeChatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, WeChatError>;
