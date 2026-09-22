use std::sync::{Mutex, Once, OnceLock};

use tauri::AppHandle;

use island_core::{AppSettings, LyricsData, MusicAction, MusicArtwork, MusicState};

use crate::error::AppResult;

mod b64;
mod hash;
mod lyrics;
mod media_sources;
mod ncm_bridge;
mod ncm_deploy;
mod provider;
mod push;
mod resolver;
mod smtc;

pub use ncm_deploy::BridgeStatus;

use ncm_bridge::NcmBridgeProvider;
use provider::{Control, MusicProvider};
use push::PushSignal;
use resolver::Resolver;
use smtc::SmtcProvider;

struct Service {
    /// 高到低 首个生效
    providers: Vec<&'static dyn MusicProvider>,
    bridge: &'static NcmBridgeProvider,
    resolver: &'static Resolver,
    active: Mutex<&'static dyn MusicProvider>,
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service").finish_non_exhaustive()
    }
}

static SERVICE: OnceLock<Service> = OnceLock::new();
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
    ncm_deploy::set_wanted(enabled && settings.music.netease_bridge);
}

fn init(app: AppHandle) {
    let smtc: &'static SmtcProvider = Box::leak(Box::new(SmtcProvider::new()));
    let bridge: &'static NcmBridgeProvider =
        Box::leak(Box::new(NcmBridgeProvider::start(request_push)));
    let resolver: &'static Resolver = Box::leak(Box::new(Resolver::new(request_push)));
    let _ = SERVICE.set(Service {
        providers: vec![bridge, smtc],
        bridge,
        resolver,
        active: Mutex::new(smtc),
    });
    push::start(app, &PUSH, "music:state", poll_state);
    island_windows::smtc::start_watch(request_push);
}

pub fn bridge_status() -> BridgeStatus {
    let connected = SERVICE
        .get()
        .map(|s| s.bridge.is_connected())
        .unwrap_or(false);
    ncm_deploy::status(connected)
}

pub fn poll_state() -> MusicState {
    let Some(svc) = SERVICE.get() else {
        return MusicState::default();
    };

    let Some((provider, src)) = svc
        .providers
        .iter()
        .find_map(|p| p.snapshot().map(|s| (*p, s)))
    else {
        svc.resolver.reset();
        return MusicState::default();
    };
    *svc.active.lock().unwrap_or_else(|e| e.into_inner()) = provider;

    let r = svc.resolver.observe(&src, provider);
    let track = if r.title.is_empty() {
        fallback_track(&src.source_app_id)
    } else {
        r.title
    };
    MusicState {
        provider: provider.name().to_string(),
        is_playing: src.is_playing,
        track: Some(track),
        artist: (!r.artist.is_empty()).then_some(r.artist),
        album: r.album,
        source_app_id: Some(src.source_app_id.clone()),
        song_id: src.song_id.clone(),
        position_ms: src.position_ms,
        anchor_epoch_ms: src.anchor_epoch_ms,
        rate: src.rate,
        duration_ms: r.duration_ms,
        seek_supported: src.seek_supported,
        artwork_url: r.artwork_url,
        artwork_hash: r.artwork_hash,
        lyrics_id: r.lyrics_id,
    }
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
pub fn artwork(hash: &str) -> Option<MusicArtwork> {
    let (hash, data_url) = SERVICE.get()?.resolver.artwork(hash)?;
    Some(MusicArtwork { hash, data_url })
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

/// 未送达退 fallback
fn route(action: Control) -> bool {
    let Some(svc) = SERVICE.get() else {
        return false;
    };
    let active: &'static dyn MusicProvider = *svc.active.lock().unwrap_or_else(|e| e.into_inner());
    let fallback = svc
        .providers
        .iter()
        .copied()
        .find(|p| p.capabilities().fallback);

    let capable = !action.needs_skip() || active.capabilities().skip;
    if capable {
        match active.control(action) {
            Ok(true) => return true,
            Ok(false) => {}
            Err(e) => eprintln!("[music] {} 控制失败，回退: {e}", active.name()),
        }
    }
    let is_active = |f: &&'static dyn MusicProvider| std::ptr::addr_eq(*f, active);
    match fallback.filter(|f| !is_active(f)) {
        Some(f) => f.control(action).unwrap_or_else(|e| {
            eprintln!("[music] {} 控制失败: {e}", f.name());
            false
        }),
        None => false,
    }
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
