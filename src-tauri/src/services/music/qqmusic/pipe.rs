//! QQ 音乐自带的本机命名管道 协议见 QQMusic.dll 的 QQMusicTransportServer
//! 发 `{cmd, params}` 收 `{reply, code}` 与 `{notify, data}` 每条以换行结尾

use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Pipes::PeekNamedPipe;

use island_core::LyricsData;

use super::qrc;
use crate::error::AppResult;
use crate::services::music::hash::content_hash;
use crate::services::music::provider::{
    now_epoch_ms, Control, LocalLyrics, MusicProvider, ProviderState,
};

const PIPE: &str = r"\\.\pipe\y.qq.com\QQMusicTransportServer";
/// 服务端不校验 appKey
const LOGIN: &str = r#"{"cmd":"login","params":{"appName":"top-island","appKey":"top-island"}}"#;
/// 3 播放 4 暂停
const STATUS_PLAYING: i64 = 3;
/// 与 SMTC 会话 ID 一致
const SOURCE_APP_ID: &str = "Tencent.QQMusic";
/// 超出即视为拖动或切歌
const DRIFT_MS: i64 = 1000;
/// 播放器加载歌词后才写缓存
const LYRIC_RETRY_MS: i64 = 2000;
const POLL: Duration = Duration::from_millis(50);
const RETRY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Default, PartialEq)]
struct Song {
    title: String,
    artist: String,
    album: String,
    cover_url: String,
}

#[derive(Debug, Clone, Default)]
struct Shared {
    logged_in: bool,
    song: Option<Song>,
    playing: bool,
    position_ms: i64,
    anchor_epoch_ms: i64,
    duration_ms: Option<i64>,
    lyrics: Option<LocalLyrics>,
    lyrics_tried_ms: i64,
}

impl Shared {
    fn position_at(&self, now_ms: i64) -> i64 {
        if self.playing {
            self.position_ms + (now_ms - self.anchor_epoch_ms)
        } else {
            self.position_ms
        }
    }

    fn anchor(&mut self, position_ms: i64, now_ms: i64) {
        self.position_ms = position_ms.max(0);
        self.anchor_epoch_ms = now_ms;
    }

    /// 返回是否需要推送
    fn apply(&mut self, v: &Value, now_ms: i64) -> bool {
        if v.get("reply").and_then(Value::as_str) == Some("login") {
            self.logged_in = v.get("code").and_then(Value::as_i64) == Some(0);
            return self.logged_in;
        }
        let data = v.get("data");
        let num = |k: &str| data.and_then(|d| d.get(k)).and_then(Value::as_i64);
        match v.get("notify").and_then(Value::as_str) {
            Some("song") => {
                let text = |k: &str| {
                    data.and_then(|d| d.get(k))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                let song = Song {
                    title: text("title"),
                    artist: text("singer"),
                    album: text("album"),
                    cover_url: text("picurl"),
                };
                if self.song.as_ref() == Some(&song) {
                    return false;
                }
                self.song = Some(song);
                self.duration_ms = None;
                self.lyrics = None;
                self.lyrics_tried_ms = 0;
                true
            }
            Some("status") => {
                let playing = num("value") == Some(STATUS_PLAYING);
                if playing == self.playing {
                    return false;
                }
                let position = self.position_at(now_ms);
                self.playing = playing;
                self.anchor(position, now_ms);
                true
            }
            Some("progress") => {
                let Some(playtime) = num("playtime") else {
                    return false;
                };
                let duration = num("duration").filter(|d| *d > 0);
                let drifted = (playtime - self.position_at(now_ms)).abs() > DRIFT_MS;
                if !drifted && duration == self.duration_ms {
                    return false;
                }
                self.duration_ms = duration;
                self.anchor(playtime, now_ms);
                true
            }
            _ => false,
        }
    }

    fn state(&self) -> Option<ProviderState> {
        let song = self.song.as_ref().filter(|_| self.logged_in)?;
        Some(ProviderState {
            is_playing: self.playing,
            title: song.title.clone(),
            artist: song.artist.clone(),
            album: Some(song.album.clone()).filter(|a| !a.is_empty()),
            source_app_id: SOURCE_APP_ID.to_string(),
            song_id: None,
            position_ms: self.position_ms,
            anchor_epoch_ms: self.anchor_epoch_ms,
            rate: if self.playing { 1.0 } else { 0.0 },
            duration_ms: self.duration_ms,
            seek_supported: true,
            artwork_url: Some(song.cover_url.clone()).filter(|u| !u.is_empty()),
        })
    }
}

#[derive(Debug)]
pub struct QqPipeProvider {
    shared: Arc<Mutex<Shared>>,
    outbox: Arc<Mutex<VecDeque<String>>>,
    on_change: fn(),
}

impl QqPipeProvider {
    pub fn start(wanted: &'static AtomicBool, on_change: fn()) -> Self {
        let shared: Arc<Mutex<Shared>> = Arc::default();
        let outbox: Arc<Mutex<VecDeque<String>>> = Arc::default();
        let (s, o) = (Arc::clone(&shared), Arc::clone(&outbox));
        std::thread::spawn(move || run(wanted, &s, &o, on_change));
        Self {
            shared,
            outbox,
            on_change,
        }
    }

