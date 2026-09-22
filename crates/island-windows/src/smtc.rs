use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use windows::core::RuntimeType;
use windows::Foundation::{
    AsyncStatus, EventRegistrationToken, IAsyncOperation, TypedEventHandler,
};
use windows::Media::Control::{
    CurrentSessionChangedEventArgs, GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    MediaPropertiesChangedEventArgs, PlaybackInfoChangedEventArgs, SessionsChangedEventArgs,
    TimelinePropertiesChangedEventArgs,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, SendMessageTimeoutW,
    SMTO_ABORTIFHUNG, WM_APPCOMMAND,
};

use crate::coreaudio::com_init_mta;
use crate::error::{Result, WinError};

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmtcSessionInfo {
    pub title: String,
    pub artist: String,
    /// SourceAppUserModelId win32 为 exe 名 UWP 为包族名
    pub app: String,
    pub playing: bool,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub seek_supported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaAction {
    Play,
    Pause,
    Next,
    Prev,
}

/// windows 0.58 无 IAsyncOperation 的 Future 只能轮询
/// 个别会话异步永不完成 须超时兜底
const ASYNC_TIMEOUT: Duration = Duration::from_secs(3);

/// E_ABORT
fn timeout_err() -> windows::core::Error {
    windows::core::Error::from(windows::core::HRESULT(0x8000_4004u32 as i32))
}

fn block_on<T: RuntimeType>(op: &IAsyncOperation<T>) -> windows::core::Result<T> {
    let deadline = Instant::now() + ASYNC_TIMEOUT;
    while op.Status()? == AsyncStatus::Started {
        if Instant::now() >= deadline {
            return Err(timeout_err());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    op.GetResults()
}

/// LoadAsync 返回专属类型 走不了 block_on
fn block_on_load(
    op: &windows::Storage::Streams::DataReaderLoadOperation,
) -> windows::core::Result<u32> {
    let deadline = Instant::now() + ASYNC_TIMEOUT;
    while op.Status()? == AsyncStatus::Started {
        if Instant::now() >= deadline {
            return Err(timeout_err());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    op.GetResults()
}

/// 过滤自身闹铃等产生的会话 否则岛会暂停自己
fn is_self_session(app_id: &str) -> bool {
    if app_id.is_empty() {
        return false;
    }
    let id = app_id.to_lowercase();
    id.contains("topisland")
        || id.contains("top-island")
        || id.contains("island-app")
        || id.contains("electron.exe")
}

fn now_1601_100ns() -> i64 {
    let unix_100ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_nanos() / 100) as i64)
        .unwrap_or(0);
    unix_100ns + 116_444_736_000_000_000
}

/// 非线程安全 调用方加锁
#[derive(Debug, Default)]
pub struct SmtcClient {
    manager: Option<SessionManager>,
    manager_time: Option<Instant>,
    /// 控制目标跟随上次 query 展示的会话
    last_app: String,
}

impl SmtcClient {
    pub const fn new() -> Self {
        Self {
            manager: None,
            manager_time: None,
            last_app: String::new(),
        }
    }

    /// 新建 manager 时间线延迟填充 故常驻并定期重建
    fn manager(&mut self) -> Result<&SessionManager> {
        let stale = self
            .manager_time
            .is_none_or(|t| t.elapsed() > Duration::from_secs(30));
        if self.manager.is_none() || stale {
            com_init_mta();
            let mgr = block_on(
                &SessionManager::RequestAsync().map_err(|e| WinError::api("SMTC 管理器", e))?,
            )
            .map_err(|e| WinError::api("SMTC 管理器", e))?;
            self.manager = Some(mgr);
            self.manager_time = Some(Instant::now());
        }
        Ok(self.manager.as_ref().expect("manager 刚写入"))
    }

    fn target_session(&mut self, prefer_app: &str) -> Option<Session> {
        let manager = match self.manager() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[smtc] 会话管理器不可用: {e}");
                return None;
            }
        };

        if !prefer_app.is_empty() && !is_self_session(prefer_app) {
            match manager.GetSessions() {
                Ok(sessions) => {
                    for i in 0..sessions.Size().unwrap_or(0) {
                        if let Ok(s) = sessions.GetAt(i) {
                            if s.SourceAppUserModelId().map(|a| a.to_string_lossy())
                                == Ok(prefer_app.to_string())
                            {
                                return Some(s);
                            }
                        }
                    }
                }
                Err(e) => eprintln!("[smtc] 枚举媒体会话失败: {e}"),
            }
        }

        if let Ok(current) = manager.GetCurrentSession() {
            let cur_app = current
                .SourceAppUserModelId()
                .map(|a| a.to_string_lossy())
                .unwrap_or_default();
            if !is_self_session(&cur_app) {
                return Some(current);
            }
        }

        let mut first: Option<Session> = None;
        match manager.GetSessions() {
            Ok(sessions) => {
                for i in 0..sessions.Size().unwrap_or(0) {
                    if let Ok(s) = sessions.GetAt(i) {
                        let app = s
                            .SourceAppUserModelId()
                            .map(|a| a.to_string_lossy())
                            .unwrap_or_default();
                        if is_self_session(&app) {
                            continue;
                        }
                        if first.is_none() {
                            first = Some(s.clone());
                        }
                        let playing = s
                            .GetPlaybackInfo()
                            .and_then(|i| i.PlaybackStatus())
                            .map(|st| st == PlaybackStatus::Playing)
                            .unwrap_or(false);
                        if playing {
                            return Some(s);
                        }
                    }
                }
            }
            Err(e) => eprintln!("[smtc] 枚举媒体会话失败: {e}"),
        }
        first
    }

    pub fn query(&mut self) -> Option<SmtcSessionInfo> {
        match self.query_inner() {
            Ok(q) => q,
            Err(e) => {
                eprintln!("[smtc] 查询当前会话失败: {e}");
                None
            }
        }
    }

    fn query_inner(&mut self) -> Result<Option<SmtcSessionInfo>> {
        let Some(session) = self.target_session("") else {
            return Ok(None);
        };
        let app = session
            .SourceAppUserModelId()
            .map_err(|e| WinError::api("SMTC 来源应用", e))?
            .to_string_lossy();
        self.last_app = app.clone();

        let props = block_on(
            &session
                .TryGetMediaPropertiesAsync()
                .map_err(|e| WinError::api("SMTC 曲目信息", e))?,
        )
        .map_err(|e| WinError::api("SMTC 曲目信息", e))?;
        let playback = session
            .GetPlaybackInfo()
            .map_err(|e| WinError::api("SMTC 播放状态", e))?;
        let playing = playback
            .PlaybackStatus()
            .map_err(|e| WinError::api("SMTC 播放状态", e))?
            == PlaybackStatus::Playing;

        let mut position_ms = 0i64;
        let mut duration_ms = 0i64;
        match session.GetTimelineProperties() {
            Ok(timeline) => {
                let end_ms = timeline.EndTime().map(|t| t.Duration / 10_000).unwrap_or(0);
                if end_ms > 0 {
                    position_ms = timeline
                        .Position()
                        .map(|t| t.Duration / 10_000)
                        .unwrap_or(0);
                    duration_ms = end_ms;
                    if playing {
                        let elapsed = timeline
                            .LastUpdatedTime()
                            .map(|t| (now_1601_100ns() - t.UniversalTime) / 10_000)
                            .unwrap_or(i64::MAX);
                        // 播放中 Position 冻结 按 LastUpdatedTime 墙钟补偿
                        if (0..30_000).contains(&elapsed) {
                            position_ms = (position_ms + elapsed).min(duration_ms);
                        }
                    }
                }
            }
            Err(e) => eprintln!("[smtc] 读取时间轴失败（按无时间轴处理）: {e}"),
        }

        let seek_supported = match playback.Controls() {
            Ok(c) => c.IsPlaybackPositionEnabled().unwrap_or(false),
            Err(e) => {
                eprintln!("[smtc] 读取控制能力失败（按不支持 seek 处理）: {e}");
                false
            }
        };

        Ok(Some(SmtcSessionInfo {
            title: props
                .Title()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default(),
            artist: props
                .Artist()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default(),
            app,
            playing,
            position_ms,
            duration_ms,
            seek_supported,
        }))
    }

    pub fn thumbnail(&mut self) -> Option<Vec<u8>> {
        match self.thumbnail_inner() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[smtc] 读取封面失败: {e}");
                None
            }
        }
    }

    fn thumbnail_inner(&mut self) -> Result<Option<Vec<u8>>> {
        let prefer = self.last_app.clone();
        let Some(session) = self.target_session(&prefer) else {
            return Ok(None);
        };
        let props = block_on(
            &session
                .TryGetMediaPropertiesAsync()
                .map_err(|e| WinError::api("SMTC 曲目信息", e))?,
        )
        .map_err(|e| WinError::api("SMTC 曲目信息", e))?;
        let Ok(thumb_ref) = props.Thumbnail() else {
            return Ok(None);
        };
        let stream = block_on(
            &thumb_ref
                .OpenReadAsync()
                .map_err(|e| WinError::api("SMTC 封面流", e))?,
        )
        .map_err(|e| WinError::api("SMTC 封面流", e))?;

        let reader =
            DataReader::CreateDataReader(&stream).map_err(|e| WinError::api("SMTC 封面流", e))?;
        let mut out = Vec::new();
        loop {
            let loaded = block_on_load(
                &reader
                    .LoadAsync(64 * 1024)
                    .map_err(|e| WinError::api("SMTC 封面读取", e))?,
            )
            .map_err(|e| WinError::api("SMTC 封面读取", e))?;
            if loaded == 0 {
                break;
            }
            let mut chunk = vec![0u8; loaded as usize];
            reader
                .ReadBytes(&mut chunk)
                .map_err(|e| WinError::api("SMTC 封面读取", e))?;
            out.extend_from_slice(&chunk);
            // 异常来源防内存拖垮
            if out.len() > 16 * 1024 * 1024 {
                return Err(WinError::api("SMTC 封面读取", "封面超过 16MB，放弃"));
            }
        }
        if out.is_empty() {
            return Ok(None);
        }
        Ok(Some(out))
    }

    /// 位置参数单位 100ns
    pub fn seek(&mut self, position_ms: i64) -> Result<()> {
        let prefer = self.last_app.clone();
        let Some(session) = self.target_session(&prefer) else {
            return Err(WinError::api("SMTC seek", "没有媒体会话"));
        };
        block_on(
            &session
                .TryChangePlaybackPositionAsync(position_ms.max(0) * 10_000)
                .map_err(|e| WinError::api("SMTC seek", e))?,
        )
        .map_err(|e| WinError::api("SMTC seek", e))?;
        Ok(())
    }

    /// 逐级回退 会话接口 定向 APPCOMMAND 全局媒体键
    /// 网易云 TryPauseAsync 会假成功 上层按播放态纠偏
    pub fn control(&mut self, action: MediaAction) {
        let mut ok = false;
        let mut app = String::new();
        let prefer = self.last_app.clone();
        if let Some(session) = self.target_session(&prefer) {
            app = session
                .SourceAppUserModelId()
                .map(|a| a.to_string_lossy())
                .unwrap_or_default();
            let op = match action {
                MediaAction::Play => session.TryPlayAsync(),
                MediaAction::Pause => session.TryPauseAsync(),
                MediaAction::Next => session.TrySkipNextAsync(),
                MediaAction::Prev => session.TrySkipPreviousAsync(),
            };
            match op.and_then(|op| block_on(&op)) {
                Ok(v) => ok = v,
                Err(e) => eprintln!("[smtc] 会话级控制 {action:?} 失败，走兜底链路: {e}"),
            }
        }
        if ok {
            return;
        }
        if app.is_empty() {
            app = prefer;
        }
        // APPCOMMAND_MEDIA_PLAY PAUSE NEXTTRACK PREVIOUSTRACK
        let cmd = match action {
            MediaAction::Play => 46,
            MediaAction::Pause => 47,
            MediaAction::Next => 11,
            MediaAction::Prev => 12,
        };
        if !app.is_empty() && send_app_command(&app, cmd) {
            return;
        }
        let vk = match action {
            MediaAction::Play | MediaAction::Pause => 0xB3, // VK_MEDIA_PLAY_PAUSE
            MediaAction::Next => 0xB0,                      // VK_MEDIA_NEXT_TRACK
            MediaAction::Prev => 0xB1,                      // VK_MEDIA_PREV_TRACK
        };
        press_media_key(vk);
    }
}

