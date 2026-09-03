//! 音乐域状态聚合：SMTC 快照 + 外部位置源（网易云 elog）+ 封面缓存 + 歌词缓存。
//! 移植自 electron/main/services/music.ts，渲染层契约不变：
//! - music_poll 返回聚合 MusicState；SMTC 事件驱动时 emit 'music:state'
//! - artwork/lyrics 按 hash/id 按值拉取（hash 或 id 不匹配说明已切歌，返回 None）

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once};
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use island_core::{AppSettings, LyricsData, MusicAction, MusicArtwork, MusicState};
use island_windows::smtc::{MediaAction, SmtcClient};

use crate::error::AppResult;

mod b64;
mod elog;
mod hash;
mod lyrics;
mod media_sources;

#[derive(Debug)]
struct ArtworkCache {
    track_key: String,
    hash: String,
    data_url: String,
}

#[derive(Debug)]
struct LyricsCache {
    track_key: String,
    data: LyricsData,
    /// 是否按平台歌曲 ID 精确获取——只有此时时长才可作权威时间轴
    by_id: bool,
}

#[derive(Debug)]
struct Inner {
    client: SmtcClient,
    elog: elog::NeteaseElog,
    artwork: Option<ArtworkCache>,
    artwork_fetching: Option<String>,
    last_track_key: String,
    lyrics: Option<LyricsCache>,
    lyrics_fetching: Option<String>,
    /// 歌词缓存键与 SMTC 曲目键解耦：网易云用 elog songId（精确），其余用曲目 key（搜索）
    last_lyrics_key: String,
}

static INNER: Mutex<Inner> = Mutex::new(Inner {
    client: SmtcClient::new(),
    elog: elog::NeteaseElog::new(),
    artwork: None,
    artwork_fetching: None,
    last_track_key: String::new(),
    lyrics: None,
    lyrics_fetching: None,
    last_lyrics_key: String::new(),
});

static ENABLED: AtomicBool = AtomicBool::new(false);
static PUSH_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static LAST_PUSHED: Mutex<Option<String>> = Mutex::new(None);

fn lock_inner() -> std::sync::MutexGuard<'static, Inner> {
    INNER.lock().unwrap_or_else(|e| e.into_inner())
}

fn track_key_of(title: &str, artist: &str, app: &str) -> String {
    format!("{title}|{artist}|{app}")
}

/// 生命周期同步：diagnostics.music_poll 开时启动（幂等）SMTC 事件监听并推送
/// 'music:state'；关时门控推送。监听线程全进程只起一次——WinRT 事件订阅无法
/// 干净拆除，开关只决定回调里算不算状态、推不推。
pub fn sync(app: &AppHandle, settings: &AppSettings) {
    let enabled = settings.diagnostics.music_poll;
    ENABLED.store(enabled, Ordering::Relaxed);
    if enabled {
        static START: Once = Once::new();
        START.call_once(|| {
            let app = app.clone();
            island_windows::smtc::start_watch(move || push_state(&app));
        });
    }
}

/// SMTC 事件驱动：立刻重算完整状态推给岛（渲染层 2s 轮询保留作兜底 +
/// 驱动网易云 elog 进度等非事件源）。内容去重：事件常成串重复。
fn push_state(app: &AppHandle) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    if PUSH_IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return;
    }
    let state = poll_state();
    let json = serde_json::to_string(&state).unwrap_or_default();
    {
        let mut last = LAST_PUSHED.lock().unwrap_or_else(|e| e.into_inner());
        if last.as_deref() == Some(json.as_str()) {
            PUSH_IN_FLIGHT.store(false, Ordering::SeqCst);
            return;
        }
        *last = Some(json);
    }
    if let Err(e) = app.emit("music:state", &state) {
        eprintln!("[music] 推送 music:state 失败: {e}");
    }
    PUSH_IN_FLIGHT.store(false, Ordering::SeqCst);
}

