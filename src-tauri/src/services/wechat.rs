use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use island_wechat::Zeroizing;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use island_core::{AppSettings, NotificationItem};

use crate::error::{AppError, AppResult};
use crate::infra::{paths, persist};

/// watch 漏事件兜底
const POLL_INTERVAL: Duration = Duration::from_millis(1500);
/// 合并落库连写
const WATCH_DEBOUNCE: Duration = Duration::from_millis(100);
const ERR_LOG_THROTTLE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredKey {
    key_hex: String,
    wxid: String,
    data_dir: String,
    recovered_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquireKeyResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wxid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn key_file() -> AppResult<PathBuf> {
    Ok(paths::data_dir()?.join("wechat.key"))
}

fn load_key() -> Option<StoredKey> {
    let file = key_file().ok()?;
    let blob = std::fs::read(&file).ok()?;
    let plain = match island_wechat::dpapi::unprotect(&blob) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[wechat] 密钥缓存解密失败（换了 Windows 用户/凭据重置？）: {e}");
            return None;
        }
    };
    match serde_json::from_slice::<StoredKey>(&plain) {
        Ok(k) => Some(k),
        Err(e) => {
            eprintln!("[wechat] 密钥缓存解析失败: {e}");
            None
        }
    }
}

fn store_key(stored: &StoredKey) -> AppResult<()> {
    let json = Zeroizing::new(
        serde_json::to_vec(stored)
            .map_err(|e| AppError::new(format!("error.io: 序列化密钥缓存: {e}")))?,
    );
    let blob = island_wechat::dpapi::protect(&json)
        .map_err(|e| AppError::new(format!("error.io: DPAPI 加密密钥: {e}")))?;
    let file = key_file()?;
    let tmp = file.with_extension("key.tmp");
    std::fs::write(&tmp, &blob).map_err(|e| AppError::io_at("写 wechat.key.tmp", &e))?;
    std::fs::rename(&tmp, &file).map_err(|e| AppError::io_at("替换 wechat.key", &e))?;
    Ok(())
}

pub fn has_key() -> bool {
    load_key()
        .map(|k| !k.key_hex.is_empty() && !k.data_dir.is_empty())
        .unwrap_or(false)
}

pub fn acquire_key(app: &AppHandle) -> AppResult<AcquireKeyResult> {
    let Some(acct) = island_wechat::discover_account() else {
        return Ok(AcquireKeyResult {
            ok: false,
            wxid: None,
            error: Some("no-account".into()),
        });
    };
    let recovered = match island_wechat::recover_key(&acct.data_dir) {
        Ok(r) => r,
        Err(e) => {
            // error 码前端按值判断
            eprintln!("[wechat] 密钥恢复失败: {e}");
            return Ok(AcquireKeyResult {
                ok: false,
                wxid: None,
                error: Some("recover-failed".into()),
            });
        }
    };
    let stored = StoredKey {
        key_hex: island_wechat::key_to_hex(&recovered.key).to_string(),
        wxid: acct.wxid.clone(),
        data_dir: acct.data_dir.to_string_lossy().into_owned(),
        recovered_at: now_unix_ms(),
    };
    store_key(&stored)?;
    // sync 幂等 须先停旧线程
    stop_worker();
    let settings = persist::get("settings")
        .map(AppSettings::from_value)
        .unwrap_or_default();
    sync(app, &settings);
    Ok(AcquireKeyResult {
        ok: true,
        wxid: Some(acct.wxid),
        error: None,
    })
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn stat_mtime_ms(f: &Path) -> u64 {
    std::fs::metadata(f)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 新消息先写 -wal
fn file_mtime_ms(f: &Path) -> u64 {
    let mut wal = f.as_os_str().to_os_string();
    wal.push("-wal");
    stat_mtime_ms(f).max(stat_mtime_ms(Path::new(&wal)))
}

fn message_db_files(data_dir: &Path) -> Vec<PathBuf> {
    let dir = data_dir.join("message");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return vec![];
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| island_wechat::discover::is_message_db_name(&n.to_string_lossy()))
                .unwrap_or(false)
        })
        .collect()
}

