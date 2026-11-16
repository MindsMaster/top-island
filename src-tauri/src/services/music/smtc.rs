use std::sync::Mutex;
use std::time::{Duration, Instant};

use island_windows::smtc::{MediaAction, SmtcClient, SmtcSessionInfo};

use super::provider::{now_epoch_ms, Control, ProviderState};

/// sessions 是阻塞 WinRT 调用
const SNAPSHOT_TTL: Duration = Duration::from_millis(900);

pub static SMTC: Smtc = Smtc::new();

#[derive(Debug)]
pub struct Smtc {
    client: Mutex<SmtcClient>,
    cache: Mutex<Option<(Instant, Vec<ProviderState>)>>,
}

impl Smtc {
    const fn new() -> Self {
        Self {
            client: Mutex::new(SmtcClient::new()),
            cache: Mutex::new(None),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SmtcClient> {
        self.client.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn cached(&self) -> Option<(Instant, Vec<ProviderState>)> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn sessions(&self) -> Vec<ProviderState> {
        if let Some((at, sessions)) = self.cached() {
            if at.elapsed() < SNAPSHOT_TTL {
                return sessions;
            }
        }
        // 持锁方可能在阻塞 回旧缓存
        let Ok(mut client) = self.client.try_lock() else {
            return self.cached().map(|(_, s)| s).unwrap_or_default();
        };
        let sessions: Vec<ProviderState> = client
            .sessions()
            .into_iter()
            .filter_map(state_from)
            .collect();
        drop(client);
        *self.cache.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((Instant::now(), sessions.clone()));
        sessions
    }

    pub fn control(&self, app: &str, action: Control) -> bool {
        let mut client = self.lock();
        let action = match action {
            Control::Seek(ms) => return client.seek(app, ms.max(0)).is_ok(),
            Control::Play => MediaAction::Play,
            Control::Pause => MediaAction::Pause,
            Control::Next => MediaAction::Next,
            Control::Prev => MediaAction::Prev,
        };
        client.control(app, action);
        true
    }

    pub fn artwork(&self, app: &str) -> Option<Vec<u8>> {
        self.lock().thumbnail(app)
    }
}

fn state_from(info: SmtcSessionInfo) -> Option<ProviderState> {
    if info.title.is_empty() && info.app.is_empty() {
        return None;
    }
    let has_timeline = info.duration_ms > 0;
    Some(ProviderState {
        is_playing: info.playing,
        title: info.title,
        artist: info.artist,
        album: None,
        source_app_id: info.app,
        song_id: None,
        position_ms: if has_timeline { info.position_ms } else { 0 },
        anchor_epoch_ms: now_epoch_ms(),
        rate: if info.playing && has_timeline {
            1.0
        } else {
            0.0
        },
        duration_ms: has_timeline.then_some(info.duration_ms),
        seek_supported: info.seek_supported && has_timeline,
        artwork_url: None,
    })
}
