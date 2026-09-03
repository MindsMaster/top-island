use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use island_core::{AppSettings, NotificationItem};

use crate::infra::persist;

// 无 UserNotificationListener 时的退化节奏，同 winbridge NotifyService
const POLL_INTERVAL: Duration = Duration::from_millis(1500);
const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
const IMAGE_CACHE_CAP: usize = 200;
const IMAGE_EXTS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
// 沿用 Electron 版 store.json 的键：被改过横幅的应用 → 原值（-1 = 原本没有该值，还原=删除）
const SUPPRESS_KEY: &str = "notifySuppressedApps";

#[derive(Debug, Default)]
struct ServiceState {
    /// 递增即作废旧监视线程（停用/重启都靠它让线程自行退出）
    generation: u64,
    watching: bool,
    suppress: bool,
}

static STATE: Mutex<ServiceState> = Mutex::new(ServiceState {
    generation: 0,
    watching: false,
    suppress: false,
});

fn lock_state() -> MutexGuard<'static, ServiceState> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// 按设置幂等启停消息托管 + 横幅接管（启动与 settings:changed 时调用）
pub fn sync(app: &AppHandle, settings: &AppSettings) {
    let enabled = settings.notifications.enabled;
    // 整体关托管时也视为关接管：右下角横幅是系统弹窗，不该由我们替用户压着
    let suppress = enabled && settings.notifications.suppress_banner;
    let (spawn, generation, need_restore) = {
        let mut st = lock_state();
        let mut spawn = false;
        if enabled != st.watching {
            st.watching = enabled;
            st.generation += 1;
            spawn = enabled;
        }
        let need_restore = st.suppress && !suppress;
        st.suppress = suppress;
        (spawn, st.generation, need_restore)
    };
    if need_restore {
        restore_all_banners();
    }
    if spawn {
        let app = app.clone();
        let spawned = std::thread::Builder::new()
            .name("notify-watch".into())
            .spawn(move || watch_loop(app, generation));
        if let Err(e) = spawned {
            eprintln!("[notify] 启动通知监视线程失败: {e}");
            lock_state().watching = false;
        }
    }
}

fn watch_loop(app: AppHandle, generation: u64) {
    // 开启托管前堆积的历史通知不弹：基线取当前最大 Id
    let mut watermark = match island_windows::wpn::query_max_id() {
        Ok(max) => max,
        Err(e) => {
            eprintln!("[notify] 读取通知水位基线失败: {e}");
            0
        }
    };
    loop {
        // 分段 sleep：停用时 100ms 内退出，不用等满一个轮询周期
        let mut stopped = false;
        for _ in 0..(POLL_INTERVAL.as_millis() / 100) {
            std::thread::sleep(Duration::from_millis(100));
            let st = lock_state();
            if !st.watching || st.generation != generation {
                stopped = true;
                break;
            }
        }
        if stopped {
            return;
        }
        let suppress = lock_state().suppress;
        match island_windows::wpn::toasts_since(watermark) {
            Ok((rows, raw_max)) => {
                if raw_max > watermark {
                    watermark = raw_max;
                }
                let items: Vec<NotificationItem> =
                    rows.into_iter().filter_map(row_to_item).collect();
                if items.is_empty() {
                    continue;
                }
                if suppress {
                    for item in &items {
                        suppress_banner_for(&item.aumid);
                    }
                }
                if let Err(e) = app.emit("notify:incoming", &items) {
                    eprintln!("[notify] 推送新通知失败: {e}");
                }
            }
            // 拷库撞上系统写入属常态（WpnDatabase.cs），记日志后下轮重试
            Err(e) => eprintln!("[notify] 扫描通知库失败: {e}"),
        }
    }
}

fn row_to_item(row: island_windows::wpn::WpnToast) -> Option<NotificationItem> {
    let payload = island_core::parse_toast_payload(&row.payload);
    // 空壳 toast（进度条/更新器）不上岛；水位已在 raw_max 推进，不会重扫
    if payload.title.is_empty() && payload.body.is_empty() {
        return None;
    }
    let app = if row.display_name.is_empty() { row.aumid.clone() } else { row.display_name };
    Some(NotificationItem {
        id: row.id,
        aumid: row.aumid,
        app,
        icon: row.icon_uri,
        image: payload.image,
        title: payload.title,
        body: payload.body,
        launch: payload.launch,
        atype: payload.atype,
        arrival: row.arrival_ms,
    })
}

