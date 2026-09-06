//! 网易云进程内 bridge 的 WebSocket 服务端。bridge（`island-cloudmusic-bridge/js/bridge.js`）
//! 作客户端连进来，上报锚点式播放状态，接收 play/pause/seek。
//!
//! 协议（JSON 文本帧）：
//! - bridge → 岛：`{type:"state", songId, playId, title, artist, album, coverUrl,
//!   durationMs, positionMs, anchorEpochMs, playing}`，首帧带 `token`
//! - 岛 → bridge：`{type:"control", action:"play"|"pause"|"seek", positionMs?}`

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::Message;

use super::provider::{now_epoch_ms, Capabilities, Control, MusicProvider, ProviderState};
use crate::error::AppResult;

/// 与 bridge.js 保持一致
pub const BRIDGE_PORT: u16 = 52847;
pub const BRIDGE_TOKEN: &str = "top-island-ncm-bridge-v1";

#[derive(Debug, Default)]
struct BridgeInner {
    state: Mutex<Option<ProviderState>>,
    connected: AtomicBool,
    /// 待下发的控制帧，连接线程取走发出
    outbox: Mutex<VecDeque<String>>,
}

#[derive(Debug)]
pub struct NcmBridgeProvider {
    inner: Arc<BridgeInner>,
}

impl NcmBridgeProvider {
    /// `on_change` 在收到新状态帧或断连时触发
    pub fn start(on_change: impl Fn() + Send + 'static) -> Self {
        let inner = Arc::new(BridgeInner::default());
        let server_inner = Arc::clone(&inner);
        if let Err(e) = std::thread::Builder::new()
            .name("ncm-bridge-ws".into())
            .spawn(move || serve(server_inner, on_change))
        {
            eprintln!("[music:bridge] WS 服务端线程启动失败，网易云权威源不可用: {e}");
        }
        Self { inner }
    }

    pub fn is_connected(&self) -> bool {
        self.inner.connected.load(Ordering::Acquire)
    }
}

impl MusicProvider for NcmBridgeProvider {
    fn name(&self) -> &'static str {
        "ncm-bridge"
    }

    fn snapshot(&self) -> Option<ProviderState> {
        if !self.inner.connected.load(Ordering::Acquire) {
            return None;
        }
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { skip: false, artwork_bitmap: false, fallback: false }
    }

    fn control(&self, action: Control) -> AppResult<bool> {
        if !self.inner.connected.load(Ordering::Acquire) {
            return Ok(false);
        }
        let frame = match action {
            Control::Play => json!({ "type": "control", "action": "play" }),
            Control::Pause => json!({ "type": "control", "action": "pause" }),
            Control::Seek(ms) => json!({ "type": "control", "action": "seek", "positionMs": ms.max(0) }),
            Control::Next | Control::Prev => return Ok(false),
        };
        self.inner
            .outbox
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(frame.to_string());
        Ok(true)
    }
}

/// 拒绝网页来源。WebSocket 不受 CORS 约束，任意网页都能连 127.0.0.1；bridge 是 orpheus:// 源
fn refuse_web_origin(req: &Request, resp: Response) -> Result<Response, ErrorResponse> {
    if let Some(origin) = req.headers().get("origin") {
        let o = origin.to_str().unwrap_or("");
        if o.starts_with("http://") || o.starts_with("https://") {
            let refused = tungstenite::http::Response::builder()
                .status(tungstenite::http::StatusCode::FORBIDDEN)
                .body(Some("origin not allowed".to_string()))
                .expect("build refuse response");
            return Err(refused);
        }
    }
    Ok(resp)
}