/// keybd_event 已废弃 走 SendInput
fn press_media_key(vk: u16) {
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [
        key(KEYEVENTF_EXTENDEDKEY),
        key(KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP),
    ];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        eprintln!("[smtc] 全局媒体键发送不完整（{sent}/{}）", inputs.len());
    }
}

/// APPCOMMAND 在 lParam 高 16 位 超时防目标挂起拖死本进程
fn send_app_command_hwnd(hwnd: HWND, cmd: i32) -> bool {
    let mut result = 0usize;
    let delivered = unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_APPCOMMAND,
            WPARAM(hwnd.0 as usize),
            LPARAM(((cmd as i64) << 16) as isize),
            SMTO_ABORTIFHUNG,
            2000,
            Some(&mut result),
        )
    };
    delivered.0 != 0
}

struct AppCommandSearch {
    target_exe: String,
    cmd: i32,
    delivered: bool,
}

unsafe extern "system" fn enum_app_window_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        let ctx = &mut *(lparam.0 as *mut AppCommandSearch);
        if ctx.delivered || GetWindowTextLengthW(hwnd) <= 0 {
            return BOOL::from(!ctx.delivered);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid != 0 && process_exe_matches(pid, &ctx.target_exe) {
            ctx.delivered = send_app_command_hwnd(hwnd, ctx.cmd);
        }
        BOOL::from(!ctx.delivered)
    }
}

