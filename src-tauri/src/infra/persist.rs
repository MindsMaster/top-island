use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::error::{AppError, AppResult};

/// JSON 文件 KV store。每次 set 立即原子落盘（tmp → bak → rename），
/// 不做 Electron 版那种 300ms 防抖——文件很小，防抖只会留数据丢失窗口。
struct Store {
    path: PathBuf,
    data: Mutex<serde_json::Map<String, serde_json::Value>>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").field("path", &self.path).finish_non_exhaustive()
    }
}

static STORE: OnceLock<Store> = OnceLock::new();

fn read_json(path: &std::path::Path) -> Option<serde_json::Map<String, serde_json::Value>> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(serde_json::Value::Object(map)) => Some(map),
        _ => None,
    }
}

pub fn init() -> AppResult<()> {
    let path = crate::infra::paths::data_dir()?.join("store.json");
    let bak = path.with_file_name("store.json.bak");
    let data = read_json(&path).or_else(|| read_json(&bak)).unwrap_or_default();
    STORE
        .set(Store { path, data: Mutex::new(data) })
        .map_err(|_| AppError::new("error.io: store 重复初始化"))
}

fn store() -> &'static Store {
    STORE.get().expect("store 未初始化")
}

fn lock() -> MutexGuard<'static, serde_json::Map<String, serde_json::Value>> {
    store().data.lock().unwrap_or_else(|e| e.into_inner())
}

fn flush(guard: &serde_json::Map<String, serde_json::Value>) -> AppResult<()> {
    let path = &store().path;
    let tmp = path.with_file_name("store.json.tmp");
    let bak = path.with_file_name("store.json.bak");
    let text = serde_json::to_string(guard).map_err(|e| AppError::new(format!("error.io: 序列化 store: {e}")))?;
    std::fs::write(&tmp, text).map_err(|e| AppError::io_at("写 store.tmp", &e))?;
    if path.exists() {
        std::fs::copy(path, &bak).map_err(|e| AppError::io_at("备份 store.bak", &e))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| AppError::io_at("替换 store.json", &e))?;
    Ok(())
}

pub fn get(key: &str) -> Option<serde_json::Value> {
    lock().get(key).cloned()
}

pub fn set(key: &str, value: serde_json::Value) -> AppResult<()> {
    let mut guard = lock();
    guard.insert(key.to_string(), value);
    flush(&guard)
}

/// 清空全部数据（设置里的「重置」用）
pub fn clear() -> AppResult<()> {
    let mut guard = lock();
    guard.clear();
    flush(&guard)
}