/// 串行处理单连接；只有网易云一个渲染进程会连进来
fn serve(inner: Arc<BridgeInner>, on_change: impl Fn()) {
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, BRIDGE_PORT)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[music:bridge] 绑定 127.0.0.1:{BRIDGE_PORT} 失败: {e}");
            return;
        }
    };
    for stream in listener.incoming() {
        match stream {
            Ok(tcp) => {
                // 连接内的 panic 只断这条连接，服务端线程死了 bridge 就永久失联
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handle_conn(&inner, tcp, &on_change)
                }));
                if r.is_err() {
                    eprintln!("[music:bridge] 连接处理 panic，已断开该连接");
                }
            }
            Err(e) => eprintln!("[music:bridge] 接受连接失败: {e}"),
        }
        let was_connected = inner.connected.swap(false, Ordering::AcqRel);
        *inner.state.lock().unwrap_or_else(|e| e.into_inner()) = None;
        inner.outbox.lock().unwrap_or_else(|e| e.into_inner()).clear();
        if was_connected {
            on_change();
        }
    }
}

fn handle_conn(inner: &Arc<BridgeInner>, tcp: TcpStream, on_change: &impl Fn()) {
    // 读超时让循环能周期性地去发 outbox；tungstenite 在 WouldBlock 时保留半包状态
    if let Err(e) = tcp.set_read_timeout(Some(Duration::from_millis(150))) {
        eprintln!("[music:bridge] 设置读超时失败: {e}");
        return;
    }
    let mut ws = match tungstenite::accept_hdr(tcp, refuse_web_origin) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("[music:bridge] WebSocket 握手失败: {e}");
            return;
        }
    };

    let mut authed = false;
    // 不鉴权的连接不能一直占着这唯一的连接位
    let auth_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        loop {
            let next = inner.outbox.lock().unwrap_or_else(|e| e.into_inner()).pop_front();
            let Some(text) = next else { break };
            if ws.send(Message::text(text)).is_err() {
                return;
            }
        }

        if !authed && Instant::now() >= auth_deadline {
            return;
        }

        match ws.read() {
            Ok(Message::Text(text)) => {
                if handle_frame(inner, text.as_str(), &mut authed) {
                    on_change();
                }
            }
            Ok(Message::Ping(payload)) => {
                let _ = ws.send(Message::Pong(payload));
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
    }
}

#[derive(Debug, PartialEq)]
enum Inbound {
    Unauthorized,
    Ignored,
    State(ProviderState),
}

/// 任一帧带正确 token 即完成鉴权
fn classify_frame(text: &str, authed: &mut bool, now_ms: i64) -> Inbound {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return Inbound::Ignored;
    };
    if !*authed {
        if v.get("token").and_then(|t| t.as_str()) == Some(BRIDGE_TOKEN) {
            *authed = true;
        } else {
            return Inbound::Unauthorized;
        }
    }
    if v.get("type").and_then(|t| t.as_str()) != Some("state") {
        return Inbound::Ignored;
    }
    Inbound::State(parse_state_frame(&v, now_ms))
}

/// 返回是否更新了状态
fn handle_frame(inner: &Arc<BridgeInner>, text: &str, authed: &mut bool) -> bool {
    let was_authed = *authed;
    match classify_frame(text, authed, now_epoch_ms()) {
        Inbound::Unauthorized => false,
        Inbound::Ignored => {
            if *authed && !was_authed {
                inner.connected.store(true, Ordering::Release);
            }
            false
        }
        Inbound::State(state) => {
            inner.connected.store(true, Ordering::Release);
            *inner.state.lock().unwrap_or_else(|e| e.into_inner()) = Some(state);
            true
        }
    }
}

