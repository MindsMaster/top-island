use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;

/// 开发者工具仅 debug 构建
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open-settings", "打开设置", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    #[cfg(debug_assertions)]
    let menu = {
        let devtools = MenuItem::with_id(app, "devtools", "打开开发者工具", true, None::<&str>)?;
        Menu::with_items(app, &[&devtools, &open, &sep, &quit])?
    };
    #[cfg(not(debug_assertions))]
    let menu = Menu::with_items(app, &[&open, &sep, &quit])?;
    let mut builder = TrayIconBuilder::new().menu(&menu).tooltip("Top Island");
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_menu_event(|app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
            "open-settings" => {
                if let Err(e) = crate::infra::layout::open_settings(app) {
                    eprintln!("[tray] 打开设置失败: {e}");
                }
            }
            #[cfg(debug_assertions)]
            "devtools" => {
                use tauri::Manager;
                if let Some(win) = app.get_webview_window("island") {
                    win.open_devtools();
                }
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}
