//! #[tauri::command] 的唯一所在地。每个文件一个域，只做参数适配后转交 services/infra。

use crate::error::{AppError, AppResult};

pub mod alarm;
pub mod app;
pub mod clipboard;
pub mod music;
pub mod notify;
pub mod settings;
pub mod store;
pub mod update;
pub mod weather;
pub mod wechat;
pub mod window;

/// 耗时命令统一走阻塞线程池，不堵 Tauri 的命令调度线程
async fn off_thread<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(AppError::from(format!("error.io: {e}"))))
}

/// 命令清单。新增命令只改这里，lib.rs 不必知道有哪些命令。
/// 路径写 crate:: 而非 $crate::：本宏只在 island-app 内部展开。
#[macro_export]
macro_rules! ipc_handlers {
    () => {
        tauri::generate_handler![
            crate::ipc::store::store_get,
            crate::ipc::store::store_set,
            crate::ipc::store::store_clear,
            crate::ipc::settings::settings_update,
            crate::ipc::settings::settings_open,
            crate::ipc::app::app_get_locale,
            crate::ipc::app::app_get_version,
            crate::ipc::app::displays_list,
            crate::ipc::app::shell_open_external,
            crate::ipc::app::diag_reveal,
            crate::ipc::window::window_close,
            crate::ipc::window::window_close_self,
            crate::ipc::window::window_get_cursor_point,
            crate::ipc::window::window_set_hot_rect,
            crate::ipc::weather::weather_ip_city,
            crate::ipc::weather::weather_geocode,
            crate::ipc::weather::weather_query,
            crate::ipc::update::update_status,
            crate::ipc::update::update_check,
            crate::ipc::update::update_install,
            crate::ipc::music::music_poll,
            crate::ipc::music::music_control,
            crate::ipc::music::music_seek,
            crate::ipc::music::music_artwork,
            crate::ipc::music::music_lyrics,
            crate::ipc::music::music_bridge_status,
            crate::ipc::notify::notify_activate,
            crate::ipc::notify::notify_image,
            crate::ipc::clipboard::clipboard_read_text,
            crate::ipc::clipboard::clipboard_write_text,
            crate::ipc::clipboard::clipboard_has_image,
            crate::ipc::clipboard::clipboard_read_file_paths,
            crate::ipc::alarm::alarm_sound_list,
            crate::ipc::alarm::alarm_sound_data,
            crate::ipc::alarm::alarm_sound_pick,
            crate::ipc::wechat::wechat_acquire_key,
            crate::ipc::wechat::wechat_has_key,
        ]
    };
}
