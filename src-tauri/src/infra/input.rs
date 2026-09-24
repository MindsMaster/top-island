use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use island_windows::{HoverChange, InputHandlers, Rect};

use crate::error::AppResult;

/// 与 src/platform/window.ts onHover 一致
const HOVER_EVENT: &str = "island:hover";
const INITIAL_HOT_HEIGHT: f64 = 72.0;

/// CSS 像素 相对窗口内容区
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// 穿透开关唯一入口 须先于前端上报热区
pub fn start(app: &AppHandle) -> tauri::Result<()> {
    let Some(win) = app.get_webview_window("island") else {
        return Ok(());
    };
    win.set_ignore_cursor_events(true)?;
    let initial = HotRect {
        x: 0.0,
        y: 0.0,
        width: f64::from(win.inner_size()?.width) / win.scale_factor()?,
        height: INITIAL_HOT_HEIGHT,
    };

    let hover_win = win.clone();
    let clip_app = app.clone();
    island_windows::start_input(InputHandlers {
        interactive_rect: Some(to_screen(&win, initial)?),
        on_hover: Some(Box::new(move |change: HoverChange| {
            let _ = hover_win.set_ignore_cursor_events(!change.interactive);
            // 前端 mouseleave 可能先行改写 每次都重申当前值
            let _ = hover_win.emit_to("island", HOVER_EVENT, change.hover);
        })),
        on_clipboard: Some(Box::new(move || {
            let _ = clip_app.emit("clipboard:changed", ());
        })),
    });
    Ok(())
}

/// interactive 为 None 即整窗可交互
pub fn set_hot_rect(
    win: &WebviewWindow,
    interactive: Option<HotRect>,
    hover: Option<HotRect>,
) -> AppResult<()> {
    let to_screen = |r| to_screen(win, r).map_err(|e| e.to_string());
    let interactive = interactive.map(to_screen).transpose()?;
    let hover = hover.map(to_screen).transpose()?;
    island_windows::input::set_hover_rect(interactive, hover);
    Ok(())
}

fn to_screen(win: &WebviewWindow, r: HotRect) -> tauri::Result<Rect> {
    let dpi = win.scale_factor()?;
    let origin = win.inner_position()?;
    Ok(Rect {
        left: origin.x + (r.x * dpi) as i32,
        top: origin.y + (r.y * dpi) as i32,
        right: origin.x + ((r.x + r.width) * dpi) as i32,
        bottom: origin.y + ((r.y + r.height) * dpi) as i32,
    })
}
