use tauri::{AppHandle, WebviewWindow};

use crate::error::AppResult;
use crate::infra;
use crate::infra::input::HotRect;

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

#[tauri::command]
pub fn window_set_hot_rect(
    window: WebviewWindow,
    interactive: Option<HotRect>,
    hover: Option<HotRect>,
) -> AppResult<()> {
    infra::input::set_hot_rect(&window, interactive, hover)
}
