use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::error::{AppError, AppResult};

struct Store {
    path: PathBuf,
    data: Mutex<serde_json::Map<String, serde_json::Value>>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("path", &self.path)
            .finish_non_exhaustive()
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

fn load() -> AppResult<Store> {
    let path = crate::infra::paths::data_dir()?.join("store.json");
    let bak = path.with_file_name("store.json.bak");
    let data = read_json(&path)
        .or_else(|| read_json(&bak))
        .unwrap_or_default();
    Ok(Store {
        path,
        data: Mutex::new(data),
    })
}

pub fn init() -> AppResult<()> {
    let _ = store();
    Ok(())
}

/// webview 加载可能早于 setup
fn store() -> &'static Store {
    STORE.get_or_init(|| {
        load().unwrap_or_else(|e| {
            eprintln!("[persist] store 初始化失败，退化为内存空库: {e}");
            Store {
                path: PathBuf::new(),
                data: Mutex::new(serde_json::Map::new()),
            }
        })
    })
}

fn lock() -> MutexGuard<'static, serde_json::Map<String, serde_json::Value>> {
    store().data.lock().unwrap_or_else(|e| e.into_inner())
}

fn flush(guard: &serde_json::Map<String, serde_json::Value>) -> AppResult<()> {
    let path = &store().path;
    if path.as_os_str().is_empty() {
        // 内存退化模式
        return Ok(());
    }
    let tmp = path.with_file_name("store.json.tmp");
    let bak = path.with_file_name("store.json.bak");
    let text = serde_json::to_string(guard)
        .map_err(|e| AppError::new(format!("error.io: 序列化 store: {e}")))?;
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

pub fn clear() -> AppResult<()> {
    let mut guard = lock();
    guard.clear();
    flush(&guard)
}
