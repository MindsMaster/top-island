use crate::error::AppResult;
use crate::services;

use super::off_thread;

/// 历史记录由前端维护（走 store_get/store_set），后端只提供剪贴板原语

#[tauri::command]
pub async fn clipboard_read_text() -> AppResult<String> {
    off_thread(services::clipboard::read_text).await
}

#[tauri::command]
pub async fn clipboard_write_text(text: String) -> AppResult<()> {
    off_thread(move || services::clipboard::write_text(&text)).await
}

#[tauri::command]
pub async fn clipboard_has_image() -> AppResult<bool> {
    off_thread(|| Ok(services::clipboard::has_image())).await
}

#[tauri::command]
pub async fn clipboard_read_file_paths() -> AppResult<Vec<String>> {
    off_thread(services::clipboard::read_file_paths).await
}

/// 剪贴板序列号：前端维护历史时判断两次变化事件之间内容是否真的变过
#[tauri::command]
pub async fn clipboard_sequence_number() -> AppResult<u32> {
    off_thread(|| Ok(services::clipboard::sequence_number())).await
}