fn newest_message_mtime(data_dir: &Path) -> u64 {
    message_db_files(data_dir)
        .iter()
        .map(|f| file_mtime_ms(f))
        .max()
        .unwrap_or(0)
}

struct PollState {
    /// unix 秒
    watermark: i64,
    last_mtime_ms: u64,
    file_mtimes: HashMap<PathBuf, u64>,
    contacts: island_wechat::Contacts,
    contact_loaded_for: String,
    contact_db_mtime: u64,
    last_err_log: Instant,
}

impl PollState {
    fn new() -> Self {
        Self {
            watermark: 0,
            last_mtime_ms: 0,
            file_mtimes: HashMap::new(),
            contacts: Default::default(),
            contact_loaded_for: String::new(),
            contact_db_mtime: 0,
            last_err_log: Instant::now() - ERR_LOG_THROTTLE,
        }
    }

    fn log_err(&mut self, where_: &str, e: &island_wechat::WeChatError) {
        if self.last_err_log.elapsed() < ERR_LOG_THROTTLE {
            return;
        }
        self.last_err_log = Instant::now();
        eprintln!("[wechat] {where_} failed: {e}");
    }
}

struct Worker {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);
/// 换代防旧线程再发弹窗
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn stop_worker() {
    let mut guard = WORKER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(w) = guard.take() {
        w.stop.store(true, Ordering::SeqCst);
        // join 慢 另起线程等
        let _ = std::thread::Builder::new()
            .name("wechat-stop".into())
            .spawn(move || {
                let _ = w.handle.join();
            });
    }
}

pub fn sync(app: &AppHandle, settings: &AppSettings) {
    let want = settings.notifications.enabled && settings.notifications.wechat && has_key();
    let mut guard = WORKER.lock().unwrap_or_else(|e| e.into_inner());
    match (want, guard.is_some()) {
        (true, false) => {
            let stop = Arc::new(AtomicBool::new(false));
            let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
            let app2 = app.clone();
            let stop2 = stop.clone();
            let handle = std::thread::Builder::new()
                .name("wechat-poll".into())
                .spawn(move || run(app2, stop2, generation));
            match handle {
                Ok(h) => {
                    *guard = Some(Worker { stop, handle: h });
                    eprintln!("[wechat] started (generation {generation})");
                }
                Err(e) => eprintln!("[wechat] 启动轮询线程失败: {e}"),
            }
        }
        (false, true) => {
            drop(guard);
            stop_worker();
            eprintln!("[wechat] stopped");
        }
        _ => {}
    }
}

