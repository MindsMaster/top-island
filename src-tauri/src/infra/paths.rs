use std::path::PathBuf;

use crate::error::{AppError, AppResult};

/// 沿用 Electron 版的 userData 目录（%APPDATA%\top-island），老用户的 store.json 直接可读
pub fn data_dir() -> AppResult<PathBuf> {
    let base = std::env::var("APPDATA")
        .map_err(|e| AppError::new(format!("error.io: APPDATA 环境变量: {e}")))?;
    let dir = PathBuf::from(base).join("top-island");
    std::fs::create_dir_all(&dir).map_err(|e| AppError::io_at("创建数据目录", &e))?;
    Ok(dir)
}
