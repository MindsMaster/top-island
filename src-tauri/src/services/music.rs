use std::sync::{Mutex, Once, OnceLock};

use tauri::AppHandle;

use island_core::{AppSettings, LyricsData, MusicAction, MusicState};

use crate::error::AppResult;
use crate::services::media::Media;

mod apps;
mod arbiter;
mod bridge_server;
mod catalog;
mod hash;
mod integration;
mod kugou;
mod lyrics;
mod ncm;
mod provider;
mod push;
mod qqmusic;
mod resolver;
mod sideload;
mod smtc;

pub use integration::{GrantResult, IntegrationInfo};

use arbiter::Arbiter;
use catalog::SongRef;
use integration::Integration;
use provider::{now_epoch_ms, Control, MusicProvider, ProviderState};
use push::PushSignal;
use resolver::Resolver;
use smtc::SMTC;

/// 每个来源应用一条 增强器认领同名 SMTC 会话
struct Session {
    state: ProviderState,
    enhancer: Option<&'static dyn MusicProvider>,
}

struct Service {
    enhancers: Vec<&'static dyn MusicProvider>,
    resolver: &'static Resolver,
    arbiter: Mutex<Arbiter>,
    /// 控制发往当前上岛的会话
    shown: Mutex<Option<(String, Option<&'static dyn MusicProvider>)>>,
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service").finish_non_exhaustive()
    }
}

static SERVICE: OnceLock<Service> = OnceLock::new();
static INTEGRATIONS: &[&dyn Integration] = &[&ncm::NETEASE, &kugou::KUGOU, &qqmusic::QQMUSIC];
static PUSH: PushSignal = PushSignal::new();

fn request_push() {
    PUSH.request();
}

pub fn sync(app: &AppHandle, settings: &AppSettings) {
    let enabled = settings.diagnostics.music_poll;
    PUSH.set_enabled(enabled);
    if enabled {
        static START: Once = Once::new();
        START.call_once(|| init(app.clone()));
    }
    // 关着时连不上 不部署
    for integration in INTEGRATIONS {
        integration.set_wanted(enabled && integration.enabled(&settings.music));
    }
}

fn init(app: AppHandle) {
    let resolver: &'static Resolver = Box::leak(Box::new(Resolver::new(request_push)));
    let enhancers = INTEGRATIONS
        .iter()
        .map(|i| i.provider(request_push))
        .collect();
    let _ = SERVICE.set(Service {
        enhancers,
        resolver,
        arbiter: Mutex::new(Arbiter::default()),
        shown: Mutex::new(None),
    });
    push::start(app, &PUSH, "music:state", current_state);
    island_windows::smtc::start_watch(request_push);
}

pub fn integrations() -> Vec<IntegrationInfo> {
    INTEGRATIONS
        .iter()
        .map(|i| IntegrationInfo {
            id: i.id(),
            status: i.status(),
        })
        .collect()
}

pub fn grant_integration(id: &str) -> GrantResult {
    INTEGRATIONS
        .iter()
        .find(|i| i.id() == id)
        .map_or(GrantResult::Failed, |i| i.grant())
}

pub fn current_state() -> MusicState {
    let Some(svc) = SERVICE.get() else {
        return MusicState::default();
    };

    let mut sessions = gather(&svc.enhancers, SMTC.sessions());
    let keys: Vec<(&str, bool)> = sessions
        .iter()
        .map(|s| (s.state.source_app_id.as_str(), s.state.is_playing))
        .collect();
    let picked = svc
        .arbiter
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .pick(&keys, now_epoch_ms());
    let mut shown = svc.shown.lock().unwrap_or_else(|e| e.into_inner());
    let Some(i) = picked else {
        *shown = None;
        svc.resolver.reset();
        return MusicState::default();
    };
    let Session {
        state: src,
        enhancer,
    } = sessions.swap_remove(i);
    *shown = Some((src.source_app_id.clone(), enhancer));
    drop(shown);

    let local = enhancer.and_then(|e| e.lyrics(&src));
    let r = svc.resolver.observe(&src, local);
    let track = if r.title.is_empty() {
        fallback_track(&src.source_app_id)
    } else {
        r.title
    };
    MusicState {
        provider: enhancer.map_or("smtc", |e| e.name()).to_string(),
        is_playing: src.is_playing,
        track: Some(track),
        artist: (!r.artist.is_empty()).then_some(r.artist),
        album: r.album,
        source_app_id: Some(src.source_app_id.clone()),
        song_id: src.song_id.as_ref().map(SongRef::key),
        position_ms: src.position_ms,
        anchor_epoch_ms: src.anchor_epoch_ms,
        rate: src.rate,
        duration_ms: r.duration_ms,
        seek_supported: src.seek_supported,
        artwork_url: r.artwork_url,
        artwork_hash: r.artwork_hash,
        lyrics_id: r.lyrics_id,
        lyric_offset_ms: apps::lyric_offset_ms(&src.source_app_id),
    }
}

