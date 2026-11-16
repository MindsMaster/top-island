//! 协议须与 services/music/kugou 同步
//! 上行 `{type:"hello", token}` 连上即发
//! 上行 `{type:"progress", positionMs, anchorEpochMs, playing, durationMs?}`
//! 上行 `{type:"lyrics", title, artist, lengthMs, lines:[[timeMs, text]]}`
//! 下行 `{type:"control", action:"seek", positionMs}`

use std::io::ErrorKind;
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde_json::json;
use tungstenite::{Error, Message, WebSocket};

use crate::hooks::{epoch_ms, LYRIC, POSITION_MS, SLIDERS, TICK_EPOCH_MS};
use crate::seek;

/// 端口与 token 须与 services/music/kugou 一致
const PORT: u16 = 52848;
const TOKEN: &str = "top-island-kugou-bridge-v1";

const SAMPLE_EVERY: Duration = Duration::from_millis(200);
/// 回调停这么久视为暂停
const PAUSED_AFTER_MS: i64 = 500;
/// 超出即视为拖动或切歌
const DRIFT_MS: i64 = 1000;
const HEARTBEAT: Duration = Duration::from_secs(5);
const RETRY_EVERY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq)]
struct Sample {
    position_ms: i64,
    anchor_epoch_ms: i64,
    playing: bool,
    duration_ms: Option<i64>,
}

fn sample(now_ms: i64) -> Option<Sample> {
    let tick = TICK_EPOCH_MS.load(Ordering::Acquire);
    if tick == 0 {
        return None;
    }
    let position_ms = POSITION_MS.load(Ordering::Relaxed);
    let duration_ms = SLIDERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .duration_ms(position_ms);
    Some(Sample {
        position_ms,
        anchor_epoch_ms: tick,
        playing: now_ms - tick < PAUSED_AFTER_MS,
        duration_ms,
    })
}

/// 匀速走动不算变化
fn changed(prev: &Sample, cur: &Sample) -> bool {
    if prev.playing != cur.playing || prev.duration_ms != cur.duration_ms {
        return true;
    }
    let elapsed = if prev.playing {
        cur.anchor_epoch_ms - prev.anchor_epoch_ms
    } else {
        0
    };
    (cur.position_ms - (prev.position_ms + elapsed)).abs() > DRIFT_MS
}

pub fn run() {
    loop {
        if let Some(ws) = connect() {
            session(ws);
        }
        std::thread::sleep(RETRY_EVERY);
    }
}

fn connect() -> Option<WebSocket<TcpStream>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], PORT));
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(1)).ok()?;
    let _ = tcp.set_nodelay(true);
    // 读超时兼作采样节拍
    tcp.set_read_timeout(Some(SAMPLE_EVERY)).ok()?;
    let (ws, _) = tungstenite::client(format!("ws://127.0.0.1:{PORT}/"), tcp).ok()?;
    Some(ws)
}

fn session(mut ws: WebSocket<TcpStream>) {
    let hello = json!({ "type": "hello", "token": TOKEN });
    if ws.send(Message::text(hello.to_string())).is_err() {
        return;
    }
    let mut last: Option<Sample> = None;
    let mut last_sent = Instant::now();
    let mut lyric_sent = 0u64;
    loop {
        match ws.read() {
            Ok(Message::Text(text)) => {
                if let Some(ms) = seek_target(&text) {
                    seek::request(ms);
                }
            }
            Ok(_) => {}
            Err(Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
        if let Some(frame) = lyric_frame(&mut lyric_sent) {
            if ws.send(Message::text(frame)).is_err() {
                return;
            }
        }
        let Some(cur) = sample(epoch_ms()) else {
            continue;
        };
        let due = last.is_none_or(|prev| changed(&prev, &cur)) || last_sent.elapsed() >= HEARTBEAT;
        if !due {
            continue;
        }
        let frame = json!({
            "type": "progress",
            "positionMs": cur.position_ms,
            "anchorEpochMs": cur.anchor_epoch_ms,
            "playing": cur.playing,
            "durationMs": cur.duration_ms,
        });
        if ws.send(Message::text(frame.to_string())).is_err() {
            return;
        }
        last = Some(cur);
        last_sent = Instant::now();
    }
}

fn seek_target(text: &str) -> Option<i64> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if v.get("type")?.as_str()? != "control" || v.get("action")?.as_str()? != "seek" {
        return None;
    }
    v.get("positionMs")?.as_i64()
}

/// 只在代号变时出帧
fn lyric_frame(sent: &mut u64) -> Option<String> {
    let cur = LYRIC.lock().unwrap_or_else(|e| e.into_inner());
    let (generation, l) = cur.as_ref()?;
    if *generation == *sent {
        return None;
    }
    *sent = *generation;
    Some(
        json!({
            "type": "lyrics",
            "title": l.title,
            "artist": l.artist,
            "lengthMs": l.length_ms,
            "lines": l.lines,
        })
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(position_ms: i64, anchor_epoch_ms: i64, playing: bool) -> Sample {
        Sample {
            position_ms,
            anchor_epoch_ms,
            playing,
            duration_ms: Some(200_000),
        }
    }

    #[test]
    fn parses_seek_control_only() {
        assert_eq!(
            seek_target(r#"{"type":"control","action":"seek","positionMs":42000}"#),
            Some(42_000)
        );
        assert_eq!(seek_target(r#"{"type":"control","action":"pause"}"#), None);
        assert_eq!(seek_target("not json"), None);
    }

    #[test]
    fn steady_playback_is_not_a_change() {
        assert!(!changed(&s(10_000, 1_000, true), &s(10_200, 1_200, true)));
    }

    #[test]
    fn seek_or_track_change_is_a_change() {
        assert!(changed(&s(82_900, 1_000, true), &s(44_050, 1_200, true)));
        assert!(changed(&s(47_500, 1_000, true), &s(40, 1_200, true)));
    }

    #[test]
    fn pause_resume_and_duration_are_changes() {
        assert!(changed(&s(10_000, 1_000, true), &s(10_000, 1_000, false)));
        let mut cur = s(10_200, 1_200, true);
        cur.duration_ms = Some(193_750);
        assert!(changed(&s(10_000, 1_000, true), &cur));
    }
}