fn suppressed_map() -> HashMap<String, i32> {
    let Some(value) = persist::get(SUPPRESS_KEY) else {
        return HashMap::new();
    };
    match serde_json::from_value(value) {
        Ok(map) => map,
        Err(e) => {
            // 解析失败按空表继续，但必须留痕：还原记录丢了横幅就永远关着
            eprintln!("[notify] 横幅原值记录损坏，按空表处理: {e}");
            HashMap::new()
        }
    }
}

/// 关掉某应用的右下角横幅（仍进通知中心）。原值记进 store 以便还原；已处理过的跳过。
fn suppress_banner_for(aumid: &str) {
    if aumid.is_empty() {
        return;
    }
    let mut map = suppressed_map();
    if map.contains_key(aumid) {
        return;
    }
    let prior = match island_windows::banner::get_banner(aumid) {
        Ok(v) => v.unwrap_or(-1),
        Err(e) => {
            // 读不到原值就不改，免得以后还原不了
            eprintln!("[notify] 读取横幅原值失败({aumid}): {e}");
            return;
        }
    };
    if let Err(e) = island_windows::banner::set_banner(aumid, Some(0)) {
        eprintln!("[notify] 关闭横幅失败({aumid}): {e}");
        return;
    }
    map.insert(aumid.to_string(), prior);
    if let Err(e) = persist::set(SUPPRESS_KEY, serde_json::json!(map)) {
        eprintln!("[notify] 记录横幅原值失败: {e}");
    }
}

fn restore_all_banners() {
    let map = suppressed_map();
    for (aumid, prior) in &map {
        let value = if *prior >= 0 { Some(*prior) } else { None };
        if let Err(e) = island_windows::banner::set_banner(aumid, value) {
            eprintln!("[notify] 还原横幅失败({aumid}): {e}");
        }
    }
    if !map.is_empty() {
        if let Err(e) = persist::set(SUPPRESS_KEY, serde_json::json!({})) {
            eprintln!("[notify] 清空横幅记录失败: {e}");
        }
    }
}

// ---- 通知图片（白名单 + LRU 缓存） ----

/// 图片缓存：容量 200 的 LRU 逐出（Electron 版是超 200 全量清空，会把正在用的头像也冲掉）。
/// 命中与否都缓存——失败结果同样占坑，避免每条消息重复拉取坏图。
#[derive(Debug, Default)]
struct ImageLru {
    map: HashMap<String, Option<String>>,
    /// 队首最久未用
    order: VecDeque<String>,
}

impl ImageLru {
    fn get(&mut self, key: &str) -> Option<Option<String>> {
        if !self.map.contains_key(key) {
            return None;
        }
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            if let Some(k) = self.order.remove(pos) {
                self.order.push_back(k);
            }
        }
        self.map.get(key).cloned()
    }

    fn insert(&mut self, key: String, value: Option<String>) {
        if self.map.contains_key(&key) {
            self.map.insert(key.clone(), value);
            if let Some(pos) = self.order.iter().position(|k| *k == key) {
                if let Some(k) = self.order.remove(pos) {
                    self.order.push_back(k);
                }
            }
            return;
        }
        if self.map.len() >= IMAGE_CACHE_CAP {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, value);
    }
}

static IMAGE_CACHE: std::sync::LazyLock<Mutex<ImageLru>> =
    std::sync::LazyLock::new(|| Mutex::new(ImageLru::default()));

fn lock_cache() -> MutexGuard<'static, ImageLru> {
    IMAGE_CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

/// 通知图片/头像 → data URL。白名单：http(s) 经网络拉取；本地只允许
/// %LOCALAPPDATA%\Packages\ 与 %TEMP% 下的图片文件。其余一律拒绝（记日志）返回 None，
/// 渲染层回退首字母块。
pub fn notify_image(src: &str) -> Option<String> {
    let src = src.trim();
    if src.is_empty() {
        return None;
    }
    if let Some(hit) = lock_cache().get(src) {
        return hit;
    }
    let out = load_image(src);
    lock_cache().insert(src.to_string(), out.clone());
    out
}

