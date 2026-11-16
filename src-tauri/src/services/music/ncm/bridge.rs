//! 网易云 bridge 协议须与 bridge.js 同步
//! bridge → 岛 `{type:"state", songId, playId, title, artist, album, coverUrl,
//!   durationMs, positionMs, anchorEpochMs, playing}` 首帧带 token
//! 岛 → bridge `{type:"control", action:"play"|"pause"|"seek", positionMs?}`

use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::error::AppResult;
use crate::services::music::bridge_server::{BridgeServer, Endpoint};
use crate::services::music::catalog::{Catalog, SongRef};
use crate::services::music::provider::{now_epoch_ms, Control, MusicProvider, ProviderState};

/// 端口与 token 须与 bridge.js 一致
const ENDPOINT: Endpoint = Endpoint {
    name: "ncm-bridge",
    port: 52847,
    token: "top-island-ncm-bridge-v1",
};

#[derive(Debug)]
pub struct NcmBridgeProvider {
    server: BridgeServer,
    state: Arc<Mutex<Option<ProviderState>>>,
}

impl NcmBridgeProvider {
    pub fn start(on_change: fn()) -> Self {
        let state: Arc<Mutex<Option<ProviderState>>> = Arc::default();
        let on_frame_state = Arc::clone(&state);
        let on_close_state = Arc::clone(&state);
        let server = BridgeServer::start(
            ENDPOINT,
            move |frame| {
                if frame.get("type").and_then(|t| t.as_str()) != Some("state") {
                    return false;
                }
                *on_frame_state.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(parse_state_frame(frame, now_epoch_ms()));
                true
            },
            move || *on_close_state.lock().unwrap_or_else(|e| e.into_inner()) = None,
            on_change,
        );
        Self { server, state }
    }

    pub fn is_connected(&self) -> bool {
        self.server.is_connected()
    }
}

impl MusicProvider for NcmBridgeProvider {
    fn name(&self) -> &'static str {
        "ncm-bridge"
    }

    fn claims(&self) -> &'static str {
        "cloudmusic"
    }

    /// 自带完整曲目 不依赖 SMTC
    fn snapshot(&self, _base: Option<&ProviderState>) -> Option<ProviderState> {
        if !self.server.is_connected() {
            return None;
        }
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn control(&self, action: Control) -> AppResult<bool> {
        let frame = match action {
            Control::Play => json!({ "type": "control", "action": "play" }),
            Control::Pause => json!({ "type": "control", "action": "pause" }),
            Control::Seek(ms) => {
                json!({ "type": "control", "action": "seek", "positionMs": ms.max(0) })
            }
            Control::Next | Control::Prev => return Ok(false),
        };
        Ok(self.server.send(frame.to_string()))
    }
}

fn parse_state_frame(v: &serde_json::Value, now_ms: i64) -> ProviderState {
    let str_of = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    // JS 侧毫秒常非整数 as_i64 会返回 None
    let num_of = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_f64)
            .map(|f| f.round() as i64)
    };

    let playing = v
        .get("playing")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    ProviderState {
        is_playing: playing,
        title: str_of("title").unwrap_or_default(),
        artist: str_of("artist").unwrap_or_default(),
        album: str_of("album"),
        source_app_id: "cloudmusic.exe".to_string(),
        song_id: str_of("songId")
            .filter(|s| !s.is_empty())
            .map(|id| SongRef::new(Catalog::Netease, id)),
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

        let s = frame(
            r#"{"type":"state","songId":"1","positionMs":144096.00000000003,"playing":true}"#,
        );
        assert_eq!(s.position_ms, 144096);

        let s = frame(r#"{"type":"state","songId":"1","positionMs":64322,"playing":true}"#);
        assert_eq!(s.position_ms, 64322);
    }

    #[test]
    fn fractional_duration_and_anchor_are_read() {
        let s = frame(
            r#"{"type":"state","songId":"1","durationMs":177899.99,"anchorEpochMs":1788691227437.7}"#,
        );
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
}
