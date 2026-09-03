use tauri::AppHandle;

use crate::error::AppResult;
use crate::services;

use super::off_thread;

/// 获取并缓存微信解密密钥（扫内存，需 Weixin.exe 在运行）。返回结果与账号。
/// error 字段是前端已知的稳定码：no-account / recover-failed。
#[tauri::command]
pub async fn wechat_acquire_key(app: AppHandle) -> AppResult<services::wechat::AcquireKeyResult> {
    off_thread(move || services::wechat::acquire_key(&app)).await
}

/// 是否已有可用的微信密钥缓存
#[tauri::command]
pub async fn wechat_has_key() -> AppResult<bool> {
    off_thread(|| Ok(services::wechat::has_key())).await
}
