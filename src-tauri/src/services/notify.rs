use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use island_core::{AppSettings, NotificationItem};

use crate::infra::persist;

// 无 UserNotificationListener 时的退化节奏，同 winbridge NotifyService
const POLL_INTERVAL: Duration = Duration::from_millis(1500);
const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
const IMAGE_CACHE_CAP: usize = 200;
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

/// 通知图片/头像 → data URL。http(s) 经网络拉取；本地文件读进来按魔数认格式，
/// 认不出是受支持图片的一律拒绝（记日志）返回 None，渲染层回退首字母块。
/// 不做扩展名/目录白名单：toast 里的路径由来源应用自己写——QQ NT 头像没有扩展名、
/// Edge 通知资源是 .tmp、缓存目录还可能不在系统盘，白名单只会误伤正常头像。
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
    // 包资源 URI（ms-appx/ms-appdata/ms-resource）解不到真实文件，静默回退（同 Electron 版）
    let lower = path_text.to_ascii_lowercase();
    if lower.starts_with("ms-appx:") || lower.starts_with("ms-appdata:") || lower.starts_with("ms-resource:") {
        return None;
    }
    let path = Path::new(path_text);
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() as usize > MAX_IMAGE_BYTES => {
            eprintln!("[notify] 拒绝通知图片（超过 2MB）: {}", path.display());
            return None;
        }
        // 通知资源是瞬态文件（Edge 的 Notification Resources 发完就删），
        // 前端来取时已不在属正常路径，不刷日志
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return None;
        }
        Err(e) => {
            eprintln!("[notify] 通知图片不可读({}): {e}", path.display());
            return None;
        }
        _ => {}
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return None;
        }
        Err(e) => {
            eprintln!("[notify] 读取通知图片失败({}): {e}", path.display());
            return None;
        }
    };
    let Some(mime) = sniff_image_mime(&bytes) else {
        eprintln!("[notify] 拒绝通知图片（内容不是受支持的图片）: {}", path.display());
        return None;
    };
    Some(format!("data:{mime};base64,{}", base64_encode(&bytes)))
}

/// 按魔数识别图片格式，认不出返回 None
fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(b"BM") {
        return Some("image/bmp");
    }
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some("image/x-icon");
    }
    None
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
    fn sniff_image_mime_recognizes_common_formats() {
        assert_eq!(sniff_image_mime(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(sniff_image_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image_mime(b"GIF89a...."), Some("image/gif"));
        assert_eq!(sniff_image_mime(b"RIFF\x00\x00\x00\x00WEBPvp8"), Some("image/webp"));
        assert_eq!(sniff_image_mime(b"BMxxxx"), Some("image/bmp"));
        assert_eq!(sniff_image_mime(&[0x00, 0x00, 0x01, 0x00]), Some("image/x-icon"));
        assert_eq!(sniff_image_mime(b"MZ\x90\x00"), None, "可执行文件必须认不出");
        assert_eq!(sniff_image_mime(b""), None, "空内容必须认不出");
    }

    #[test]
    fn read_local_rejects_non_image_content() {
        assert_eq!(
            read_local(r"C:\Windows\System32\kernel32.dll"),
            None,
            "内容不是图片的文件必须拒绝"
        );
    }

    #[test]
    fn read_local_silently_skips_package_resource_uris() {
        assert_eq!(read_local("ms-appdata:///local/ToastCollectionIcons/x.png"), None);
        assert_eq!(read_local("ms-appx:///Assets/icon.png"), None);
        assert_eq!(read_local("ms-resource:app/Resources/icon"), None);
    }

    #[test]
    fn read_local_accepts_extensionless_and_tmp_images() {
        // QQ NT 头像缓存没有扩展名，Edge 通知资源是 .tmp：按内容认，都得能读
        let dir = std::env::temp_dir().join("top-island-test-noext");
        std::fs::create_dir_all(&dir).unwrap();
        let avatar = dir.join("s_b643435b134d5c1cc2495e6076174f59");
        std::fs::write(&avatar, [0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3]).unwrap();
        let out = read_local(&avatar.to_string_lossy());
        assert!(
            out.as_deref().is_some_and(|s| s.starts_with("data:image/jpeg;base64,")),
            "无扩展名的 jpeg 头像必须按内容识别，实际: {out:?}"
        );
        let tmp = dir.join("e8143a85.tmp");
        std::fs::write(&tmp, b"\x89PNG\r\n\x1a\n....").unwrap();
        let out = read_local(&tmp.to_string_lossy());
        assert!(
            out.as_deref().is_some_and(|s| s.starts_with("data:image/png;base64,")),
            ".tmp 的 png 通知资源必须按内容识别，实际: {out:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_local_accepts_image_outside_system_dirs() {
        // 头像缓存目录由应用自定（QQ 常在数据盘），不得再做目录白名单
        let exe = std::env::current_exe().unwrap();
        let file = exe.with_file_name("top-island-test-anywhere.png");
        std::fs::write(&file, b"\x89PNG\r\n\x1a\n....").unwrap();
        let out = read_local(&file.to_string_lossy());
        assert!(
            out.as_deref().is_some_and(|s| s.starts_with("data:image/png;base64,")),
            "任意目录下的合法图片必须可读，实际: {out:?}"
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
