pub mod error;
mod infra;
mod ipc;
mod services;

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
            ipc::window_set_hot_rect,
            ipc::update_status,
            ipc::update_check,
            ipc::update_install,
            ipc::diag_reveal,
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
        ])
        .run(tauri::generate_context!())
        .expect("top island run");
}
