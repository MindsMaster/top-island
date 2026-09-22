use crate::error::AppResult;
use crate::services;
use crate::services::alarm::AlarmSound;

use super::off_thread;

#[tauri::command]
pub async fn alarm_sound_list() -> AppResult<Vec<AlarmSound>> {
    off_thread(services::alarm::list_default_sounds).await
}

#[tauri::command]
pub async fn alarm_sound_data(path: String) -> AppResult<Option<String>> {
    off_thread(move || services::alarm::sound_data_url(&path)).await
}

/// 取消返回 None
#[tauri::command]
pub async fn alarm_sound_pick() -> AppResult<Option<AlarmSound>> {
    off_thread(services::alarm::pick_sound).await
}
