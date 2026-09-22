//! TODO Electron 下线后删模块及 lib.rs 调用点

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::infra::{paths, persist};

/// 全是 Chromium 生成的名字
const CHROMIUM_ARTIFACTS: &[&str] = &[
    "blob_storage",
    "Cache",
    "Code Cache",
    "component_crx_cache",
    "Cookies",
    "Cookies-journal",
    "databases",
    "DawnCache",
    "DawnGraphiteCache",
    "DawnWebGPUCache",
    "DevToolsActivePort",
    "DIPS",
    "DIPS-journal",
    "extensions_crx_cache",
    "GPUCache",
    "GrShaderCache",
    "IndexedDB",
    "Local Extension Settings",
    "Local State",
    "Local Storage",
    "Network",
    "Network Persistent State",
    "Preferences",
    "QuotaManager",
    "QuotaManager-journal",
    "Service Worker",
    "Session Storage",
    "ShaderCache",
    "Shared Dictionary",
    "SharedStorage",
    "SharedStorage-wal",
    "TransportSecurity",
    "Trust Tokens",
    "Trust Tokens-journal",
    "VideoDecodeStats",
    "WebStorage",
    // electron-updater 实例 id
    ".updaterId",
];

/// electron-updater 下载缓存
const UPDATER_CACHE_DIR: &str = "top-island-updater";

/// 安装器占着自身 exe 删不掉
const RETRY_DELAY: Duration = Duration::from_secs(10);

const DONE_KEY: &str = "legacySweepDone";

static STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Swept {
    pub entries: usize,
    pub bytes: u64,
    pub failed: usize,
}

fn entry_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries.flatten().map(|e| entry_size(&e.path())).sum()
}

fn remove(path: &Path) -> std::io::Result<()> {
    if std::fs::symlink_metadata(path)?.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

fn remove_if_exists(path: &Path, swept: &mut Swept) {
    if !path.exists() {
        return;
    }
    let size = entry_size(path);
    match remove(path) {
        Ok(()) => {
            swept.entries += 1;
            swept.bytes += size;
        }
        Err(e) => {
            swept.failed += 1;
            eprintln!("[legacy] 删不掉 {}: {e}", path.display());
        }
    }
}

/// EBWebView 在则是活数据
fn is_our_migrated_profile(data_dir: &Path) -> bool {
    let has_store =
        data_dir.join("store.json").exists() || data_dir.join("store.json.bak").exists();
    has_store && !data_dir.join("EBWebView").exists()
}

pub fn sweep(data_dir: &Path, local_app_data: &Path) -> Swept {
    let mut swept = Swept::default();
    if is_our_migrated_profile(data_dir) {
        for name in CHROMIUM_ARTIFACTS {
            remove_if_exists(&data_dir.join(name), &mut swept);
        }
    } else {
        eprintln!(
            "[legacy] {} 不像我们的数据目录，跳过按名字删",
            data_dir.display()
        );
    }
    // 按包名建 只可能是我们的
    remove_if_exists(&local_app_data.join(UPDATER_CACHE_DIR), &mut swept);
    swept
}

pub fn start() {
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    if persist::get(DONE_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("legacy-sweep".into())
        .spawn(|| {
            let (Ok(data_dir), Ok(local)) = (paths::data_dir(), std::env::var("LOCALAPPDATA"))
            else {
                return;
            };
            let local = PathBuf::from(local);
            let mut swept = sweep(&data_dir, &local);
            if swept.failed > 0 {
                std::thread::sleep(RETRY_DELAY);
                let again = sweep(&data_dir, &local);
                swept.entries += again.entries;
                swept.bytes += again.bytes;
                swept.failed = again.failed;
            }
            if swept.entries > 0 {
                let mb = swept.bytes / (1024 * 1024);
                eprintln!(
                    "[legacy] 清掉 Electron 残留 {} 项，释放约 {mb} MB",
                    swept.entries
                );
            }
            // 删不掉不记完成
            if swept.failed == 0 {
                let _ = persist::set(DONE_KEY, serde_json::Value::Bool(true));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("island-legacy-test-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sweep_removes_chromium_junk_but_keeps_our_own_files() {
        let root = temp_root("keep");
        let data = root.join("top-island");
        std::fs::create_dir_all(data.join("Code Cache").join("js")).unwrap();
        std::fs::write(data.join("Code Cache").join("js").join("blob"), [0u8; 1024]).unwrap();
        std::fs::write(data.join("DevToolsActivePort"), b"1234").unwrap();
        std::fs::write(data.join("store.json"), b"{}").unwrap();
        std::fs::write(data.join("island_heatmap.json"), b"{}").unwrap();
        std::fs::write(data.join("diag.log"), b"x").unwrap();

        let swept = sweep(&data, &root);

        assert!(!data.join("Code Cache").exists());
        assert!(!data.join("DevToolsActivePort").exists());
        assert!(data.join("store.json").exists());
        assert!(data.join("island_heatmap.json").exists());
        assert!(data.join("diag.log").exists());
        assert_eq!(swept.entries, 2);
        assert!(swept.bytes >= 1024);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn sweep_removes_the_electron_updater_download_cache() {
        let root = temp_root("updater");
        let data = root.join("top-island");
        std::fs::create_dir_all(&data).unwrap();
        let pending = root.join(UPDATER_CACHE_DIR).join("pending");
        std::fs::create_dir_all(&pending).unwrap();
        std::fs::write(pending.join("TopIsland_0.0.2_x64-setup.exe"), [7u8; 2048]).unwrap();

        let swept = sweep(&data, &root);

        assert!(!root.join(UPDATER_CACHE_DIR).exists());
        assert_eq!(swept.entries, 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn sweep_leaves_a_data_dir_that_is_not_ours_alone() {
        let root = temp_root("foreign");
        let data = root.join("top-island");
        std::fs::create_dir_all(data.join("Cache")).unwrap();
        std::fs::write(data.join("Cache").join("blob"), [0u8; 16]).unwrap();

        let swept = sweep(&data, &root);

        assert!(data.join("Cache").exists());
        assert_eq!(swept, Swept::default());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn sweep_leaves_a_live_webview2_profile_alone() {
        let root = temp_root("webview");
        let data = root.join("top-island");
        std::fs::create_dir_all(data.join("EBWebView")).unwrap();
        std::fs::create_dir_all(data.join("Local Storage")).unwrap();
        std::fs::write(data.join("store.json"), b"{}").unwrap();

        let swept = sweep(&data, &root);

        assert!(data.join("Local Storage").exists());
        assert_eq!(swept, Swept::default());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn sweep_does_nothing_on_a_clean_profile() {
        let root = temp_root("clean");
        let data = root.join("top-island");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("store.json"), b"{}").unwrap();

        assert_eq!(sweep(&data, &root), Swept::default());
        assert!(data.join("store.json").exists());
        std::fs::remove_dir_all(&root).ok();
    }
}
