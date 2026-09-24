use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

use crate::error::{AppError, AppResult};
use crate::infra::persist;

const FEED_BASE: &str = "https://repo.azuramc.cc/repository/raw-public/top-island";
const CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3600);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl UpdateStatus {
    fn new(status: &str) -> Self {
        Self {
            status: status.into(),
            version: None,
            message: None,
        }
    }
}

static STATUS: Mutex<Option<UpdateStatus>> = Mutex::new(None);
static DOWNLOADED: AtomicBool = AtomicBool::new(false);
/// install 当场 exit 存着等用户点
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

struct Pending {
    update: tauri_plugin_updater::Update,
    bytes: Vec<u8>,
}

fn set_status(status: UpdateStatus) {
    *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = Some(status);
}

pub fn status() -> UpdateStatus {
    STATUS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| {
            if tauri::is_dev() {
                UpdateStatus::new("dev")
            } else {
                UpdateStatus::new("not-available")
            }
        })
}

fn channel(version: &str) -> &'static str {
    // -SNAPSHOT 走快照渠道
    if version.to_uppercase().ends_with("-SNAPSHOT") {
        "snapshot"
    } else {
        "latest"
    }
}

fn checked_today() -> bool {
    let today = chrono_today();
    persist::get("updateLastCheckDate")
        .and_then(|v| v.as_str().map(String::from))
        .map(|d| d == today)
        .unwrap_or(false)
}

fn mark_checked_today() {
    let _ = persist::set(
        "updateLastCheckDate",
        serde_json::Value::String(chrono_today()),
    );
}

fn chrono_today() -> String {
    // 免引 chrono
    let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:04}-{:02}-{:02}", st.wYear, st.wMonth, st.wDay)
}

async fn run_check(app: AppHandle, force: bool) -> UpdateStatus {
    if DOWNLOADED.load(Ordering::Relaxed) {
        return status();
    }
    if !force && checked_today() {
        return status();
    }
    if tauri::is_dev() {
        let st = UpdateStatus::new("dev");
        set_status(st.clone());
        return st;
    }

    set_status(UpdateStatus::new("checking"));
    let version = app.package_info().version.to_string();
    let url = format!("{FEED_BASE}/{}.json", channel(&version));
    let result: AppResult<Option<tauri_plugin_updater::Update>> = async {
        let endpoint = url
            .parse()
            .map_err(|e| AppError::new(format!("error.io: feed 地址: {e}")))?;
        let updater = app
            .updater_builder()
            .endpoints(vec![endpoint])
            .map_err(|e| AppError::new(format!("error.io: 更新器配置: {e}")))?
            .build()
            .map_err(|e| AppError::new(format!("error.network: 更新器初始化: {e}")))?;
        updater
            .check()
            .await
            .map_err(|e| AppError::new(format!("error.network: 检查更新: {e}")))
    }
    .await;

    let st = match result {
        Ok(Some(update)) => {
            let mut st = UpdateStatus::new("available");
            st.version = Some(update.version.clone());
            set_status(st);
            // 查到即下载 装等用户点
            match update.download(|_, _| {}, || {}).await {
                Ok(bytes) => {
                    let version = update.version.clone();
                    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(Pending { update, bytes });
                    DOWNLOADED.store(true, Ordering::Relaxed);
                    let mut st = UpdateStatus::new("downloaded");
                    st.version = Some(version.clone());
                    let _ = app.emit(
                        "update:downloaded",
                        serde_json::json!({ "version": version }),
                    );
                    st
                }
                Err(e) => {
                    let mut st = UpdateStatus::new("error");
                    st.message = Some(format!("下载失败: {e}"));
                    st
                }
            }
        }
        Ok(None) => {
            let mut st = UpdateStatus::new("not-available");
            st.version = Some(version);
            st
        }
        Err(e) => {
            let mut st = UpdateStatus::new("error");
            st.message = Some(e.to_string());
            st
        }
    };
    mark_checked_today();
    set_status(st.clone());
    st
}

pub async fn check(app: AppHandle, force: bool) -> UpdateStatus {
    run_check(app, force).await
}

/// install 成功即退出进程
pub fn install() -> AppResult<()> {
    let Some(pending) = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return Err(AppError::new("error.io: 没有已下载的更新"));
    };
    if let Err(e) = pending.update.install(&pending.bytes) {
        // 失败不丢包
        *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(pending);
        let mut st = UpdateStatus::new("error");
        st.message = Some(format!("安装失败: {e}"));
        set_status(st);
        return Err(AppError::new(format!("error.io: 安装更新: {e}")));
    }
    Ok(())
}

/// 更新器在 %TEMP% 留残包
fn clean_stale_downloads(app: &AppHandle) {
    let prefix = format!("{}-", app.package_info().name);
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) && name.contains("-updater-") {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

pub fn start(app: &AppHandle) {
    let handle = app.clone();
    std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            clean_stale_downloads(&handle);
            tauri::async_runtime::block_on(run_check(handle.clone(), false));
            loop {
                std::thread::sleep(CHECK_INTERVAL);
                if DOWNLOADED.load(Ordering::Relaxed) {
                    break;
                }
                tauri::async_runtime::block_on(run_check(handle.clone(), false));
            }
        })
        .expect("spawn update-check");
}