fn gather(enhancers: &[&'static dyn MusicProvider], mut bases: Vec<ProviderState>) -> Vec<Session> {
    let mut out = Vec::new();
    for &e in enhancers {
        let idx = bases
            .iter()
            .position(|b| b.source_app_id.to_lowercase().contains(e.claims()));
        let Some(mut state) = e.snapshot(idx.map(|i| &bases[i])) else {
            continue;
        };
        if let Some(i) = idx {
            state.source_app_id = bases.swap_remove(i).source_app_id;
        }
        out.push(Session {
            state,
            enhancer: Some(e),
        });
    }
    out.extend(bases.into_iter().map(|state| Session {
        state,
        enhancer: None,
    }));
    out
}

fn fallback_track(source_app_id: &str) -> String {
    let tail = if source_app_id.contains('.') {
        source_app_id
            .rsplit('.')
            .next()
            .unwrap_or(source_app_id)
            .to_string()
    } else {
        source_app_id
            .chars()
            .rev()
            .take(30)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    };
    format!("SMTC: {tail}")
}

/// hash 不符即切歌
pub fn artwork(hash: &str) -> Option<Media> {
    SERVICE.get()?.resolver.artwork(hash)
}

/// id 不符即切歌
pub fn lyrics(id: &str) -> Option<LyricsData> {
    SERVICE.get()?.resolver.lyrics(id)
}

pub fn seek(position_ms: i64) -> bool {
    route(Control::Seek(position_ms.max(0)))
}

pub fn control(action: MusicAction, level: Option<i64>) -> AppResult<String> {
    let msg = match action {
        MusicAction::Play => {
            route(Control::Play);
            "已发送播放指令。"
        }
        MusicAction::Pause => {
            route(Control::Pause);
            "已发送暂停指令。"
        }
        MusicAction::Next => {
            route(Control::Next);
            "已切换到下一首。"
        }
        MusicAction::Prev => {
            route(Control::Prev);
            "已切换到上一首。"
        }
        MusicAction::Volume => {
            let lvl = level.unwrap_or(50).clamp(0, 100) as f32 / 100.0;
            return match island_windows::coreaudio::set_master_volume_scalar(lvl) {
                Ok(()) => Ok(format!("音量已设置为 {}%", (lvl * 100.0).round() as i64)),
                Err(e) => {
                    eprintln!("[music] 调整音量失败: {e}");
                    Ok("无法调整音量。".to_string())
                }
            };
        }
    };
    Ok(msg.to_string())
}

/// 增强器做不了的改走同一来源的 SMTC
fn route(action: Control) -> bool {
    let Some(svc) = SERVICE.get() else {
        return false;
    };
    let Some((app, enhancer)) = svc.shown.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
        return false;
    };
    if let Some(e) = enhancer {
        match e.control(action) {
            Ok(true) => return true,
            Ok(false) => {}
            Err(err) => eprintln!("[music] {} 控制失败，改走 SMTC: {err}", e.name()),
        }
    }
    SMTC.control(&app, action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_track_uses_aumid_tail_or_last_30_chars() {
        assert_eq!(fallback_track("cloudmusic.exe"), "SMTC: exe");
        let long = "a".repeat(40);
        assert_eq!(fallback_track(&long), format!("SMTC: {}", "a".repeat(30)));
    }
}
