use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use island_core::LyricsData;

use super::apps;
use super::catalog::{self, SongRef};
use super::hash::content_hash;
use super::lyrics::{self as api, LyricSource};
use super::provider::{LocalLyrics, ProviderState, TrackMeta};
use super::smtc::SMTC;
use crate::services::media::Media;

/// 来源间切换不丢
const SLOT_CAPACITY: usize = 4;

#[derive(Debug)]
struct Slot<T> {
    /// 兼作切歌水位
    key: String,
    values: VecDeque<(String, T)>,
    fetching: Option<String>,
}

/// 免 T: Default 约束
impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self {
            key: String::new(),
            values: VecDeque::new(),
            fetching: None,
        }
    }
}

impl<T> Slot<T> {
    fn get(&self, key: &str) -> Option<&T> {
        self.values.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn insert(&mut self, key: String, value: T) {
        self.values.retain(|(k, _)| *k != key);
        if self.values.len() >= SLOT_CAPACITY {
            self.values.pop_front();
        }
        self.values.push_back((key, value));
    }
}

#[derive(Debug)]
pub struct Artwork {
    pub hash: String,
    pub media: Media,
}

#[derive(Debug, Default)]
struct Caches {
    meta: Slot<TrackMeta>,
    lyrics: Slot<LyricsData>,
    artwork: Slot<Artwork>,
}

#[derive(Debug)]
struct LyricsQuery {
    key: String,
    title: String,
    artist: String,
    duration_ms: Option<i64>,
    song: Option<SongRef>,
    sources: &'static [LyricSource],
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
        Self {
            caches: Mutex::new(Caches::default()),
            on_ready: Arc::new(on_ready),
        }
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

