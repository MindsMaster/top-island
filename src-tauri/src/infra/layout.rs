use tauri::{Manager, PhysicalPosition, PhysicalSize};

use island_core::IslandLayout;

use crate::error::AppResult;

const ISLAND_BASE_W: f64 = 900.0;
// 高度要装下大视图（370）+ 下方通知栈（3 张卡 + 折叠行 ≈ 280）
const ISLAND_BASE_H: f64 = 660.0;
const SETTINGS_BASE_W: f64 = 480.0;
const SETTINGS_BASE_H: f64 = 420.0;
/// 设置窗距所在屏顶部的基准偏移（随缩放等比放大）
const SETTINGS_OFFSET_Y: f64 = 64.0;

fn resolve_monitor(
    win: &tauri::WebviewWindow,
    display_id: &str,
) -> AppResult<Option<tauri::Monitor>> {
    let monitor = if display_id != "primary" {
        win.available_monitors()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|m| m.name().map(|n| n.as_str()) == Some(display_id))
    } else {
        None
    };
    match monitor {
        Some(m) => Ok(Some(m)),
        None => win.primary_monitor().map_err(|e| crate::error::AppError::from(e.to_string())),
    }
}

/// 按设置（缩放/所在显示器）摆放岛窗：顶部居中
pub fn apply_island_layout(app: &tauri::AppHandle, layout: &IslandLayout) -> AppResult<()> {
    let win = app
        .get_webview_window("island")
        .ok_or("error.io: 岛窗不存在")?;
    let scale = (layout.scale / 100.0).clamp(0.45, 3.0);
    let Some(monitor) = resolve_monitor(&win, &layout.display_id)? else {
        return Err("error.io: 找不到显示器".into());
    };

    let area = monitor.size();
    let origin = monitor.position();
    let dpi = monitor.scale_factor();
    let w = (ISLAND_BASE_W * scale * dpi) as u32;
    let h = (ISLAND_BASE_H * scale * dpi) as u32;
    let x = origin.x + (area.width as i32 - w as i32) / 2;
    win.set_size(PhysicalSize::new(w, h)).map_err(|e| e.to_string())?;
    win.set_position(PhysicalPosition::new(x, origin.y)).map_err(|e| e.to_string())?;

    apply_settings_layout(app, layout)
}

/// 设置窗跟随岛所在屏与缩放：顶部居中 + 64px 基准偏移
pub fn apply_settings_layout(app: &tauri::AppHandle, layout: &IslandLayout) -> AppResult<()> {
    let Some(win) = app.get_webview_window("settings") else {
        return Ok(());
    };
    let scale = (layout.scale / 100.0).clamp(0.45, 3.0);
    let Some(monitor) = resolve_monitor(&win, &layout.display_id)? else {
        return Ok(());
    };
    let area = monitor.size();
    let origin = monitor.position();
    let dpi = monitor.scale_factor();
    let w = (SETTINGS_BASE_W * scale * dpi) as u32;
    let h = (SETTINGS_BASE_H * scale * dpi) as u32;
    let x = origin.x + (area.width as i32 - w as i32) / 2;
    let y = origin.y + (SETTINGS_OFFSET_Y * scale * dpi) as i32;
    win.set_size(PhysicalSize::new(w, h)).map_err(|e| e.to_string())?;
    win.set_position(PhysicalPosition::new(x, y)).map_err(|e| e.to_string())?;
    Ok(())
}

/// 显示器信息（岛落屏设置用）
#[derive(Debug, serde::Serialize)]
pub struct DisplayInfo {
    pub id: String,
    pub label: String,
    pub primary: bool,
}

pub fn list_displays(app: &tauri::AppHandle) -> AppResult<Vec<DisplayInfo>> {
    let win = app.get_webview_window("island").ok_or("error.io: 岛窗不存在")?;
    let primary = win.primary_monitor().map_err(|e| e.to_string())?;
    let primary_name = primary.as_ref().and_then(|m| m.name().cloned());
    let monitors = win.available_monitors().map_err(|e| e.to_string())?;
    let friendly = island_windows::displays::monitor_friendly_names();
    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let name = m.name().cloned().unwrap_or_else(|| format!("DISPLAY{}", i + 1));
            DisplayInfo {
                primary: Some(&name) == primary_name.as_ref(),
                label: friendly.get(&name).cloned().unwrap_or_else(|| name.clone()),
                id: name,
            }
        })
        .collect())
}
