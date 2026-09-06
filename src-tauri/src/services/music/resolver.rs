//! 把 provider 的原始状态补全成可展示的曲目信息（元数据、歌词、封面），资源就绪时通知推送线程。
//! 每种资源一个 `Slot`：键、值、正在抓的键。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use island_core::LyricsData;

use super::b64;
use super::hash::content_hash;
use super::lyrics as api;
use super::media_sources;
use super::provider::{MusicProvider, ProviderState, TrackMeta};

#[derive(Debug)]
struct Slot<T> {
    /// 最近决定要抓的键，也是切歌检测水位
    key: String,
    value: Option<(String, T)>,
    fetching: Option<String>,
}

// 派生 Default 会给 T 加 Default 约束
impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self { key: String::new(), value: None, fetching: None }
    }
}

impl<T> Slot<T> {
    fn get(&self, key: &str) -> Option<&T> {
        self.value.as_ref().filter(|(k, _)| k == key).map(|(_, v)| v)
    }
}

#[derive(Debug)]
pub struct Artwork {
    pub hash: String,
    pub data_url: String,
}

#[derive(Debug, Default)]
struct Caches {
    meta: Slot<TrackMeta>,
    lyrics: Slot<LyricsData>,
    artwork: Slot<Artwork>,
}

#[derive(Debug, Default)]
pub struct Resolved {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub artwork_url: Option<String>,
    pub artwork_hash: Option<String>,
    pub duration_ms: Option<i64>,
    pub lyrics_id: Option<String>,
}

pub struct Resolver {
    caches: Mutex<Caches>,
    on_ready: Arc<dyn Fn() + Send + Sync>,
}

impl std::fmt::Debug for Resolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolver").finish_non_exhaustive()
    }
}

