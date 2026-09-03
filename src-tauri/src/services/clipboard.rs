use crate::error::{AppError, AppResult};

/// Electron 契约：读不到文本返回空串而非报错
pub fn read_text() -> AppResult<String> {
    Ok(island_windows::clipboard::read_text()?.unwrap_or_default())
}

pub fn write_text(text: &str) -> AppResult<()> {
    island_windows::clipboard::write_text(text).map_err(AppError::from)
}

pub fn has_image() -> bool {
    island_windows::clipboard::has_image()
}

pub fn read_file_paths() -> AppResult<Vec<String>> {
    island_windows::clipboard::read_file_paths().map_err(AppError::from)
}

/// 剪贴板序列号：前端维护历史时可用它判断两次「变化事件」之间内容是否真的变过
pub fn sequence_number() -> u32 {
    island_windows::clipboard::sequence_number()
}
