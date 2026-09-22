use tauri::AppHandle;

use crate::error::AppResult;
use crate::services;

use super::off_thread;

/// error 码与前端约定 no-account / recover-failed
#[tauri::command]
pub async fn wechat_acquire_key(app: AppHandle) -> AppResult<services::wechat::AcquireKeyResult> {
    off_thread(move || services::wechat::acquire_key(&app)).await
}

#[tauri::command]
pub async fn wechat_has_key() -> AppResult<bool> {
    off_thread(|| Ok(services::wechat::has_key())).await
}
