//! 老 Electron 版留下的残留清理。安装器有意不动 %APPDATA%\top-island（用户数据在里面，
//! store.json 要原样交给新版），electron-updater 的下载缓存里躺着的又正是把安装器
//! 拉起来的那个 exe，安装期删不掉自己。两笔都留到新版启动后由这里收尾。
//!
//! 迁移窗口期专用代码：扫干净后往 store 记一笔，之后启动不再扫。
//! TODO(0.1.0): Electron 版（0.0.1-*-SNAPSHOT）下线后删掉整个模块，
//! 连带 lib.rs 里 on_page_load 的调用。之后更新只会在 %TEMP% 留一份安装包，
//! 由 services::update::clean_stale_downloads 管。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::infra::{paths, persist};

/// 数据目录里属于 Chromium/Electron 运行时的条目。名字全由 Chromium 生成，
/// 新版自己的文件（store.json、island_heatmap.json、diag*.log）都不在其中，
/// 将来加自家文件也不会撞上，所以不需要一次性标记，每次启动扫一遍即可。
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
    // electron-updater 写的实例 id，Tauri 更新器不读
    ".updaterId",
];

/// electron-updater 的下载缓存（%LOCALAPPDATA% 下）。迁移那一趟的安装包留在里面的
/// pending\，Tauri 更新器改用临时目录，整个目录都是死的。
const UPDATER_CACHE_DIR: &str = "top-island-updater";

/// 迁移安装器把新版拉起来后自己还要跑完退出，那之前它占着 pending 里的自身 exe。
/// 第一遍有删不掉的就等这么久再来一遍。
const RETRY_DELAY: Duration = Duration::from_secs(10);

/// store 里的完成标记，避免每次启动都扫
const DONE_KEY: &str = "legacySweepDone";

static STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Swept {
    pub entries: usize,
    pub bytes: u64,
    pub failed: usize,
}

fn entry_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return 0 };
    if !meta.is_dir() {
        return meta.len();
    }
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
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

/// 下面这批名字都是 Chromium 生成的，只有确认这个目录确实是我们自己的
/// 数据目录、而且不是活着的 WebView2 用户目录时才敢按名字删：
/// 目录里得有 store.json（老版 Electron 写的，或新版首启写的），
/// 且不能有 EBWebView——那意味着 WebView2 的 user data 就放在这儿，
/// Cookies / Local Storage 这些是活数据而不是残留。
fn is_our_migrated_profile(data_dir: &Path) -> bool {
    let has_store =
        data_dir.join("store.json").exists() || data_dir.join("store.json.bak").exists();
    has_store && !data_dir.join("EBWebView").exists()
}

/// 清 data_dir 下的 Chromium 残留和 local_app_data 下的 electron-updater 缓存。
/// 路径由调用方给，便于测试；不存在或删不掉的一律跳过。
pub fn sweep(data_dir: &Path, local_app_data: &Path) -> Swept {
    let mut swept = Swept::default();
    if is_our_migrated_profile(data_dir) {
        for name in CHROMIUM_ARTIFACTS {
            remove_if_exists(&data_dir.join(name), &mut swept);
        }
    } else {
        eprintln!("[legacy] {} 不像我们的数据目录，跳过按名字删", data_dir.display());
    }
    // 这个目录名（<包名>-updater）是 electron-updater 按包名建的，只可能是我们的
    remove_if_exists(&local_app_data.join(UPDATER_CACHE_DIR), &mut swept);
    swept
}

/// 前端首屏加载完成后触发（lib.rs 的 on_page_load），不挡启动路径。
/// 重复调用只会跑第一次；已经扫干净过的装机直接返回，不碰磁盘。
pub fn start() {
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    if persist::get(DONE_KEY).and_then(|v| v.as_bool()).unwrap_or(false) {
        return;
    }
    let _ = std::thread::Builder::new().name("legacy-sweep".into()).spawn(|| {
        let (Ok(data_dir), Ok(local)) = (paths::data_dir(), std::env::var("LOCALAPPDATA")) else {
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
            eprintln!("[legacy] 清掉 Electron 残留 {} 项，释放约 {mb} MB", swept.entries);
        }
        // 有删不掉的就先不记完成，下次启动再来
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

        assert!(!data.join("Code Cache").exists(), "Chromium 缓存目录得删，留着白占几百 MB");
        assert!(!data.join("DevToolsActivePort").exists(), "Chromium 的单文件残留也要删");
        assert!(data.join("store.json").exists(), "store.json 是用户设置，清理绝不能碰");
        assert!(data.join("island_heatmap.json").exists(), "自家数据文件必须保留");
        assert!(data.join("diag.log").exists(), "日志不在名单里：将来自家写 diag.log 不能被误删");
        assert_eq!(swept.entries, 2, "本例只有两项残留");
        assert!(swept.bytes >= 1024, "释放量要把目录内文件算进去");
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

        assert!(!root.join(UPDATER_CACHE_DIR).exists(), "迁移用的安装包缓存要删，安装器删不掉自己");
        assert_eq!(swept.entries, 1, "只该删更新缓存这一项");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn sweep_leaves_a_data_dir_that_is_not_ours_alone() {
        let root = temp_root("foreign");
        let data = root.join("top-island");
        std::fs::create_dir_all(data.join("Cache")).unwrap();
        std::fs::write(data.join("Cache").join("blob"), [0u8; 16]).unwrap();

        let swept = sweep(&data, &root);

        assert!(
            data.join("Cache").exists(),
            "没有 store.json 就不能认定是我们的目录：同名第三方软件的缓存不能删"
        );
        assert_eq!(swept, Swept::default(), "认不出来时一个都不该删");
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

        assert!(
            data.join("Local Storage").exists(),
            "WebView2 的 user data 若放在这里，这些名字是活数据不是残留"
        );
        assert_eq!(swept, Swept::default(), "有 EBWebView 就整轮跳过");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn sweep_does_nothing_on_a_clean_profile() {
        let root = temp_root("clean");
        let data = root.join("top-island");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("store.json"), b"{}").unwrap();

        assert_eq!(sweep(&data, &root), Swept::default(), "没有残留时不该有任何删除动作");
        assert!(data.join("store.json").exists(), "空跑不能动用户数据");
        std::fs::remove_dir_all(&root).ok();
    }
}
