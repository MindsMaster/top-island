use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use island_core::{AppSettings, IpCityInfo};

use crate::error::{AppError, AppResult};
use crate::{infra, services};

pub mod alarm;
pub mod clipboard;
pub mod music;
pub mod notify;
pub mod wechat;

/// 耗时命令统一走阻塞线程池，不堵 Tauri UI 线程
async fn off_thread<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(AppError::from(format!("error.io: {e}"))))
}

// ---- store ----

#[tauri::command]
pub async fn store_get(key: String) -> AppResult<serde_json::Value> {
    off_thread(move || Ok(infra::persist::get(&key).unwrap_or(serde_json::Value::Null))).await
}

#[tauri::command]
pub async fn store_set(key: String, value: serde_json::Value) -> AppResult<()> {
    off_thread(move || infra::persist::set(&key, value)).await
}

/// 清空全部本地数据并重启（设置里的「重置」，不可撤销）
#[tauri::command]
pub async fn store_clear(app: AppHandle) -> AppResult<()> {
    infra::persist::clear()?;
    app.restart();
}

// ---- settings ----

/// 持久化设置并广播给其他窗口，附带同步自启与各域生命周期
#[tauri::command]
pub async fn settings_update(app: AppHandle, settings: AppSettings) -> AppResult<()> {
    let value = serde_json::to_value(&settings)
        .map_err(|e| AppError::new(format!("error.io: 序列化设置: {e}")))?;
    infra::persist::set("settings", value)?;
    infra::autolaunch::sync(settings.auto_launch)?;
    services::music::sync(&app, &settings);
    services::notify::sync(&app, &settings);
    services::wechat::sync(&app, &settings);
    let _ = app.emit("settings:changed", &settings);
    Ok(())
}

// ---- weather ----

#[tauri::command]
pub async fn weather_ip_city() -> AppResult<IpCityInfo> {
    off_thread(services::weather::ip_city).await
}

#[tauri::command]
pub async fn weather_geocode(city: String, lang: String) -> AppResult<serde_json::Value> {
    off_thread(move || services::weather::geocode(&city, &lang)).await
}

#[tauri::command]
pub async fn weather_query(
    lat: f64,
    lon: f64,
    daily: Option<String>,
    forecast_days: Option<u32>,
) -> AppResult<serde_json::Value> {
    off_thread(move || services::weather::query(lat, lon, daily.as_deref(), forecast_days)).await
}

// ---- misc ----

#[tauri::command]
pub fn app_get_locale() -> String {
    locale()
}

fn locale() -> String {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    const LOCALE_NAME_MAX_LENGTH: usize = 85;
    let mut buf = vec![0u16; LOCALE_NAME_MAX_LENGTH];
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n <= 0 {
        return "en-US".into();
    }
    String::from_utf16_lossy(&buf[..(n - 1) as usize])
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
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    AppVersionInfo {
        version: app.package_info().version.to_string(),
        git_hash: option_env!("TI_GIT_HASH").unwrap_or("").into(),
        packaged: !exe.contains("\\target\\"),
    }
}

#[tauri::command]
pub fn displays_list(app: AppHandle) -> AppResult<Vec<infra::layout::DisplayInfo>> {
    infra::layout::list_displays(&app)
}

/// 协议白名单：渲染层只能开 http(s)/mailto，挡住 file:// 之类的本地协议
#[tauri::command]
pub async fn shell_open_external(url: String) -> AppResult<()> {
    let allowed = url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("mailto:");
    if !allowed {
        return Err(AppError::new("error.denied: 不允许打开的协议"));
    }
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|e| AppError::new(format!("error.io: {e}")))
}

#[tauri::command]
pub fn settings_open(app: AppHandle) -> AppResult<()> {
    let win = app.get_webview_window("settings").ok_or("error.io: 设置窗不存在")?;
    win.show().map_err(|e| e.to_string())?;
    win.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

// ---- window ----

/// 退出整个应用
#[tauri::command]
pub fn window_close(app: AppHandle) {
    app.exit(0);
}

/// 关闭调用方所在窗口：设置窗只隐藏（conf 声明的窗口关掉就没了，重开靠 show）
#[tauri::command]
pub fn window_close_self(window: tauri::WebviewWindow) {
    if window.label() == "settings" {
        let _ = window.hide();
    } else {
        let _ = window.close();
    }
}

/// 全局光标位置（相对调用方窗口内容区）。
/// 悬停看门狗用：mouseleave 不触发时的兜底校验。
#[tauri::command]
pub fn window_get_cursor_point(window: tauri::WebviewWindow) -> AppResult<(i32, i32)> {
    let (x, y) = island_windows::input::cursor_position();
    let origin = window.inner_position().map_err(|e| e.to_string())?;
    Ok((x - origin.x, y - origin.y))
}

// ---- spike 保留：Phase 4 前端切换后随 spike-ui 一起删除 ----

#[tauri::command]
pub async fn smtc_now() -> AppResult<Option<island_windows::NowPlaying>> {
    off_thread(|| island_windows::now_playing().map_err(AppError::from)).await
}

#[tauri::command]
pub fn set_panel_open(open: bool) {
    crate::set_panel_hot(open);
}