impl Resolver {
    pub fn new(on_ready: impl Fn() + Send + Sync + 'static) -> Self {
        Self { caches: Mutex::new(Caches::default()), on_ready: Arc::new(on_ready) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Caches> {
        self.caches.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn reset(&self) {
        let mut c = self.lock();
        c.meta.key.clear();
        c.lyrics.key.clear();
        c.artwork.key.clear();
    }

    /// 用缓存补全展示字段，缺的资源在后台发起抓取
    pub fn observe(&'static self, src: &ProviderState, provider: &'static dyn MusicProvider) -> Resolved {
        let mut out = Resolved {
            title: src.title.clone(),
            artist: src.artist.clone(),
            album: src.album.clone(),
            artwork_url: src.artwork_url.clone(),
            duration_ms: src.duration_ms,
            ..Default::default()
        };

        // spawn 内部要再拿锁，出锁后再 spawn
        let mut fetch_meta: Option<String> = None;
        let mut fetch_lyrics: Option<(String, String, String, Option<String>)> = None;
        let mut fetch_artwork: Option<String> = None;

        {
            let mut c = self.lock();

            match &src.song_id {
                Some(id) => {
                    if *id != c.meta.key {
                        c.meta.key = id.clone();
                        fetch_meta = Some(id.clone());
                    }
                    if let Some(m) = c.meta.get(id) {
                        if out.title.is_empty() {
                            out.title = m.title.clone();
                        }
                        if out.artist.is_empty() {
                            out.artist = m.artist.clone();
                        }
                        if out.album.is_none() && !m.album.is_empty() {
                            out.album = Some(m.album.clone());
                        }
                        if out.artwork_url.is_none() && !m.cover_url.is_empty() {
                            out.artwork_url = Some(m.cover_url.clone());
                        }
                        if out.duration_ms.is_none() && m.duration_ms > 0 {
                            out.duration_ms = Some(m.duration_ms);
                        }
                    }
                }
                None => c.meta.key.clear(),
            }

            // 键按补全后的标题算：标题晚到时若用空标题记键，之后就不会再抓
            let track_key = format!("{}|{}|{}", out.title, out.artist, src.source_app_id);
            let lyrics_key = match &src.song_id {
                Some(id) => format!("163:{id}"),
                None => track_key.clone(),
            };

            if out.artwork_url.is_none() && provider.capabilities().artwork_bitmap {
                if track_key != c.artwork.key {
                    c.artwork.key = track_key.clone();
                    fetch_artwork = Some(track_key.clone());
                }
                if let Some(a) = c.artwork.get(&track_key) {
                    out.artwork_hash = Some(a.hash.clone());
                }
            } else {
                c.artwork.key = track_key.clone();
            }

            let can_fetch = src.song_id.is_some() || !out.title.is_empty();
            if lyrics_key != c.lyrics.key && can_fetch {
                c.lyrics.key = lyrics_key.clone();
                if src.song_id.is_some() || media_sources::lyrics_supported(&src.source_app_id) {
                    fetch_lyrics = Some((
                        lyrics_key.clone(),
                        out.title.clone(),
                        out.artist.clone(),
                        src.song_id.clone(),
                    ));
                }
            }
            if let Some(l) = c.lyrics.get(&lyrics_key) {
                if !l.lines.is_empty() {
                    out.lyrics_id = Some(lyrics_key.clone());
                }
            }
        }

        if let Some(id) = fetch_meta {
            self.spawn_meta(id);
        }
        if let Some((k, t, a, sid)) = fetch_lyrics {
            self.spawn_lyrics(k, t, a, sid);
        }
        if let Some(k) = fetch_artwork {
            self.spawn_artwork(k, provider);
        }
        out
    }

    pub fn artwork(&self, hash: &str) -> Option<(String, String)> {
        let c = self.lock();
        let (_, a) = c.artwork.value.as_ref()?;
        (a.hash == hash).then(|| (a.hash.clone(), a.data_url.clone()))
    }

    pub fn lyrics(&self, id: &str) -> Option<LyricsData> {
        self.lock().lyrics.get(id).cloned()
    }

    /// 后台跑 `work`，结果仍匹配当前键则写入并通知；失败清键，下次 observe 重试
    fn fetch_once<T: Send + 'static>(
        &'static self,
        thread_name: &'static str,
        key: String,
        select: fn(&mut Caches) -> &mut Slot<T>,
        work: impl FnOnce() -> Option<T> + Send + 'static,
    ) {
        {
            let mut c = self.lock();
            let slot = select(&mut c);
            if slot.fetching.as_deref() == Some(key.as_str()) {
                return;
            }
            slot.fetching = Some(key.clone());
        }
        let on_ready = Arc::clone(&self.on_ready);
        let key_for_cleanup = key.clone();
        let spawned = std::thread::Builder::new().name(thread_name.into()).spawn(move || {
            let result = work();
            let notify = {
                let mut c = self.lock();
                let slot = select(&mut c);
                let still_current = slot.key == key;
                if slot.fetching.as_deref() == Some(key.as_str()) {
                    slot.fetching = None;
                }
                match result {
                    Some(v) if still_current => {
                        slot.value = Some((key.clone(), v));
                        true
                    }
                    None if still_current => {
                        slot.key.clear();
                        false
                    }
                    _ => false,
                }
            };
            if notify {
                on_ready();
            }
        });
        if let Err(e) = spawned {
            eprintln!("[resolver] {thread_name} 线程启动失败: {e}");
            let mut c = self.lock();
            let slot = select(&mut c);
            if slot.fetching.as_deref() == Some(key_for_cleanup.as_str()) {
                slot.fetching = None;
            }
        }
    }

    fn spawn_meta(&'static self, song_id: String) {
        let id = song_id.clone();
        self.fetch_once("music-meta", song_id, |c| &mut c.meta, move || api::fetch_163_detail(&id));
    }

    fn spawn_lyrics(&'static self, key: String, title: String, artist: String, song_id: Option<String>) {
        self.fetch_once("music-lyrics", key, |c| &mut c.lyrics, move || match &song_id {
            Some(sid) => api::fetch_163_by_id(sid).or_else(|| api::fetch_lyrics(&title, &artist)),
            None => api::fetch_lyrics(&title, &artist),
        });
    }

    fn spawn_artwork(&'static self, key: String, provider: &'static dyn MusicProvider) {
        let key_for_check = key.clone();
        let this: &'static Resolver = self;
        self.fetch_once("music-artwork", key, |c| &mut c.artwork, move || {
            // 切歌后播放器填充封面有延迟
            std::thread::sleep(Duration::from_millis(800));
            for attempt in 0..6 {
                if this.lock().artwork.key != key_for_check {
                    return None;
                }
                if let Some(bytes) = provider.artwork_bytes() {
                    return Some(Artwork {
                        hash: content_hash(&bytes),
                        data_url: format!("data:{};base64,{}", sniff_mime(&bytes), b64::encode(&bytes)),
                    });
                }
                std::thread::sleep(Duration::from_millis(if attempt < 3 { 500 } else { 1000 }));
            }
            None
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_mime_detects_common_image_formats() {
        assert_eq!(sniff_mime(&[0x89, 0x50, 0x4E, 0x47]), "image/png");
        assert_eq!(sniff_mime(&[0xFF, 0xD8, 0xFF]), "image/jpeg");
        assert_eq!(sniff_mime(&[0x47, 0x49, 0x46]), "image/gif");
        assert_eq!(sniff_mime(&[0x42, 0x4D, 0x00]), "image/bmp");
        assert_eq!(sniff_mime(&[0x00, 0x01]), "image/jpeg", "未知格式回退 jpeg");
    }

    #[test]
    fn slot_get_only_hits_matching_key() {
        let mut s: Slot<i32> = Slot::default();
        s.value = Some(("a".into(), 1));
        assert_eq!(s.get("a"), Some(&1));
        assert_eq!(s.get("b"), None, "键不匹配（已切歌）不得返回旧值");
    }
}
