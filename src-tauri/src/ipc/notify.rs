use crate::error::{AppError, AppResult};
use crate::services;

/// 耗时命令统一走阻塞线程池，不堵 Tauri UI 线程（模板见 ipc/mod.rs 顶部）
async fn off_thread<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(AppError::from(format!("error.io: {e}"))))
}

/// 复现点击：激活来源应用（protocol 直开 → COM activator → AAM → shell 兜底）。
/// 返回实际生效方式；独立于托管开关——用户临时关了托管也应能点开已收到的消息。
// fn 名不能叫 notify_activate：ipc/mod.rs 的 spike 同名命令还没退役，
// tauri 宏生成的辅助宏会撞名；invoke 名用 rename 钉在契约上
#[tauri::command(rename = "notify_activate")]
pub async fn notify_activate_toast(aumid: String, launch: String, atype: String) -> AppResult<String> {
    // 微信消息卡片：自绘应用无系统激活器，用协议唤起（沿用 Electron 版特例）
    if aumid == "wechat" {
        tauri_plugin_opener::open_url("weixin://", None::<&str>)
            .map_err(|e| AppError::new(format!("error.io: 唤起微信失败: {e}")))?;
        return Ok("wechat".into());
    }
    off_thread(move || Ok(island_windows::activate::activate_toast(&aumid, &launch, &atype))).await
}

/// 通知图片/头像 → data URL。内容认不出是图片的来源返回 null，渲染层回退首字母块
#[tauri::command]
pub async fn notify_image(src: String) -> AppResult<Option<String>> {
    off_thread(move || Ok(services::notify::notify_image(&src))).await
}