    pub fn observe(&'static self, src: &ProviderState, local: Option<LocalLyrics>) -> Resolved {
        let mut out = Resolved {
            title: src.title.clone(),
            artist: src.artist.clone(),
            album: src.album.clone(),
            artwork_url: src.artwork_url.clone(),
            duration_ms: src.duration_ms,
            ..Default::default()
        };

        // spawn 内要拿锁 须出锁
        let mut fetch_meta: Option<SongRef> = None;
        let mut fetch_lyrics: Option<LyricsQuery> = None;
        let mut fetch_artwork: Option<(String, String)> = None;

        {
            let mut c = self.lock();

            match &src.song_id {
                Some(song) => {
                    let key = song.key();
                    if key != c.meta.key {
                        c.meta.key = key.clone();
                        if c.meta.get(&key).is_none() {
                            fetch_meta = Some(song.clone());
                        }
                    }
                    if let Some(m) = c.meta.get(&key) {
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

            // 键按补全后标题算
            let track_key = format!("{}|{}|{}", out.title, out.artist, src.source_app_id);
            // 按名搜须带时长 时长晚到则重搜
            let lyrics_key = match (&local, &src.song_id) {
                (Some(l), _) => l.key.clone(),
                (None, Some(song)) => song.key(),
                (None, None) => format!("{track_key}|{}", out.duration_ms.map_or(0, |d| d / 1000)),
            };

            if out.artwork_url.is_none() {
                if track_key != c.artwork.key {
                    c.artwork.key = track_key.clone();
                    if c.artwork.get(&track_key).is_none() {
                        fetch_artwork = Some((track_key.clone(), src.source_app_id.clone()));
                    }
                }
                if let Some(a) = c.artwork.get(&track_key) {
                    out.artwork_hash = Some(a.hash.clone());
                }
            } else {
                c.artwork.key = track_key.clone();
            }

            let can_fetch = src.song_id.is_some() || !out.title.is_empty();
            if let Some(l) = local {
                c.lyrics.key = lyrics_key.clone();
                if c.lyrics.get(&lyrics_key).is_none() {
                    c.lyrics.insert(l.key, (*l.data).clone());
                }
            } else if lyrics_key != c.lyrics.key && can_fetch {
                c.lyrics.key = lyrics_key.clone();
                let cached = c.lyrics.get(&lyrics_key).is_some();
                if !cached && (src.song_id.is_some() || apps::lyrics_supported(&src.source_app_id))
                {
                    fetch_lyrics = Some(LyricsQuery {
                        key: lyrics_key.clone(),
                        title: out.title.clone(),
                        artist: out.artist.clone(),
                        duration_ms: out.duration_ms,
                        song: src.song_id.clone(),
                        sources: apps::lyric_sources(&src.source_app_id),
                    });
                }
            }
            if let Some(l) = c.lyrics.get(&lyrics_key) {
                if !l.lines.is_empty() {
                    out.lyrics_id = Some(lyrics_key.clone());
                }
            }
        }

        if let Some(song) = fetch_meta {
            self.spawn_meta(song);
        }
        if let Some(q) = fetch_lyrics {
            self.spawn_lyrics(q);
        }
        if let Some((k, app)) = fetch_artwork {
            self.spawn_artwork(k, app);
        }
        out
    }

    pub fn artwork(&self, hash: &str) -> Option<Media> {
        let c = self.lock();
        c.artwork
            .values
            .iter()
            .find(|(_, a)| a.hash == hash)
            .map(|(_, a)| a.media.clone())
    }

    pub fn lyrics(&self, id: &str) -> Option<LyricsData> {
        self.lock().lyrics.get(id).cloned()
    }

    /// 键仍当前才通知
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
        let spawned = std::thread::Builder::new()
            .name(thread_name.into())
            .spawn(move || {
                let result = work();
                let notify = {
                    let mut c = self.lock();
                    let slot = select(&mut c);
                    let still_current = slot.key == key;
                    if slot.fetching.as_deref() == Some(key.as_str()) {
                        slot.fetching = None;
                    }
                    match result {
                        Some(v) => {
                            slot.insert(key.clone(), v);
                            still_current
                        }
                        None if still_current => {
                            slot.key.clear();
                            false
                        }
                        None => false,
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

    fn spawn_meta(&'static self, song: SongRef) {
        self.fetch_once(
            "music-meta",
            song.key(),
            |c| &mut c.meta,
            move || catalog::detail(&song),
        );
    }

    fn spawn_lyrics(&'static self, q: LyricsQuery) {
        self.fetch_once(
            "music-lyrics",
            q.key,
            |c| &mut c.lyrics,
            move || {
                q.song
                    .as_ref()
                    .and_then(catalog::lyrics)
                    .or_else(|| api::fetch_lyrics(&q.title, &q.artist, q.duration_ms, q.sources))
            },
        );
    }

    fn spawn_artwork(&'static self, key: String, app: String) {
        let key_for_check = key.clone();
        let this: &'static Resolver = self;
        self.fetch_once(
            "music-artwork",
            key,
            |c| &mut c.artwork,
            move || {
                // 封面填充有延迟
                std::thread::sleep(Duration::from_millis(800));
                for attempt in 0..6 {
                    if this.lock().artwork.key != key_for_check {
                        return None;
                    }
                    if let Some(media) = SMTC.artwork(&app).map(Media::image_or_jpeg) {
                        return Some(Artwork {
                            hash: content_hash(&media.bytes),
                            media,
                        });
                    }
                    std::thread::sleep(Duration::from_millis(if attempt < 3 { 500 } else { 1000 }));
                }
                None
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_get_only_hits_matching_key() {
        let mut s: Slot<i32> = Slot::default();
        s.insert("a".into(), 1);
        assert_eq!(s.get("a"), Some(&1));
        assert_eq!(s.get("b"), None);
    }

    #[test]
    fn slot_keeps_recent_entries_and_evicts_oldest() {
        let mut s: Slot<i32> = Slot::default();
        for (i, k) in ["a", "b", "c", "d", "e"].iter().enumerate() {
            s.insert((*k).into(), i as i32);
        }
        assert_eq!(s.get("a"), None);
        assert_eq!(s.get("b"), Some(&1));
        s.insert("b".into(), 9);
        assert_eq!(s.get("b"), Some(&9));
        assert_eq!(s.values.len(), SLOT_CAPACITY);
    }
}
