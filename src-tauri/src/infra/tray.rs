use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

/// 托盘菜单：打开设置 / 退出。开发者工具只给 debug 构建，生产不该常驻。
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open-settings", "打开设置", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let mut items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![&open, &sep, &quit];
    #[cfg(debug_assertions)]
    let devtools = MenuItem::with_id(app, "devtools", "打开开发者工具", true, None::<&str>)?;
    #[cfg(debug_assertions)]
    items.insert(0, &devtools);

    let menu = Menu::with_items(app, &items)?;
    let mut builder = TrayIconBuilder::new().menu(&menu).tooltip("Top Island");
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_menu_event(|app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
            "open-settings" => {
                if let Some(win) = app.get_webview_window("settings") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            #[cfg(debug_assertions)]
            "devtools" => {
                if let Some(win) = app.get_webview_window("island") {
                    win.open_devtools();
                }
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}
