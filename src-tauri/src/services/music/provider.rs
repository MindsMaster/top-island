use std::sync::Arc;

use island_core::LyricsData;

use super::catalog::SongRef;
use crate::error::AppResult;

#[derive(Debug, Clone, Copy)]
pub enum Control {
    Play,
    Pause,
    Next,
    Prev,
    Seek(i64),
}

/// position_ms 为锚点时刻位置
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderState {
    pub is_playing: bool,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    /// SMTC SourceAppUserModelId
    pub source_app_id: String,
    pub song_id: Option<SongRef>,
    pub position_ms: i64,
    pub anchor_epoch_ms: i64,
    /// 播放 1 暂停 0
    pub rate: f64,
    pub duration_ms: Option<i64>,
    pub seek_supported: bool,
    pub artwork_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub cover_url: String,
    pub duration_ms: i64,
}

/// 播放器已加载的歌词 键随内容变
#[derive(Debug, Clone)]
pub struct LocalLyrics {
    pub key: String,
    pub data: Arc<LyricsData>,
}

/// 播放器增强 叠加在同名 SMTC 会话上
pub trait MusicProvider: Send + Sync {
    fn name(&self) -> &'static str;

    /// SMTC 来源应用 ID 小写子串
    fn claims(&self) -> &'static str;

    /// 每帧调用 不得阻塞
    fn snapshot(&self, base: Option<&ProviderState>) -> Option<ProviderState>;

    /// 须属于 state 这首 否则按名搜
    fn lyrics(&self, _state: &ProviderState) -> Option<LocalLyrics> {
        None
    }

    /// false 则改走 SMTC
    fn control(&self, _action: Control) -> AppResult<bool> {
        Ok(false)
    }
}

pub fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
