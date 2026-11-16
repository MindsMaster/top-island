//! 协议须与 island-kugou-bridge 的 report.rs 同步
//! 曲目与切歌走 SMTC 组件只补进度 播放状态与总时长

use std::sync::{Arc, Mutex};

use crate::error::AppResult;
use crate::services::music::bridge_server::{BridgeServer, Endpoint};
use crate::services::music::provider::{Capabilities, Control, MusicProvider, ProviderState};
use crate::services::music::smtc::SmtcProvider;

/// 端口与 token 须与 island-kugou-bridge 一致
const ENDPOINT: Endpoint = Endpoint {
    name: "kugou-bridge",
    port: 52848,
    token: "top-island-kugou-bridge-v1",
};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Progress {
    position_ms: i64,
    anchor_epoch_ms: i64,
    playing: bool,
    duration_ms: Option<i64>,
}

#[derive(Debug)]
pub struct KugouBridgeProvider {
    server: BridgeServer,
    progress: Arc<Mutex<Option<Progress>>>,
    smtc: &'static SmtcProvider,
}

impl KugouBridgeProvider {
    pub fn start(on_change: fn(), smtc: &'static SmtcProvider) -> Self {
        let progress: Arc<Mutex<Option<Progress>>> = Arc::default();
        let on_frame_progress = Arc::clone(&progress);
        let on_close_progress = Arc::clone(&progress);
        let server = BridgeServer::start(
            ENDPOINT,
            move |frame| {
                let Some(p) = parse_progress(frame) else {
                    return false;
                };
                *on_frame_progress.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
                true
            },
            move || *on_close_progress.lock().unwrap_or_else(|e| e.into_inner()) = None,
            on_change,
        );
        Self {
            server,
            progress,
            smtc,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.server.is_connected()
    }
}

impl MusicProvider for KugouBridgeProvider {
    fn name(&self) -> &'static str {
        "kugou-bridge"
    }

    fn snapshot(&self) -> Option<ProviderState> {
        if !self.server.is_connected() {
            return None;
        }
        let p = (*self.progress.lock().unwrap_or_else(|e| e.into_inner()))?;
        let base = self
            .smtc
            .snapshot()
            .filter(|s| s.source_app_id.to_lowercase().contains("kugou"))?;
        Some(merge(base, p))
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            skip: true,
            artwork_bitmap: true,
            fallback: false,
        }
    }

    fn control(&self, action: Control) -> AppResult<bool> {
        match action {
            Control::Seek(_) => Ok(false),
            _ => self.smtc.control(action),
        }
    }

    fn artwork_bytes(&self) -> Option<Vec<u8>> {
        self.smtc.artwork_bytes()
    }
}

fn merge(base: ProviderState, p: Progress) -> ProviderState {
    ProviderState {
        is_playing: p.playing,
        position_ms: p.position_ms,
        anchor_epoch_ms: p.anchor_epoch_ms,
        rate: if p.playing { 1.0 } else { 0.0 },
        duration_ms: p.duration_ms.or(base.duration_ms),
        seek_supported: false,
        ..base
    }
}

fn parse_progress(v: &serde_json::Value) -> Option<Progress> {
    if v.get("type").and_then(|t| t.as_str()) != Some("progress") {
        return None;
    }
    let num_of = |k: &str| v.get(k).and_then(serde_json::Value::as_i64);
    Some(Progress {
        position_ms: num_of("positionMs")?.max(0),
        anchor_epoch_ms: num_of("anchorEpochMs")?,
        playing: v.get("playing").and_then(serde_json::Value::as_bool)?,
        duration_ms: num_of("durationMs").filter(|d| *d > 0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Option<Progress> {
        parse_progress(&serde_json::from_str(json).unwrap())
    }

    #[test]
    fn progress_frame_parses_with_and_without_duration() {
        let p = parse(r#"{"type":"progress","positionMs":33615,"anchorEpochMs":1000,"playing":true,"durationMs":256470}"#).unwrap();
        assert_eq!(p.position_ms, 33615);
        assert_eq!(p.duration_ms, Some(256_470));
        let p = parse(r#"{"type":"progress","positionMs":5,"anchorEpochMs":1000,"playing":false,"durationMs":null}"#).unwrap();
        assert_eq!(p.duration_ms, None);
        assert!(!p.playing);
    }

    #[test]
    fn hello_and_incomplete_frames_are_ignored() {
        assert_eq!(parse(r#"{"type":"hello","token":"x"}"#), None);
        assert_eq!(parse(r#"{"type":"progress","positionMs":5}"#), None);
    }

    #[test]
    fn merge_overrides_timeline_but_keeps_track() {
        let base = ProviderState {
            is_playing: true,
            title: "Here With You".into(),
            artist: "Electus".into(),
            album: None,
            source_app_id: "kugou".into(),
            song_id: None,
            position_ms: 0,
            anchor_epoch_ms: 0,
            rate: 0.0,
            duration_ms: None,
            seek_supported: false,
            artwork_url: None,
        };
        let p = Progress {
            position_ms: 47_503,
            anchor_epoch_ms: 9_000,
            playing: false,
            duration_ms: Some(279_370),
        };
        let s = merge(base, p);
        assert_eq!(s.title, "Here With You");
        assert_eq!(s.position_ms, 47_503);
        assert_eq!(s.duration_ms, Some(279_370));
        assert!(!s.is_playing);
        assert_eq!(s.rate, 0.0);
    }
}
