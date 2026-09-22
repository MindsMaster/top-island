use tauri::AppHandle;

use island_core::AppSettings;

use crate::error::AppResult;
use crate::infra;

pub mod alarm;
pub mod clipboard;
pub mod music;
pub mod notify;
pub mod update;
pub mod weather;
pub mod wechat;

/// 新增受设置驱动的域在此登记
pub fn apply_settings(app: &AppHandle, settings: &AppSettings) -> AppResult<()> {
    infra::autolaunch::sync(settings.auto_launch)?;
    infra::layout::apply_island_layout(app, &settings.island)?;
    music::sync(app, settings);
    notify::sync(app, settings);
    wechat::sync(app, settings);
    Ok(())
}
