//! 注入组件的本地 WebSocket 服务端 每个播放器独占端口与 token
//! 首帧须带 token 之后每帧原样交给 on_frame

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::Message;

#[derive(Debug, Clone, Copy)]
pub struct Endpoint {
    pub name: &'static str,
    /// 须与注入组件一致
    pub port: u16,
    pub token: &'static str,
}

#[derive(Debug, Default)]
struct Shared {
    connected: AtomicBool,
    outbox: Mutex<VecDeque<String>>,
}

#[derive(Debug)]
pub struct BridgeServer {
    shared: Arc<Shared>,
}

impl BridgeServer {
    /// on_frame 返回真才通知 on_change
    pub fn start(
        endpoint: Endpoint,
        on_frame: impl Fn(&serde_json::Value) -> bool + Send + 'static,
        on_close: impl Fn() + Send + 'static,
        on_change: fn(),
    ) -> Self {
        let shared = Arc::new(Shared::default());
        let server_shared = Arc::clone(&shared);
        let name = endpoint.name;
        if let Err(e) = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || serve(endpoint, &server_shared, on_frame, on_close, on_change))
        {
            eprintln!("[{name}] WS 服务端线程启动失败: {e}");
        }
        Self { shared }
    }

    pub fn is_connected(&self) -> bool {
        self.shared.connected.load(Ordering::Acquire)
    }

    pub fn send(&self, frame: String) -> bool {
        if !self.is_connected() {
            return false;
        }
        self.shared
            .outbox
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(frame);
        true
    }
}

/// WebSocket 不受 CORS 约束 任意网页都能连本机
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

/// 串行单连接 只有一个宿主进程会连
fn serve(
    endpoint: Endpoint,
    shared: &Shared,
    on_frame: impl Fn(&serde_json::Value) -> bool,
    on_close: impl Fn(),
    on_change: fn(),
) {
    let name = endpoint.name;
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, endpoint.port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[{name}] 绑定 127.0.0.1:{} 失败: {e}", endpoint.port);
            return;
        }
    };
    for stream in listener.incoming() {
        match stream {
            Ok(tcp) => {
                // panic 只断本连接 线程死了永久失联
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handle_conn(endpoint, shared, tcp, &on_frame, on_change)
                }));
                if r.is_err() {
                    eprintln!("[{name}] 连接处理 panic，已断开该连接");
                }
            }
            Err(e) => eprintln!("[{name}] 接受连接失败: {e}"),
        }
        let was_connected = shared.connected.swap(false, Ordering::AcqRel);
        shared
            .outbox
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        if was_connected {
            on_close();
            on_change();
        }
    }
}

fn handle_conn(
    endpoint: Endpoint,
    shared: &Shared,
    tcp: TcpStream,
    on_frame: &impl Fn(&serde_json::Value) -> bool,
    on_change: fn(),
) {
    let name = endpoint.name;
    // 读超时让循环能发 outbox WouldBlock 时 tungstenite 保留半包
    if let Err(e) = tcp.set_read_timeout(Some(Duration::from_millis(150))) {
        eprintln!("[{name}] 设置读超时失败: {e}");
        return;
    }
    let mut ws = match tungstenite::accept_hdr(tcp, refuse_web_origin) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("[{name}] WebSocket 握手失败: {e}");
            return;
        }
    };

    let mut authed = false;
    // 未鉴权连接不得久占唯一连接位
    let auth_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        loop {
            let next = shared
                .outbox
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .pop_front();
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
                let Some(frame) = authenticate(text.as_str(), &mut authed, endpoint.token) else {
                    continue;
                };
                shared.connected.store(true, Ordering::Release);
                if on_frame(&frame) {
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

/// 未鉴权或非 JSON 返回 None
fn authenticate(text: &str, authed: &mut bool, token: &str) -> Option<serde_json::Value> {
    let v = serde_json::from_str::<serde_json::Value>(text).ok()?;
    if !*authed {
        if v.get("token").and_then(|t| t.as_str()) != Some(token) {
            return None;
        }
        *authed = true;
    }
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "test-token";

    #[test]
    fn frames_without_token_are_rejected_until_authed() {
        let mut authed = false;
        assert!(authenticate(r#"{"type":"state"}"#, &mut authed, TOKEN).is_none());
        assert!(!authed);
        assert!(authenticate(r#"{"type":"hello","token":"wrong"}"#, &mut authed, TOKEN).is_none());
        assert!(!authed);
    }

    #[test]
    fn token_authenticates_then_later_frames_pass() {
        let mut authed = false;
        let hello = format!(r#"{{"type":"hello","token":"{TOKEN}"}}"#);
        assert!(authenticate(&hello, &mut authed, TOKEN).is_some());
        assert!(authed);
        let v = authenticate(r#"{"type":"state","positionMs":5}"#, &mut authed, TOKEN).unwrap();
        assert_eq!(v["positionMs"], 5);
    }

    #[test]
    fn malformed_json_is_dropped() {
        let mut authed = true;
        assert!(authenticate("not json", &mut authed, TOKEN).is_none());
    }
}