/// target 不带 .exe
fn process_exe_matches(pid: u32, target: &str) -> bool {
    let matches = (|| -> Option<bool> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let path = unsafe {
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
        }
        .ok();
        unsafe {
            let _ = CloseHandle(handle);
        }
        path?;
        let full = String::from_utf16_lossy(&buf[..len as usize]);
        let name = full.rsplit(['\\', '/']).next()?.to_lowercase();
        Some(name == format!("{target}.exe"))
    })();
    matches.unwrap_or(false)
}

fn send_app_command(app_id: &str, cmd: i32) -> bool {
    if !app_id.to_ascii_lowercase().ends_with(".exe") {
        return false;
    }
    let mut ctx = AppCommandSearch {
        target_exe: app_id[..app_id.len() - 4].to_lowercase(),
        cmd,
        delivered: false,
    };
    unsafe {
        let _ = EnumWindows(
            Some(enum_app_window_proc),
            LPARAM(&mut ctx as *mut AppCommandSearch as isize),
        );
    }
    ctx.delivered
}

struct Watcher {
    manager: SessionManager,
    created: Instant,
    mgr_tokens: Option<(EventRegistrationToken, EventRegistrationToken)>,
    session_hooks: Vec<(Session, [EventRegistrationToken; 3])>,
    tx: Sender<()>,
}