fn load_image(src: &str) -> Option<String> {
    let lower = src.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return fetch_http(src);
    }
    read_local(&strip_file_scheme(src))
}

fn fetch_http(url: &str) -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .into();
    let mut resp = match agent.get(url).call() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[notify] 拉取通知图片失败({url}): {e}");
            return None;
        }
    };
    let mime = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(';').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("image/png")
        .to_string();
    // 上限 +1 字节：借此区分「刚好 2MB」和「超限」
    let limit = (MAX_IMAGE_BYTES + 1) as u64;
    match resp.body_mut().with_config().limit(limit).read_to_vec() {
        Ok(bytes) if !bytes.is_empty() && bytes.len() <= MAX_IMAGE_BYTES => {
            Some(format!("data:{mime};base64,{}", base64_encode(&bytes)))
        }
        Ok(_) => {
            eprintln!("[notify] 拒绝通知图片（为空或超过 2MB）: {url}");
            None
        }
        Err(e) => {
            eprintln!("[notify] 读取通知图片响应失败({url}): {e}");
            None
        }
    }
}

fn read_local(path_text: &str) -> Option<String> {
    let ext = Path::new(path_text)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    let Some(ext) = ext.filter(|e| IMAGE_EXTS.contains(&e.as_str())) else {
        eprintln!("[notify] 拒绝通知图片（扩展名不在白名单）: {path_text}");
        return None;
    };
    // 规范化后才能比前缀：toast 里的路径可能带 .. 或符号链接
    let path = match std::fs::canonicalize(path_text) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[notify] 通知图片路径不可读({path_text}): {e}");
            return None;
        }
    };
    if !under_allowed_root(&path) {
        eprintln!("[notify] 拒绝通知图片（路径不在白名单目录下）: {}", path.display());
        return None;
    }
    match std::fs::metadata(&path) {
        Ok(meta) if meta.len() as usize > MAX_IMAGE_BYTES => {
            eprintln!("[notify] 拒绝通知图片（超过 2MB）: {}", path.display());
            return None;
        }
        Err(e) => {
            eprintln!("[notify] 读取通知图片信息失败({}): {e}", path.display());
            return None;
        }
        _ => {}
    }
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[notify] 读取通知图片失败({}): {e}", path.display());
            return None;
        }
    };
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => unreachable!("扩展名已过白名单"),
    };
    Some(format!("data:{mime};base64,{}", base64_encode(&bytes)))
}

/// 只允许 %LOCALAPPDATA%\Packages\ 与 %TEMP% 之下。两侧都做 canonicalize
/// （TEMP 可能是 8.3 短名），统一小写比较（Windows 路径大小写不敏感）。
fn under_allowed_root(path: &Path) -> bool {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        roots.push(Path::new(&local).join("Packages"));
    }
    roots.push(std::env::temp_dir());
    let text = path.to_string_lossy().to_lowercase();
    let text = text.trim_end_matches('\\');
    roots.iter().any(|root| {
        let root = std::fs::canonicalize(root).unwrap_or_default();
        let root_text = root.to_string_lossy().to_lowercase();
        let root_text = root_text.trim_end_matches('\\');
        !root_text.is_empty()
            && (text == root_text || text.starts_with(&format!("{root_text}\\")))
    })
}

/// file:// 前缀剥离（toast 图片多为裸路径，file:// 只是防御性兼容）
fn strip_file_scheme(src: &str) -> String {
    if !src.to_ascii_lowercase().starts_with("file://") {
        return src.to_string();
    }
    let rest = &src[7..];
    // file:///C:/... → C:/...
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    percent_decode(&rest.replace('/', "\\"))
}