    fn shared(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn is_connected(&self) -> bool {
        self.shared().logged_in
    }

    fn send(&self, cmd: &str, params: Value) -> bool {
        if !self.is_connected() {
            return false;
        }
        let frame = json!({ "cmd": cmd, "params": params });
        self.outbox
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(frame.to_string());
        true
    }
}

impl MusicProvider for QqPipeProvider {
    fn name(&self) -> &'static str {
        "qqmusic-pipe"
    }

    fn claims(&self) -> &'static str {
        "qqmusic"
    }

    /// 自带完整曲目 不依赖 SMTC
    fn snapshot(&self, _base: Option<&ProviderState>) -> Option<ProviderState> {
        self.shared().state()
    }

    fn lyrics(&self, state: &ProviderState) -> Option<LocalLyrics> {
        let s = self.shared();
        s.song.as_ref().filter(|song| song.title == state.title)?;
        s.lyrics.clone()
    }

    /// 管道只有切换式命令 播放暂停走 SMTC
    fn control(&self, action: Control) -> AppResult<bool> {
        Ok(match action {
            Control::Seek(ms) => {
                let ms = ms.max(0);
                let sent = self.send("seek", json!({ "value": ms }));
                if sent {
                    // 进度推送滞后 先行回写
                    self.shared().anchor(ms, now_epoch_ms());
                    (self.on_change)();
                }
                sent
            }
            Control::Next => self.send("next", json!({})),
            Control::Prev => self.send("prev", json!({})),
            Control::Play | Control::Pause => false,
        })
    }
}

fn run(
    wanted: &AtomicBool,
    shared: &Mutex<Shared>,
    outbox: &Mutex<VecDeque<String>>,
    on_change: fn(),
) {
    loop {
        if wanted.load(Ordering::Acquire) {
            if let Ok(pipe) = File::options().read(true).write(true).open(PIPE) {
                session(pipe, wanted, shared, outbox, on_change);
                *shared.lock().unwrap_or_else(|e| e.into_inner()) = Shared::default();
                outbox.lock().unwrap_or_else(|e| e.into_inner()).clear();
                on_change();
            }
        }
        std::thread::sleep(RETRY);
    }
}

/// 阻塞读会卡住写 先探测再读
fn session(
    mut pipe: File,
    wanted: &AtomicBool,
    shared: &Mutex<Shared>,
    outbox: &Mutex<VecDeque<String>>,
    on_change: fn(),
) {
    if pipe.write_all(LOGIN.as_bytes()).is_err() {
        return;
    }
    let lyric_dir = qrc::cache_dir();
    let mut pending = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    while wanted.load(Ordering::Acquire) {
        if lyric_dir.as_deref().is_some_and(|d| load_lyrics(shared, d)) {
            on_change();
        }
        let queued: Vec<String> = outbox
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
            .collect();
        for frame in queued {
            // 服务端按单次读取解析
            if pipe.write_all(frame.as_bytes()).is_err() {
                return;
            }
        }
        let Some(available) = available(&pipe) else {
            return;
        };
        if available == 0 {
            std::thread::sleep(POLL);
            continue;
        }
        let Ok(n) = pipe.read(&mut chunk) else {
            return;
        };
        pending.extend_from_slice(&chunk[..n]);
        let mut changed = false;
        while let Some(end) = pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = pending.drain(..=end).collect();
            if let Ok(v) = serde_json::from_slice::<Value>(&line) {
                changed |= shared
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .apply(&v, now_epoch_ms());
            }
        }
        if changed {
            on_change();
        }
    }
}

