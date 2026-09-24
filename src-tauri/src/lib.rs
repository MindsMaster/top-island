pub mod error;
mod infra;
mod ipc;
mod services;

use tauri::webview::PageLoadEvent;
use tauri::Manager;

use island_core::AppSettings;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("island") {
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // 首屏后再清 Electron 残留
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
            // 须先于热区初始化摆好窗位
            services::apply_settings(app.handle(), &settings)?;

            infra::watchdog::start(app.handle().clone());
            infra::input::start(app.handle())?;
            infra::tray::build(app.handle())?;
            services::update::start(app.handle());
            Ok(())
        })
        .invoke_handler(crate::ipc_handlers!())
        .run(tauri::generate_context!())
        .expect("top island run");
}