pub fn poll_state() -> MusicState {
    // spawn_*_fetch 入口要再抢 INNER，持锁调用会自死锁（std Mutex 不可重入），
    // 先在锁内记下要抓什么，出锁后再 spawn
    let mut need_artwork: Option<String> = None;
    let mut need_lyrics: Option<(String, String, String, Option<String>)> = None;
    let mut result = MusicState::default();

    {
        let mut inner = lock_inner();
        let Some(smtc) = inner.client.query() else {
            inner.last_track_key.clear();
            inner.last_lyrics_key.clear();
            return result;
        };
        let title = smtc.title;
        let artist = smtc.artist;
        let source = smtc.app;
        if title.is_empty() && source.is_empty() {
            inner.last_track_key.clear();
            inner.last_lyrics_key.clear();
            return result;
        }

        result.is_playing = smtc.playing;
        let track = if !title.is_empty() {
            title.clone()
        } else {
            // 无标题的源退化为来源标识（对齐 Electron：取 AUMID 最后一段或末尾 30 字符）
            let tail = if source.contains('.') {
                source.rsplit('.').next().unwrap_or(&source).to_string()
            } else {
                source.chars().rev().take(30).collect::<Vec<_>>().into_iter().rev().collect()
            };
            format!("SMTC: {tail}")
        };
        result.track = Some(track);
        if !artist.is_empty() {
            result.artist = Some(artist.clone());
        }
        result.source_app_id = Some(source.clone());
        result.position_ms = Some(smtc.position_ms);
        result.duration_ms = Some(smtc.duration_ms);
        result.seek_supported = Some(smtc.seek_supported);

        let key = track_key_of(&title, &artist, &source);
        if key != inner.last_track_key {
            inner.last_track_key = key.clone();
            need_artwork = Some(key.clone());
        }
        if let Some(art) = &inner.artwork {
            if art.track_key == key {
                result.artwork_hash = Some(art.hash.clone());
            }
        }

        // SMTC 无时间轴的源（网易云等）：外部位置源补真实进度；
        // 源能给出 songId 时歌词/时长按 ID 精确获取
        let mut lyrics_key = key.clone();
        let mut by_id_song: Option<String> = None;
        if result.duration_ms == Some(0) {
            if let Some(ext) = inner.elog.poll_if_matches(&source) {
                result.position_ms = Some(ext.position_ms);
                // 网易云 SMTC PlaybackStatus 冻结/滞后不可信，播放态以 elog 为准
                // （否则 UI 播放/暂停按钮显示反相，点按发出错误动词成为无操作）
                result.is_playing = ext.playing;
                if ext.duration_ms > 0 {
                    result.duration_ms = Some(ext.duration_ms);
                }
                if let Some(song_id) = ext.song_id {
                    lyrics_key = format!("163:{song_id}");
                    by_id_song = Some(song_id);
                }
            }
        }

        if lyrics_key != inner.last_lyrics_key {
            inner.last_lyrics_key = lyrics_key.clone();
            // 只对已知音乐播放器查歌词（songId 精确获取不受白名单限制——能给出
            // songId 说明外部位置源已确认是网易云）
            if by_id_song.is_some() || media_sources::lyrics_supported(&source) {
                need_lyrics = Some((lyrics_key.clone(), title.clone(), artist.clone(), by_id_song));
            }
        }
        if let Some(cache) = &inner.lyrics {
            if cache.track_key == lyrics_key {
                if !cache.data.lines.is_empty() {
                    result.lyrics_id = Some(lyrics_key.clone());
                }
                if result.duration_ms == Some(0) && cache.data.duration_ms > 0 {
                    // 按 ID 取得的时长是权威值，与外部位置源构成完整时间轴（渲染层可回同步）；
                    // 搜索得到的时长可能是错误版本，仅作估算展示
                    if cache.by_id {
                        result.duration_ms = Some(cache.data.duration_ms);
                    } else {
                        result.estimated_duration_ms = Some(cache.data.duration_ms);
                    }
                }
            }
        }
    }

    if let Some(key) = need_artwork {
        spawn_artwork_fetch(key);
    }
    if let Some((key, title, artist, song_id)) = need_lyrics {
        spawn_lyrics_fetch(key, title, artist, song_id);
    }
    result
}

