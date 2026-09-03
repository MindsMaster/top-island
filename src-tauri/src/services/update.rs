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
        Self { status: status.into(), version: None, message: None }
    }
}

static STATUS: Mutex<Option<UpdateStatus>> = Mutex::new(None);
static DOWNLOADED: AtomicBool = AtomicBool::new(false);

fn set_status(status: UpdateStatus) {
    *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = Some(status);
}

pub fn status() -> UpdateStatus {
    STATUS.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_else(|| {
        if cfg!(debug_assertions) {
            UpdateStatus::new("dev")
        } else {
            UpdateStatus::new("not-available")
        }
    })
}

fn channel(version: &str) -> &'static str {
    // 版本号以 -SNAPSHOT 结尾走快照渠道（与 Electron 版约定一致）
    if version.to_uppercase().ends_with("-SNAPSHOT") {
        "snapshot"
    } else {
        "latest"
    }
}

/// 同一自然日非强制只查一次（持久化到 store，重启也记得）
fn checked_today() -> bool {
    let today = chrono_today();
    persist::get("updateLastCheckDate")
        .and_then(|v| v.as_str().map(String::from))
        .map(|d| d == today)
        .unwrap_or(false)
}

fn mark_checked_today() {
    let _ = persist::set("updateLastCheckDate", serde_json::Value::String(chrono_today()));
}

fn chrono_today() -> String {
    // 不引 chrono：SYSTEMTIME 转本地日期串
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
    if cfg!(debug_assertions) {
        // dev 构建没有安装包可更新，对齐 Electron 的 dev 行为
        let st = UpdateStatus::new("dev");
        set_status(st.clone());
        return st;
    }

    set_status(UpdateStatus::new("checking"));
    let version = app.package_info().version.to_string();
    let url = format!("{FEED_BASE}/{}.json", channel(&version));
    let result: AppResult<Option<tauri_plugin_updater::Update>> = async {
        let endpoint = url.parse().map_err(|e| AppError::new(format!("error.io: feed 地址: {e}")))?;
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
            // autoDownload 语义：检查到即下载安装，装完等用户重启
            match update.download_and_install(|_, _| {}, || {}).await {
                Ok(()) => {
                    DOWNLOADED.store(true, Ordering::Relaxed);
                    let mut st = UpdateStatus::new("downloaded");
                    st.version = Some(update.version.clone());
                    let _ = app.emit("update:downloaded", serde_json::json!({ "version": update.version }));
                    st
                }
                Err(e) => {
                    let mut st = UpdateStatus::new("error");
                    st.message = Some(format!("下载安装失败: {e}"));
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

/// 安装已下载的更新：重启即生效（NSIS 已装好，重启进新版本）
pub fn install(app: &AppHandle) -> AppResult<()> {
    if !DOWNLOADED.load(Ordering::Relaxed) {
        return Err(AppError::new("error.io: 没有已下载的更新"));
    }
    app.restart();
}

/// 启动即查 + 每小时（非强制，按日去重）
pub fn start(app: &AppHandle) {
    let handle = app.clone();
    std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
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
