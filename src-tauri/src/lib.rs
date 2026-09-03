mod input;
mod notify;
mod smtc;

use tauri::Manager;
use windows::Win32::Foundation::RECT;

#[tauri::command]
async fn smtc_now() -> Result<Option<smtc::NowPlaying>, String> {
    tauri::async_runtime::spawn_blocking(smtc::now_playing)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.message())
}

#[tauri::command]
async fn notify_recent(limit: Option<i64>) -> Result<Vec<notify::ToastRow>, String> {
    let limit = limit.unwrap_or(5);
    tauri::async_runtime::spawn_blocking(move || notify::recent_toasts(limit))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn notify_activate(aumid: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || notify::activate(&aumid))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn set_panel_open(open: bool) {
    input::set_panel_open(open);
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let win = app.get_webview_window("island").expect("island window");

            if let Some(mon) = win.primary_monitor()? {
                let area = mon.size();
                let origin = mon.position();
                let size = win.outer_size()?;
                let x = origin.x + (area.width as i32 - size.width as i32) / 2;
                win.set_position(tauri::PhysicalPosition::new(x, origin.y))?;
            }

            win.set_ignore_cursor_events(true)?;

            let pos = win.outer_position()?;
            let scale = win.scale_factor()?;
            let size = win.outer_size()?;
            let rect = RECT {
                left: pos.x,
                top: pos.y,
                right: pos.x + size.width as i32,
                bottom: pos.y + size.height as i32,
            };
            let cap_h = (72.0 * scale) as i32;
            input::start(app.handle().clone(), rect, cap_h, size.height as i32);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            smtc_now,
            notify_recent,
            notify_activate,
            set_panel_open
        ])
        .run(tauri::generate_context!())
        .expect("island spike run");
}
