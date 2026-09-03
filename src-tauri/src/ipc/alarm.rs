use crate::error::AppResult;
use crate::services;
use crate::services::alarm::AlarmSound;

use super::off_thread;

/// 列出系统默认闹钟音（%windir%\Media\Alarm*.wav）
#[tauri::command]
pub async fn alarm_sound_list() -> AppResult<Vec<AlarmSound>> {
    off_thread(services::alarm::list_default_sounds).await
}

/// 读取音频文件为 data URL；不可读/格式不支持/超限返回 None（Electron 契约）
#[tauri::command]
pub async fn alarm_sound_data(path: String) -> AppResult<Option<String>> {
    off_thread(move || services::alarm::sound_data_url(&path)).await
}

/// 打开文件对话框选择自定义音频；取消返回 None
#[tauri::command]
pub async fn alarm_sound_pick() -> AppResult<Option<AlarmSound>> {
    off_thread(services::alarm::pick_sound).await
}
