use tauri::AppHandle;

use crate::error::AppResult;
use crate::services::update::{self, UpdateStatus};

#[tauri::command]
pub async fn update_status() -> AppResult<UpdateStatus> {
    Ok(update::status())
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> AppResult<UpdateStatus> {
    Ok(update::check(app, true).await)
}

#[tauri::command]
pub async fn update_install() -> AppResult<()> {
    update::install()
}