fn run(app: AppHandle, stop: Arc<AtomicBool>, generation: u64) {
    let Some(stored) = load_key() else {
        eprintln!("[wechat] 无密钥，轮询退出");
        return;
    };
    let Some(key) = island_wechat::key_from_hex(&stored.key_hex) else {
        eprintln!("[wechat] 密钥缓存 hex 解码失败或长度不对");
        return;
    };

    let data_dir = PathBuf::from(&stored.data_dir);
    let acct = island_wechat::discover_account();
    eprintln!(
        "[wechat] account={} dbFiles={}",
        acct.as_ref().map(|a| a.wxid.as_str()).unwrap_or("none"),
        message_db_files(&data_dir).len()
    );

    // tx 保活防断连
    let (tx, rx) = mpsc::channel::<()>();
    if acct.is_some() {
        let watch_dir = data_dir.join("message");
        let watch_stop = stop.clone();
        let watch_tx = tx.clone();
        let h = std::thread::Builder::new()
            .name("wechat-watch".into())
            .spawn(move || {
                while !watch_stop.load(Ordering::Relaxed) {
                    if !island_wechat::watch::wait_for_change(&watch_dir, &watch_stop) {
                        break;
                    }
                    let _ = watch_tx.send(());
                }
            });
        if let Err(e) = h {
            eprintln!("[wechat] watch 线程启动失败，仅兜底轮询: {e}");
        }
    }

    let mut st = PollState::new();
    while !stop.load(Ordering::Relaxed) {
        match rx.recv_timeout(POLL_INTERVAL) {
            Ok(()) => while rx.recv_timeout(WATCH_DEBOUNCE).is_ok() {},
            Err(mpsc::RecvTimeoutError::Timeout) => {} // 兜底轮询
            Err(mpsc::RecvTimeoutError::Disconnected) => break, // 不可达
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        poll_once(&app, &key, &stored, &data_dir, &mut st, generation);
    }
}

fn poll_once(
    app: &AppHandle,
    key: &[u8; 32],
    stored: &StoredKey,
    data_dir: &Path,
    st: &mut PollState,
    generation: u64,
) {
    let mtime = newest_message_mtime(data_dir);
    if mtime <= st.last_mtime_ms && st.watermark > 0 {
        return;
    }
    st.last_mtime_ms = mtime;

    let files = message_db_files(data_dir);

    // 首轮只建基线
    if st.watermark == 0 {
        st.watermark = now_unix();
        for f in &files {
            st.file_mtimes.insert(f.clone(), file_mtime_ms(f));
        }
        eprintln!(
            "[wechat] baseline watermark={} dbFiles={}",
            st.watermark,
            files.len()
        );
        return;
    }

    // 免打扰实时同步
    let contact_db = data_dir.join("contact").join("contact.db");
    let c_mtime = file_mtime_ms(&contact_db);
    if st.contact_loaded_for != stored.wxid || c_mtime > st.contact_db_mtime {
        match island_wechat::load_contacts(key, &contact_db) {
            Ok(c) => {
                st.contacts = c;
                st.contact_loaded_for = stored.wxid.clone();
                st.contact_db_mtime = c_mtime;
            }
            Err(e) => st.log_err("loadContacts", &e),
        }
    }

    let mut items: Vec<NotificationItem> = vec![];
    let mut max_time = st.watermark;
    for f in &files {
        // 只解有写入的库
        let mt = file_mtime_ms(f);
        if mt <= st.file_mtimes.get(f).copied().unwrap_or(0) {
            continue;
        }
        st.file_mtimes.insert(f.clone(), mt);
        let msgs = match island_wechat::read_new_messages(key, f, st.watermark, &stored.wxid) {
            Ok(m) => m,
            Err(e) => {
                st.log_err("readNewMessages", &e);
                continue;
            }
        };
        for m in msgs {
            if m.create_time > max_time {
                max_time = m.create_time;
            }
            // 表名 Msg_+md5
            let conv = st.contacts.by_hash.get(&m.table[4..].to_lowercase());
            if conv.map(|c| c.muted).unwrap_or(false) {
                continue;
            }
            let sender = st
                .contacts
                .by_username
                .get(&m.sender_username)
                .map(|c| c.name.clone())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| {
                    if m.sender_username.is_empty() {
                        "微信".into()
                    } else {
                        m.sender_username.clone()
                    }
                });
            let is_group = conv.map(|c| c.is_group).unwrap_or(false);
            let title = if is_group {
                conv.map(|c| c.name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "群聊".into())
            } else {
                conv.map(|c| c.name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| sender.clone())
            };
            let body = if is_group {
                format!("{sender}: {}", m.content)
            } else {
                m.content.clone()
            };
            items.push(NotificationItem {
                id: m.local_id,
                aumid: "wechat".into(),
                app: "微信".into(),
                icon: String::new(),
                image: conv.map(|c| c.avatar.clone()).unwrap_or_default(),
                title,
                body,
                launch: String::new(),
                atype: String::new(),
                arrival: m.create_time * 1000,
            });
        }
    }
    // 各表已读空 水位可推
    st.watermark = max_time;
    if items.is_empty() {
        return;
    }
    eprintln!("[wechat] poll@{}: {} msg(s)", now_unix(), items.len());

    // 前台是微信则不弹
    let fg = island_wechat::foreground_process_stem().unwrap_or_default();
    if fg == "weixin" || fg == "wechat" {
        return;
    }
    if generation == GENERATION.load(Ordering::SeqCst) {
        let _ = app.emit("notify:incoming", &items);
    }
}
