use crate::error::{AppError, AppResult};

use super::off_thread;

#[tauri::command]
pub async fn notify_activate(aumid: String, launch: String, atype: String) -> AppResult<String> {
    // 微信无系统激活器 走协议唤起
    if aumid == "wechat" {
        tauri_plugin_opener::open_url("weixin://", None::<&str>)
            .map_err(|e| AppError::new(format!("error.io: 唤起微信失败: {e}")))?;
        return Ok("wechat".into());
    }
    off_thread(move || {
        Ok(island_windows::activate::activate_toast(
            &aumid, &launch, &atype,
        ))
    })
    .await
}