fn parse_state_frame(v: &serde_json::Value, now_ms: i64) -> ProviderState {
    let str_of = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    // JS 侧 秒*1000 常是非整数，serde_json 对非整数 as_i64 返回 None
    let num_of = |k: &str| v.get(k).and_then(serde_json::Value::as_f64).map(|f| f.round() as i64);

    let playing = v.get("playing").and_then(serde_json::Value::as_bool).unwrap_or(false);
    ProviderState {
        is_playing: playing,
        title: str_of("title").unwrap_or_default(),
        artist: str_of("artist").unwrap_or_default(),
        album: str_of("album"),
        source_app_id: "cloudmusic.exe".to_string(),
        song_id: str_of("songId").filter(|s| !s.is_empty()),
        position_ms: num_of("positionMs").unwrap_or(0).max(0),
        anchor_epoch_ms: num_of("anchorEpochMs").unwrap_or(now_ms),
        rate: if playing { 1.0 } else { 0.0 },
        duration_ms: num_of("durationMs").filter(|d| *d > 0),
        seek_supported: true,
        artwork_url: str_of("coverUrl").filter(|s| !s.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(json: &str) -> ProviderState {
        parse_state_frame(&serde_json::from_str(json).unwrap(), 1_000_000)
    }

    #[test]
    fn fractional_position_is_not_read_as_zero() {
        let s = frame(r#"{"type":"state","songId":"1","positionMs":63992.5,"playing":true}"#);
        assert_eq!(s.position_ms, 63993);

        let s = frame(r#"{"type":"state","songId":"1","positionMs":144096.00000000003,"playing":true}"#);
        assert_eq!(s.position_ms, 144096);

        let s = frame(r#"{"type":"state","songId":"1","positionMs":64322,"playing":true}"#);
        assert_eq!(s.position_ms, 64322);
    }

    #[test]
    fn fractional_duration_and_anchor_are_read() {
        let s = frame(r#"{"type":"state","songId":"1","durationMs":177899.99,"anchorEpochMs":1788691227437.7}"#);
        assert_eq!(s.duration_ms, Some(177900));
        assert_eq!(s.anchor_epoch_ms, 1788691227438);
    }

    #[test]
    fn missing_fields_fall_back_sanely() {
        let s = frame(r#"{"type":"state"}"#);
        assert_eq!(s.position_ms, 0);
        assert_eq!(s.anchor_epoch_ms, 1_000_000);
        assert_eq!(s.duration_ms, None);
        assert!(!s.is_playing);
        assert_eq!(s.rate, 0.0);
        assert_eq!(s.song_id, None);
    }

    #[test]
    fn rate_mirrors_playing() {
        assert_eq!(frame(r#"{"type":"state","playing":true}"#).rate, 1.0);
        assert_eq!(frame(r#"{"type":"state","playing":false}"#).rate, 0.0);
    }

    #[test]
    fn frames_without_token_are_rejected_until_authed() {
        let mut authed = false;
        let r = classify_frame(r#"{"type":"state","positionMs":5}"#, &mut authed, 0);
        assert_eq!(r, Inbound::Unauthorized);
        assert!(!authed);
        let r = classify_frame(r#"{"type":"hello","token":"wrong"}"#, &mut authed, 0);
        assert_eq!(r, Inbound::Unauthorized);
        assert!(!authed);
    }

    #[test]
    fn hello_with_token_authenticates_then_later_frames_pass() {
        let mut authed = false;
        let hello = format!(r#"{{"type":"hello","token":"{BRIDGE_TOKEN}"}}"#);
        assert_eq!(classify_frame(&hello, &mut authed, 0), Inbound::Ignored);
        assert!(authed);
        let r = classify_frame(r#"{"type":"state","songId":"7","positionMs":1234}"#, &mut authed, 0);
        match r {
            Inbound::State(s) => {
                assert_eq!(s.song_id.as_deref(), Some("7"));
                assert_eq!(s.position_ms, 1234);
            }
            other => panic!("expected State, got {other:?}"),
        }
    }

    #[test]
    fn token_on_first_state_frame_also_authenticates() {
        let mut authed = false;
        let f = format!(r#"{{"type":"state","token":"{BRIDGE_TOKEN}","positionMs":42}}"#);
        assert!(matches!(classify_frame(&f, &mut authed, 0), Inbound::State(_)));
        assert!(authed);
    }

    #[test]
    fn malformed_json_and_unknown_types_are_ignored() {
        let mut authed = true;
        assert_eq!(classify_frame("not json", &mut authed, 0), Inbound::Ignored);
        assert_eq!(classify_frame(r#"{"type":"whatever"}"#, &mut authed, 0), Inbound::Ignored);
    }
}
