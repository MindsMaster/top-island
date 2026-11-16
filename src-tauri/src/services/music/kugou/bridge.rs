//! 协议须与 island-kugou-bridge 的 report.rs 同步
//! 曲目与播控走 SMTC 余下走组件

use std::sync::{Arc, Mutex};

use island_core::{LyricLine, LyricsData};

use crate::services::music::bridge_server::{BridgeServer, Endpoint};
use crate::services::music::hash::content_hash;
use serde_json::json;

use crate::error::AppResult;
use crate::services::music::provider::{Control, LocalLyrics, MusicProvider, ProviderState};

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

#[derive(Debug, Default)]
struct Shared {
    progress: Option<Progress>,
    /// 附歌名 与 SMTC 曲目对上才用
    lyrics: Option<(String, LocalLyrics)>,
}

#[derive(Debug)]
pub struct KugouBridgeProvider {
    server: BridgeServer,
    shared: Arc<Mutex<Shared>>,
    /// 识别进度条偶有落空 按曲目沿用
    last_duration: Mutex<Option<(String, i64)>>,
}

impl KugouBridgeProvider {
    pub fn start(on_change: fn()) -> Self {
        let shared: Arc<Mutex<Shared>> = Arc::default();
        let on_frame = Arc::clone(&shared);
        let on_close = Arc::clone(&shared);
        let server = BridgeServer::start(
            ENDPOINT,
            move |frame| {
                let mut s = on_frame.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(p) = parse_progress(frame) {
                    s.progress = Some(p);
                } else if let Some(l) = parse_lyrics(frame) {
                    s.lyrics = Some(l);
                } else {
                    return false;
                }
                true
            },
            move || *on_close.lock().unwrap_or_else(|e| e.into_inner()) = Shared::default(),
            on_change,
        );
        Self {
            server,
            shared,
            last_duration: Mutex::new(None),
        }
    }

    fn shared(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn is_connected(&self) -> bool {
        self.server.is_connected()
    }
}

impl MusicProvider for KugouBridgeProvider {
    fn name(&self) -> &'static str {
        "kugou-bridge"
    }

    fn claims(&self) -> &'static str {
        "kugou"
    }

    fn snapshot(&self, base: Option<&ProviderState>) -> Option<ProviderState> {
        if !self.server.is_connected() {
            return None;
        }
        let mut p = self.shared().progress?;
        let base = base?.clone();
        let track = format!("{}|{}", base.title, base.artist);
        let mut last = self.last_duration.lock().unwrap_or_else(|e| e.into_inner());
        match p.duration_ms {
            Some(d) => *last = Some((track, d)),
            None => p.duration_ms = last.as_ref().filter(|(t, _)| *t == track).map(|(_, d)| *d),
        }
        Some(merge(base, p))
    }

    fn lyrics(&self, state: &ProviderState) -> Option<LocalLyrics> {
        let s = self.shared();
        let (title, l) = s.lyrics.as_ref()?;
        same_title(title, &state.title).then(|| l.clone())
    }

    fn control(&self, action: Control) -> AppResult<bool> {
        let Control::Seek(ms) = action else {
            return Ok(false);
        };
        let frame = json!({ "type": "control", "action": "seek", "positionMs": ms.max(0) });
        Ok(self.server.send(frame.to_string()))
    }
}

fn same_title(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// 认出进度条才能跳转
fn merge(base: ProviderState, p: Progress) -> ProviderState {
    ProviderState {
        is_playing: p.playing,
        position_ms: p.position_ms,
        anchor_epoch_ms: p.anchor_epoch_ms,
        rate: if p.playing { 1.0 } else { 0.0 },
        duration_ms: p.duration_ms.or(base.duration_ms),
        seek_supported: p.duration_ms.is_some(),
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

fn parse_lyrics(v: &serde_json::Value) -> Option<(String, LocalLyrics)> {
    if v.get("type").and_then(|t| t.as_str()) != Some("lyrics") {
        return None;
    }
    let title = v.get("title")?.as_str()?.to_string();
    let lines = v
        .get("lines")?
        .as_array()?
        .iter()
        .filter_map(|l| {
            let time_ms = l.get(0)?.as_i64()?;
            let text = l.get(1)?.as_str()?.trim();
            (!text.is_empty()).then(|| LyricLine {
                time_ms,
                text: text.to_string(),
            })
        })
        .collect();
    let data = LyricsData {
        lines,
        duration_ms: v
            .get("lengthMs")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
    };
    let key = format!("kugou:{}", content_hash(v.to_string().as_bytes()));
    Some((
        title,
        LocalLyrics {
            key,
            data: Arc::new(data),
        },
    ))
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
    fn lyrics_frame_keeps_timed_non_empty_lines() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"type":"lyrics","title":"洗牌 (Live)","artist":"SHarK/Rapeter","lengthMs":152297,
                "lines":[[892,"洗牌 (Live) - SHarK/Rapeter"],[1524,"  "],[35504,"有多烫就多烫"]]}"#,
        )
        .unwrap();
        let (title, l) = parse_lyrics(&v).unwrap();
        assert_eq!(title, "洗牌 (Live)");
        assert_eq!(l.data.lines.len(), 2);
        assert_eq!(l.data.lines[1].time_ms, 35_504);
        assert_eq!(l.data.duration_ms, 152_297);
        assert!(l.key.starts_with("kugou:"));
        assert!(parse_progress(&v).is_none());
    }

    #[test]
    fn lyrics_only_match_same_title() {
        assert!(same_title("洗牌 (Live)", " 洗牌 (live)"));
        assert!(!same_title("洗牌 (Live)", "林宛瑜"));
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
        assert!(s.seek_supported);
    }
}
