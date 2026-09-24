use tauri::{AppHandle, Emitter, Manager};

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
pub async fn settings_open(app: AppHandle) -> AppResult<()> {
    off_thread(move || {
        let win = app
            .get_webview_window("settings")
            .ok_or("error.io: 设置窗不存在")?;
        if !win.is_visible().unwrap_or(false) {
            let layout = infra::persist::get("settings")
                .map(AppSettings::from_value)
                .unwrap_or_default()
                .island;
            let _ = infra::layout::apply_settings_layout(&app, &layout);
            let _ = app.emit_to("settings", "settings:opened", ());
        }
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
}