/// 后台抓封面，带重试；曲目已切换则放弃
fn spawn_artwork_fetch(track_key: String) {
    {
        let mut inner = lock_inner();
        if inner.artwork_fetching.as_deref() == Some(track_key.as_str()) {
            return;
        }
        inner.artwork_fetching = Some(track_key.clone());
    }
    let track_key_for_retry = track_key.clone();
    let spawned = std::thread::Builder::new()
        .name("music-artwork".into())
        .spawn(move || {
            // 切歌后播放器填充封面有延迟，先等一拍再开始抓
            std::thread::sleep(Duration::from_millis(800));
            for attempt in 0..6 {
                let done = {
                    let mut inner = lock_inner();
                    if inner.last_track_key != track_key {
                        true // 已切歌
                    } else {
                        match inner.client.thumbnail() {
                            Some(bytes) => {
                                let data_url = format!("data:{};base64,{}", sniff_mime(&bytes), b64::encode(&bytes));
                                inner.artwork = Some(ArtworkCache {
                                    track_key: track_key.clone(),
                                    hash: hash::content_hash(&bytes),
                                    data_url,
                                });
                                true
                            }
                            None => false,
                        }
                    }
                };
                if done {
                    break;
                }
                std::thread::sleep(Duration::from_millis(if attempt < 3 { 500 } else { 1000 }));
            }
            let mut inner = lock_inner();
            if inner.artwork_fetching.as_deref() == Some(track_key.as_str()) {
                inner.artwork_fetching = None;
            }
        });
    if let Err(e) = spawned {
        eprintln!("[music] 封面抓取线程启动失败: {e}");
        // 线程没起来，清掉占位标志，否则同曲目永远不会重试
        let mut inner = lock_inner();
        if inner.artwork_fetching.as_deref() == Some(track_key_for_retry.as_str()) {
            inner.artwork_fetching = None;
        }
    }
}

fn sniff_mime(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, 0x50, ..] => "image/png",
        [0xFF, 0xD8, ..] => "image/jpeg",
        [0x47, 0x49, ..] => "image/gif",
        [0x42, 0x4D, ..] => "image/bmp",
        _ => "image/jpeg",
    }
}

/// 后台取歌词。按 ID 失败回退搜索：时长从权威降级为估算，
/// 故缓存需自带 by_id 标记
fn spawn_lyrics_fetch(lyrics_key: String, title: String, artist: String, song_id: Option<String>) {
    {
        let mut inner = lock_inner();
        if inner.lyrics_fetching.as_deref() == Some(lyrics_key.as_str()) {
            return;
        }
        inner.lyrics_fetching = Some(lyrics_key.clone());
    }
    let lyrics_key_for_retry = lyrics_key.clone();
    let spawned = std::thread::Builder::new()
        .name("music-lyrics".into())
        .spawn(move || {
            let fetched: Option<(LyricsData, bool)> = match &song_id {
                Some(sid) => match lyrics::fetch_163_by_id(sid) {
                    Some(data) => Some((data, true)),
                    None => lyrics::fetch_lyrics(&title, &artist).map(|d| (d, false)),
                },
                None => lyrics::fetch_lyrics(&title, &artist).map(|d| (d, false)),
            };
            let mut inner = lock_inner();
            if let Some((data, by_id)) = fetched {
                if inner.last_lyrics_key == lyrics_key {
                    inner.lyrics = Some(LyricsCache { track_key: lyrics_key.clone(), data, by_id });
                }
            }
            if inner.lyrics_fetching.as_deref() == Some(lyrics_key.as_str()) {
                inner.lyrics_fetching = None;
            }
        });
    if let Err(e) = spawned {
        eprintln!("[music] 歌词抓取线程启动失败: {e}");
        // 线程没起来，清掉占位标志，否则同曲目永远不会重试
        let mut inner = lock_inner();
        if inner.lyrics_fetching.as_deref() == Some(lyrics_key_for_retry.as_str()) {
            inner.lyrics_fetching = None;
        }
    }
}

