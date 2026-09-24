use tauri::WebviewWindow;

use crate::error::AppResult;
use crate::infra;
use crate::infra::input::HotRect;

#[tauri::command]
pub fn window_set_hot_rect(
    window: WebviewWindow,
    interactive: Option<HotRect>,
    hover: Option<HotRect>,
) -> AppResult<()> {
    infra::input::set_hot_rect(&window, interactive, hover)
}
