use serde::Serialize;
use tauri::AppHandle;

use crate::error::{AppError, AppResult};
use crate::infra;

#[tauri::command]
pub fn app_get_locale() -> String {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    const LOCALE_NAME_MAX_LENGTH: usize = 85;
    let mut buf = vec![0u16; LOCALE_NAME_MAX_LENGTH];
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n <= 0 {
        return "en-US".into();
    }
    String::from_utf16_lossy(&buf[..(n - 1) as usize])
}

#[tauri::command]
pub fn app_quit(app: AppHandle) {
    app.exit(0);
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppVersionInfo {
    pub version: String,
    pub git_hash: String,
    pub packaged: bool,
}

#[tauri::command]
pub fn app_get_version(app: AppHandle) -> AppVersionInfo {
    AppVersionInfo {
        version: app.package_info().version.to_string(),
        git_hash: option_env!("TI_GIT_HASH").unwrap_or("").into(),
        packaged: !tauri::is_dev(),
    }
}

#[tauri::command]
pub fn displays_list(app: AppHandle) -> AppResult<Vec<infra::layout::DisplayInfo>> {
    infra::layout::list_displays(&app)
}

/// 协议白名单
#[tauri::command]
pub async fn shell_open_external(url: String) -> AppResult<()> {
    let allowed =
        url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:");
    if !allowed {
        return Err(AppError::new("error.denied: 不允许打开的协议"));
    }
    tauri_plugin_opener::open_url(&url, None::<&str>)
        .map_err(|e| AppError::new(format!("error.io: {e}")))
}

#[tauri::command]
pub async fn diag_reveal() -> AppResult<()> {
    let dir = infra::paths::data_dir()?;
    tauri_plugin_opener::open_path(&dir, None::<&str>)
        .map_err(|e| AppError::new(format!("error.io: {e}")))
}
