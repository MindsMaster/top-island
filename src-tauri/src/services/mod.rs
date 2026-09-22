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

/// 设置生效的唯一入口：启动和 settings_update 都走这里，
/// 新增受设置驱动的域只在此登记一次。各 sync 幂等，可重复调用。
pub fn apply_settings(app: &AppHandle, settings: &AppSettings) -> AppResult<()> {
    infra::autolaunch::sync(settings.auto_launch)?;
    infra::layout::apply_island_layout(app, &settings.island)?;
    music::sync(app, settings);
    notify::sync(app, settings);
    wechat::sync(app, settings);
    Ok(())
}