impl Watcher {
    fn new(tx: Sender<()>) -> Result<Self> {
        com_init_mta();
        let manager =
            block_on(&SessionManager::RequestAsync().map_err(|e| WinError::api("SMTC 管理器", e))?)
                .map_err(|e| WinError::api("SMTC 管理器", e))?;
        let mut w = Self {
            manager,
            created: Instant::now(),
            mgr_tokens: None,
            session_hooks: Vec::new(),
            tx,
        };
        w.resubscribe()?;
        Ok(w)
    }

    /// 会话增删后必须重挂 新会话此前无订阅
    fn resubscribe(&mut self) -> Result<()> {
        if let Some((t1, t2)) = self.mgr_tokens.take() {
            if self.manager.RemoveSessionsChanged(t1).is_err()
                || self.manager.RemoveCurrentSessionChanged(t2).is_err()
            {
                eprintln!("[smtc] 管理器旧事件摘除失败（句柄可能已失效）");
            }
        }
        let mut unhook_failed = false;
        for (s, tokens) in self.session_hooks.drain(..) {
            if s.RemoveMediaPropertiesChanged(tokens[0]).is_err()
                || s.RemovePlaybackInfoChanged(tokens[1]).is_err()
                || s.RemoveTimelinePropertiesChanged(tokens[2]).is_err()
            {
                unhook_failed = true;
            }
        }
        if unhook_failed {
            eprintln!("[smtc] 部分会话旧事件摘除失败（会话可能已销毁）");
        }

        let tx = self.tx.clone();
        let on_sessions = TypedEventHandler::new(
            move |_: &Option<SessionManager>, _: &Option<SessionsChangedEventArgs>| {
                let _ = tx.send(());
                Ok(())
            },
        );
        let tx = self.tx.clone();
        let on_current = TypedEventHandler::new(
            move |_: &Option<SessionManager>, _: &Option<CurrentSessionChangedEventArgs>| {
                let _ = tx.send(());
                Ok(())
            },
        );
        let t1 = self
            .manager
            .SessionsChanged(&on_sessions)
            .map_err(|e| WinError::api("SMTC 事件订阅", e))?;
        let t2 = self
            .manager
            .CurrentSessionChanged(&on_current)
            .map_err(|e| WinError::api("SMTC 事件订阅", e))?;
        self.mgr_tokens = Some((t1, t2));

        let sessions = self
            .manager
            .GetSessions()
            .map_err(|e| WinError::api("SMTC 会话列表", e))?;
        for i in 0..sessions
            .Size()
            .map_err(|e| WinError::api("SMTC 会话列表", e))?
        {
            let s = sessions
                .GetAt(i)
                .map_err(|e| WinError::api("SMTC 会话列表", e))?;
            let hook = || -> Result<[EventRegistrationToken; 3]> {
                let tx = self.tx.clone();
                let on_media = TypedEventHandler::new(
                    move |_: &Option<Session>, _: &Option<MediaPropertiesChangedEventArgs>| {
                        let _ = tx.send(());
                        Ok(())
                    },
                );
                let tx = self.tx.clone();
                let on_playback = TypedEventHandler::new(
                    move |_: &Option<Session>, _: &Option<PlaybackInfoChangedEventArgs>| {
                        let _ = tx.send(());
                        Ok(())
                    },
                );
                let tx = self.tx.clone();
                let on_timeline = TypedEventHandler::new(
                    move |_: &Option<Session>, _: &Option<TimelinePropertiesChangedEventArgs>| {
                        let _ = tx.send(());
                        Ok(())
                    },
                );
                Ok([
                    s.MediaPropertiesChanged(&on_media)
                        .map_err(|e| WinError::api("SMTC 事件订阅", e))?,
                    s.PlaybackInfoChanged(&on_playback)
                        .map_err(|e| WinError::api("SMTC 事件订阅", e))?,
                    s.TimelinePropertiesChanged(&on_timeline)
                        .map_err(|e| WinError::api("SMTC 事件订阅", e))?,
                ])
            };
            match hook() {
                Ok(tokens) => self.session_hooks.push((s, tokens)),
                Err(e) => eprintln!("[smtc] 单个会话事件订阅失败（跳过该会话）: {e}"),
            }
        }
        Ok(())
    }
}