fn percent_decode(text: &str) -> String {
    if !text.contains('%') {
        return text.to_string();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encode_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==", "单字节尾部应补两个 =");
        assert_eq!(base64_encode(b"fo"), "Zm8=", "双字节尾部应补一个 =");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy", "RFC 4648 标准向量");
    }

    #[test]
    fn image_lru_evicts_oldest_when_full() {
        let mut lru = ImageLru::default();
        for i in 0..IMAGE_CACHE_CAP {
            lru.insert(format!("k{i}"), Some(format!("v{i}")));
        }
        lru.insert("overflow".into(), Some("x".into()));
        assert_eq!(lru.get("k0"), None, "最早插入的 k0 应被逐出");
        assert_eq!(
            lru.get("overflow"),
            Some(Some("x".to_string())),
            "新插入的条目必须保留"
        );
        assert_eq!(lru.map.len(), IMAGE_CACHE_CAP, "缓存容量不应超过上限");
    }

    #[test]
    fn image_lru_get_refreshes_recency() {
        let mut lru = ImageLru::default();
        for i in 0..IMAGE_CACHE_CAP {
            lru.insert(format!("k{i}"), Some(format!("v{i}")));
        }
        // 摸一下 k0 让它变成最近使用
        let _ = lru.get("k0");
        lru.insert("overflow".into(), Some("x".into()));
        assert_eq!(
            lru.get("k0"),
            Some(Some("v0".to_string())),
            "刚访问过的 k0 不应被逐出，逐出的应是更久没用的 k1"
        );
        assert_eq!(lru.get("k1"), None, "k1 才是当前最久未用的条目");
    }

    #[test]
    fn image_lru_caches_negative_results() {
        let mut lru = ImageLru::default();
        lru.insert("bad".into(), None);
        assert_eq!(lru.get("bad"), Some(None), "失败结果也要能命中（避免重复拉坏图）");
        assert_eq!(lru.get("missing"), None, "未缓存的键返回未命中");
    }

    #[test]
    fn strip_file_scheme_handles_plain_and_file_urls() {
        assert_eq!(strip_file_scheme(r"C:\Temp\a.png"), r"C:\Temp\a.png");
        assert_eq!(strip_file_scheme("file:///C:/Temp/a.png"), r"C:\Temp\a.png");
        assert_eq!(
            strip_file_scheme("file:///C:/Temp/a%20b.png"),
            r"C:\Temp\a b.png",
            "file URL 里的百分号编码应解码"
        );
    }

    #[test]
    fn read_local_rejects_non_image_extensions() {
        assert_eq!(
            read_local(r"C:\Windows\System32\kernel32.dll"),
            None,
            "非图片扩展名必须拒绝"
        );
    }

    #[test]
    fn under_allowed_root_rejects_paths_outside_whitelist() {
        // canonicalize 后的路径带 \\?\ 前缀；白名单根同样在 Users 目录下，C:\Windows 必不在其中
        let outside = Path::new(r"\\?\C:\Windows\System32\evil.png");
        assert!(!under_allowed_root(outside), "白名单目录之外的路径必须拒绝");
    }

    #[test]
    fn read_local_rejects_existing_image_outside_whitelist_dirs() {
        // 测试可执行文件在 target\ 下，必不在 Packages/TEMP 白名单里
        let exe = std::env::current_exe().unwrap();
        let file = exe.with_file_name("top-island-test-outside.png");
        let png: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        std::fs::write(&file, png).unwrap();
        assert_eq!(
            read_local(&file.to_string_lossy()),
            None,
            "真实存在但在白名单目录之外的图片必须拒绝"
        );
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn read_local_accepts_image_under_temp() {
        let dir = std::env::temp_dir().join("top-island-test-inside");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("avatar.png");
        // 1x1 PNG
        let png: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        std::fs::write(&file, png).unwrap();
        let out = read_local(&file.to_string_lossy());
        assert!(
            out.as_deref().is_some_and(|s| s.starts_with("data:image/png;base64,")),
            "TEMP 下的 png 应被接受并编码为 data URL，实际: {out:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn percent_decode_passes_through_plain_text() {
        assert_eq!(percent_decode("no-encoding"), "no-encoding");
        assert_eq!(percent_decode("100%"), "100%", "不完整编码应原样保留");
    }
}
