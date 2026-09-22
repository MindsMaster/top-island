use crate::error::AppResult;
use crate::infra;

use super::off_thread;

#[tauri::command]
pub async fn store_get(key: String) -> AppResult<serde_json::Value> {
    off_thread(move || Ok(infra::persist::get(&key).unwrap_or(serde_json::Value::Null))).await
}

#[tauri::command]
pub async fn store_set(key: String, value: serde_json::Value) -> AppResult<()> {
    off_thread(move || infra::persist::set(&key, value)).await
}

#[tauri::command]
pub async fn store_clear(app: tauri::AppHandle) -> AppResult<()> {
    infra::persist::clear()?;
    app.restart();
}
