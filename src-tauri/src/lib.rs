pub mod error;
mod infra;
mod ipc;
mod services;

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

use island_core::AppSettings;
use island_windows::{InputHandlers, Rect};

/// 岛窗悬停热区：面板收起时只有胶囊一条，展开后放大到整窗
struct HotRegion {
    left: i32,
    top: i32,
    right: i32,
    cap_h: i32,
    full_h: i32,
}

static HOT: Mutex<Option<HotRegion>> = Mutex::new(None);

pub fn set_panel_hot(open: bool) {
    let guard = HOT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(h) = guard.as_ref() {
        let height = if open { h.full_h } else { h.cap_h };
        island_windows::input::set_hover_rect(Rect {
            left: h.left,
            top: h.top,
            right: h.right,
            bottom: h.top + height,
        });
    }
}

fn init_input(app: &AppHandle) {
    let Some(win) = app.get_webview_window("island") else { return };
    let (Ok(pos), Ok(size), Ok(dpi)) =
        (win.outer_position(), win.outer_size(), win.scale_factor())
    else {
        return;
    };
    let cap_h = (72.0 * dpi) as i32;
    let region = Rect {
        left: pos.x,
        top: pos.y,
        right: pos.x + size.width as i32,
        bottom: pos.y + cap_h,
    };
    *HOT.lock().unwrap_or_else(|e| e.into_inner()) = Some(HotRegion {
        left: region.left,
        top: region.top,
        right: region.right,
        cap_h,
        full_h: size.height as i32,
    });

    let hover_app = app.clone();
    let clip_app = app.clone();
    island_windows::start_input(InputHandlers {
        hover_rect: Some(region),
        on_hover: Some(Box::new(move |inside| {
            if let Some(win) = hover_app.get_webview_window("island") {
                let _ = win.set_ignore_cursor_events(!inside);
                let _ = hover_app.emit("island-hover", inside);
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
        .setup(|app| {
            infra::persist::init()?;

            let settings = infra::persist::get("settings")
                .map(AppSettings::from_value)
                .unwrap_or_default();
            infra::layout::apply_island_layout(app.handle(), &settings.island)?;
            infra::autolaunch::sync(settings.auto_launch)?;

            let win = app.get_webview_window("island").expect("island window");
            win.set_ignore_cursor_events(true)?;
            init_input(app.handle());
            infra::tray::build(app.handle())?;

            services::music::sync(app.handle(), &settings);
            services::notify::sync(app.handle(), &settings);
            services::wechat::sync(app.handle(), &settings);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::store_get,
            ipc::store_set,
            ipc::store_clear,
            ipc::settings_update,
            ipc::settings_open,
            ipc::weather_ip_city,
            ipc::weather_geocode,
            ipc::weather_query,
            ipc::app_get_locale,
            ipc::app_get_version,
            ipc::displays_list,
            ipc::shell_open_external,
            ipc::window_close,
            ipc::window_close_self,
            ipc::window_get_cursor_point,
            ipc::music::music_poll,
            ipc::music::music_control,
            ipc::music::music_seek,
            ipc::music::music_artwork,
            ipc::music::music_lyrics,
            ipc::notify::notify_activate_toast,
            ipc::notify::notify_image,
            ipc::clipboard::clipboard_read_text,
            ipc::clipboard::clipboard_write_text,
            ipc::clipboard::clipboard_has_image,
            ipc::clipboard::clipboard_read_file_paths,
            ipc::clipboard::clipboard_sequence_number,
            ipc::alarm::alarm_sound_list,
            ipc::alarm::alarm_sound_data,
            ipc::alarm::alarm_sound_pick,
            ipc::wechat::wechat_acquire_key,
            ipc::wechat::wechat_has_key,
            ipc::smtc_now,
            ipc::set_panel_open,
        ])
        .run(tauri::generate_context!())
        .expect("top island run");
}