fn load_lyrics(shared: &Mutex<Shared>, dir: &Path) -> bool {
    let now = now_epoch_ms();
    let (song, duration_ms) = {
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        if s.lyrics.is_some() || now - s.lyrics_tried_ms < LYRIC_RETRY_MS {
            return false;
        }
        let (Some(song), Some(d)) = (s.song.clone(), s.duration_ms) else {
            return false;
        };
        s.lyrics_tried_ms = now;
        (song, d)
    };
    let Some((text, lines)) = qrc::find(dir, &song.title, &song.artist, duration_ms) else {
        return false;
    };
    let lyrics = LocalLyrics {
        key: format!("qqmusic:{}", content_hash(text.as_bytes())),
        data: Arc::new(LyricsData { lines, duration_ms }),
    };
    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
    if s.song.as_ref() != Some(&song) {
        return false;
    }
    s.lyrics = Some(lyrics);
    true
}

fn available(pipe: &File) -> Option<u32> {
    let mut n = 0u32;
    unsafe {
        PeekNamedPipe(
            HANDLE(pipe.as_raw_handle()),
            None,
            0,
            None,
            Some(&mut n),
            None,
        )
    }
    .ok()?;
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(s: &mut Shared, json: &str, now_ms: i64) -> bool {
        s.apply(&serde_json::from_str(json).unwrap(), now_ms)
    }

    fn logged_in_with_song() -> Shared {
        let mut s = Shared::default();
        feed(&mut s, r#"{"code":0,"message":"ok","reply":"login"}"#, 0);
        feed(
            &mut s,
            r#"{"data":{"album":"DD backseat","picurl":"https://y.gtimg.cn/x.jpg","singer":"Top Barry/Rapeter","title":"DD backseat"},"notify":"song"}"#,
            0,
        );
        s
    }

    #[test]
    fn login_failure_hides_state() {
        let mut s = Shared::default();
        feed(&mut s, r#"{"code":1,"reply":"login"}"#, 0);
        feed(
            &mut s,
            r#"{"data":{"title":"x","singer":"y"},"notify":"song"}"#,
            0,
        );
        assert!(s.state().is_none());
    }

    #[test]
    fn song_and_progress_build_state() {
        let mut s = logged_in_with_song();
        feed(&mut s, r#"{"data":{"value":4},"notify":"status"}"#, 0);
        feed(
            &mut s,
            r#"{"data":{"duration":196875,"playtime":20433},"notify":"progress"}"#,
            1_000,
        );
        let st = s.state().unwrap();
        assert_eq!(st.title, "DD backseat");
        assert_eq!(st.artist, "Top Barry/Rapeter");
        assert_eq!(st.duration_ms, Some(196_875));
        assert_eq!(st.position_ms, 20_433);
        assert!(!st.is_playing);
        assert!(st.seek_supported);
        assert_eq!(st.source_app_id, SOURCE_APP_ID);
    }

    #[test]
    fn steady_progress_does_not_notify() {
        let mut s = logged_in_with_song();
        feed(&mut s, r#"{"data":{"value":3},"notify":"status"}"#, 0);
        assert!(feed(
            &mut s,
            r#"{"data":{"duration":196875,"playtime":21000},"notify":"progress"}"#,
            1_000,
        ));
        assert!(!feed(
            &mut s,
            r#"{"data":{"duration":196875,"playtime":21330},"notify":"progress"}"#,
            1_320,
        ));
        assert!(feed(
            &mut s,
            r#"{"data":{"duration":196875,"playtime":60851},"notify":"progress"}"#,
            1_640,
        ));
        assert_eq!(s.position_ms, 60_851);
    }

    #[test]
    fn resume_reanchors_at_status_time() {
        let mut s = logged_in_with_song();
        feed(
            &mut s,
            r#"{"data":{"duration":196875,"playtime":20433},"notify":"progress"}"#,
            0,
        );
        feed(&mut s, r#"{"data":{"value":3},"notify":"status"}"#, 30_000);
        assert_eq!(s.position_at(31_000), 21_433);
    }

    #[test]
    fn new_song_drops_old_duration() {
        let mut s = logged_in_with_song();
        feed(
            &mut s,
            r#"{"data":{"duration":196875,"playtime":1000},"notify":"progress"}"#,
            0,
        );
        assert!(feed(
            &mut s,
            r#"{"data":{"title":"林宛瑜","singer":"Rapeter"},"notify":"song"}"#,
            100,
        ));
        assert_eq!(s.duration_ms, None);
    }
}