pub fn start_watch(on_change: impl Fn() + Send + 'static) {
    use std::sync::Once;
    static START: Once = Once::new();
    START.call_once(|| {
        if let Err(e) = std::thread::Builder::new()
            .name("smtc-watch".into())
            .spawn(move || watch_loop(on_change))
        {
            eprintln!("[smtc] 监听线程启动失败，SMTC 事件推送不可用: {e}");
        }
    });
}

fn watch_loop(on_change: impl Fn()) {
    let (tx, rx) = channel::<()>();
    // 服务未就绪时创建会失败 重试
    let mut watcher = loop {
        match Watcher::new(tx.clone()) {
            Ok(w) => break w,
            Err(e) => {
                eprintln!("[smtc] 会话管理器创建失败，2s 后重试: {e}");
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    };
    // 先推一次当前快照
    let _ = tx.send(());
    loop {
        if rx.recv().is_err() {
            return;
        }
        // 防抖归并成串事件
        loop {
            match rx.recv_timeout(Duration::from_millis(150)) {
                Ok(()) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if watcher.created.elapsed() > Duration::from_secs(30) {
            match Watcher::new(tx.clone()) {
                Ok(w) => watcher = w,
                Err(e) => eprintln!("[smtc] 重建会话管理器失败: {e}"),
            }
        } else if let Err(e) = watcher.resubscribe() {
            eprintln!("[smtc] 重挂会话事件失败: {e}");
        }
        on_change();
    }
}