/// 按 hash 取当前曲目封面；hash 不匹配（已切歌）返回 None
pub fn artwork(hash: &str) -> Option<MusicArtwork> {
    let inner = lock_inner();
    let art = inner.artwork.as_ref()?;
    if art.hash == hash {
        Some(MusicArtwork { hash: art.hash.clone(), data_url: art.data_url.clone() })
    } else {
        None
    }
}

/// 按 lyricsId 取当前曲目歌词；id 不匹配（已切歌）返回 None
pub fn lyrics(id: &str) -> Option<LyricsData> {
    let inner = lock_inner();
    let cache = inner.lyrics.as_ref()?;
    if cache.track_key == id {
        Some(cache.data.clone())
    } else {
        None
    }
}

pub fn seek(position_ms: i64) -> bool {
    let mut inner = lock_inner();
    match inner.client.seek(position_ms.max(0)) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("[music] seek 失败: {e}");
            false
        }
    }
}

pub fn control(action: MusicAction, level: Option<i64>) -> AppResult<String> {
    match action {
        MusicAction::Play | MusicAction::Pause | MusicAction::Next | MusicAction::Prev => {
            let media_action = match action {
                MusicAction::Play => MediaAction::Play,
                MusicAction::Pause => MediaAction::Pause,
                MusicAction::Next => MediaAction::Next,
                MusicAction::Prev => MediaAction::Prev,
                MusicAction::Volume => unreachable!("volume 已在上面分支排除"),
            };
            let mut inner = lock_inner();
            inner.client.control(media_action);
            drop(inner);
            Ok(match action {
                MusicAction::Play => "已发送播放指令。".to_string(),
                MusicAction::Pause => "已发送暂停指令。".to_string(),
                MusicAction::Next => "已切换到下一首。".to_string(),
                MusicAction::Prev => "已切换到上一首。".to_string(),
                MusicAction::Volume => unreachable!(),
            })
        }
        MusicAction::Volume => {
            let lvl = level.unwrap_or(50).clamp(0, 100) as f32 / 100.0;
            match island_windows::coreaudio::set_master_volume_scalar(lvl) {
                Ok(()) => Ok(format!("音量已设置为 {}%", (lvl * 100.0).round() as i64)),
                Err(e) => {
                    eprintln!("[music] 调整音量失败: {e}");
                    Ok("无法调整音量。".to_string())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_mime_detects_common_image_formats() {
        assert_eq!(sniff_mime(&[0x89, 0x50, 0x4E, 0x47]), "image/png", "PNG 魔数 89 50 应识别为 png");
        assert_eq!(sniff_mime(&[0xFF, 0xD8, 0xFF]), "image/jpeg", "JPEG 魔数 FF D8 应识别为 jpeg");
        assert_eq!(sniff_mime(&[0x47, 0x49, 0x46]), "image/gif", "GIF 魔数 47 49 应识别为 gif");
        assert_eq!(sniff_mime(&[0x42, 0x4D, 0x00]), "image/bmp", "BMP 魔数 42 4D 应识别为 bmp");
        assert_eq!(sniff_mime(&[0x00, 0x01]), "image/jpeg", "未知格式回退 jpeg（SMTC 封面绝大多数是 jpeg）");
    }

    #[test]
    fn track_key_combines_title_artist_and_app() {
        assert_eq!(
            track_key_of("晴天", "周杰伦", "qqmusic.exe"),
            "晴天|周杰伦|qqmusic.exe",
            "曲目键应为 标题|艺术家|来源，错了切歌检测会失效"
        );
    }
}
