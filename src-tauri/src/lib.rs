pub mod error;
mod infra;
mod ipc;
mod services;

use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, Manager};

use island_core::AppSettings;
use island_windows::{InputHandlers, Rect};

fn init_input(app: &AppHandle) {
    let Some(win) = app.get_webview_window("island") else { return };
    let (Ok(pos), Ok(size), Ok(dpi)) =
        (win.outer_position(), win.outer_size(), win.scale_factor())
    else {
        return;
    };
    // 前端挂载前先用胶囊高度当初始热区，挂载后由 window_set_hot_rect 接管
    let region = Rect {
        left: pos.x,
        top: pos.y,
        right: pos.x + size.width as i32,
        bottom: pos.y + (72.0 * dpi) as i32,
    };

    let hover_app = app.clone();
    let clip_app = app.clone();
    island_windows::start_input(InputHandlers {
        interactive_rect: Some(region),
        on_hover: Some(Box::new(move |change| {
            if let Some(win) = hover_app.get_webview_window("island") {
                let _ = win.set_ignore_cursor_events(!change.interactive);
                let _ = hover_app.emit("island-hover", change.hover);
            }
        })),
        on_clipboard: Some(Box::new(move || {
            let _ = clip_app.emit("clipboard:changed", ());
        })),
    });
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("island") {
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // 首屏加载完再清 Electron 残留：启动路径上不做磁盘扫描
        .on_page_load(|webview, payload| {
            if webview.label() == "island" && payload.event() == PageLoadEvent::Finished {
                infra::legacy::start();
            }
        })
        .setup(|app| {
            infra::persist::init()?;

            let settings = infra::persist::get("settings")
                .map(AppSettings::from_value)
                .unwrap_or_default();
            // 必须在 init_input 之前：它按窗口位置算初始热区
            services::apply_settings(app.handle(), &settings)?;

            let win = app.get_webview_window("island").expect("island window");
            win.set_ignore_cursor_events(true)?;
            infra::watchdog::start(app.handle().clone());
            init_input(app.handle());
            infra::tray::build(app.handle())?;
            services::update::start(app.handle());
            Ok(())
        })
        .invoke_handler(crate::ipc_handlers!())
        .run(tauri::generate_context!())
        .expect("top island run");
}
