use tauri::{AppHandle, WebviewWindow};

use crate::error::AppResult;

#[tauri::command]
pub fn window_close(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn window_close_self(window: WebviewWindow) {
    // conf 窗口关闭不可重建 设置窗只隐藏
    if window.label() == "settings" {
        let _ = window.hide();
    } else {
        let _ = window.close();
    }
}

/// mouseleave 不触发时的兜底
#[tauri::command]
pub fn window_get_cursor_point(window: WebviewWindow) -> AppResult<(i32, i32)> {
    let (x, y) = island_windows::input::cursor_position();
    let origin = window.inner_position().map_err(|e| e.to_string())?;
    Ok((x - origin.x, y - origin.y))
}

/// CSS 像素 相对窗口内容区 空即全程可交互
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[tauri::command]
pub fn window_set_hot_rect(
    window: WebviewWindow,
    interactive: Option<HotRect>,
    hover: Option<HotRect>,
) -> AppResult<()> {
    let dpi = window.scale_factor().map_err(|e| e.to_string())?;
    let origin = window.outer_position().map_err(|e| e.to_string())?;
    let to_phys = |r: HotRect| island_windows::Rect {
        left: origin.x + (r.x * dpi) as i32,
        top: origin.y + (r.y * dpi) as i32,
        right: origin.x + ((r.x + r.width) * dpi) as i32,
        bottom: origin.y + ((r.y + r.height) * dpi) as i32,
    };
    let passive = interactive.is_none();
    island_windows::input::set_hover_rect(interactive.map(to_phys), hover.map(to_phys));
    if passive {
        window
            .set_ignore_cursor_events(false)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
