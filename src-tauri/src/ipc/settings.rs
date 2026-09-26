use tauri::{AppHandle, Emitter};

use island_core::AppSettings;

use crate::error::{AppError, AppResult};
use crate::{infra, services};

use super::off_thread;

#[tauri::command]
pub async fn settings_update(app: AppHandle, settings: AppSettings) -> AppResult<()> {
    off_thread(move || {
        let value = serde_json::to_value(&settings)
            .map_err(|e| AppError::new(format!("error.io: 序列化设置: {e}")))?;
        infra::persist::set("settings", value)?;
        services::apply_settings(&app, &settings)?;
        let _ = app.emit("settings:changed", &settings);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn settings_open(app: AppHandle, section: Option<String>) -> AppResult<()> {
    off_thread(move || infra::layout::open_settings(&app, section.as_deref())).await
}
