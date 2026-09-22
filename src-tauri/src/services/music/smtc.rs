use std::sync::Mutex;
use std::time::{Duration, Instant};

use island_windows::smtc::{MediaAction, SmtcClient};

use super::provider::{now_epoch_ms, Capabilities, Control, MusicProvider, ProviderState};
use crate::error::AppResult;

/// query 是阻塞 WinRT 调用
const SNAPSHOT_TTL: Duration = Duration::from_millis(900);

#[derive(Debug)]
pub struct SmtcProvider {
    client: Mutex<SmtcClient>,
    cache: Mutex<Option<(Instant, Option<ProviderState>)>>,
}

impl SmtcProvider {
    pub const fn new() -> Self {
        Self {
            client: Mutex::new(SmtcClient::new()),
            cache: Mutex::new(None),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SmtcClient> {
        self.client.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn state_from(info: Option<island_windows::smtc::SmtcSessionInfo>) -> Option<ProviderState> {
        let info = info?;
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
}

impl MusicProvider for SmtcProvider {
    fn name(&self) -> &'static str {
        "smtc"
    }

    fn snapshot(&self) -> Option<ProviderState> {
        {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((at, state)) = cache.as_ref() {
                if at.elapsed() < SNAPSHOT_TTL {
                    return state.clone();
                }
            }
        }
        // 持锁方可能在阻塞 回旧缓存
        let Ok(mut client) = self.client.try_lock() else {
            return self
                .cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .and_then(|(_, s)| s.clone());
        };
        let state = Self::state_from(client.query());
        drop(client);
        *self.cache.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((Instant::now(), state.clone()));
        state
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            skip: true,
            artwork_bitmap: true,
            fallback: true,
        }
    }

    fn control(&self, action: Control) -> AppResult<bool> {
        let mut client = self.lock();
        Ok(match action {
            Control::Seek(ms) => client.seek(ms.max(0)).is_ok(),
            Control::Play => {
                client.control(MediaAction::Play);
                true
            }
            Control::Pause => {
                client.control(MediaAction::Pause);
                true
            }
            Control::Next => {
                client.control(MediaAction::Next);
                true
            }
            Control::Prev => {
                client.control(MediaAction::Prev);
                true
            }
        })
    }

    fn artwork_bytes(&self) -> Option<Vec<u8>> {
        self.lock().thumbnail()
    }
}
