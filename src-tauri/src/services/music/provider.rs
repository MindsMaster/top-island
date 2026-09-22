use crate::error::AppResult;

#[derive(Debug, Clone, Copy)]
pub enum Control {
    Play,
    Pause,
    Next,
    Prev,
    Seek(i64),
}

impl Control {
    pub fn needs_skip(self) -> bool {
        matches!(self, Control::Next | Control::Prev)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Capabilities {
    /// bridge 不覆盖
    pub skip: bool,
    /// 位图字节流 否则直链
    pub artwork_bitmap: bool,
    pub fallback: bool,
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
    pub song_id: Option<String>,
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

pub trait MusicProvider: Send + Sync {
    fn name(&self) -> &'static str;

    /// 每帧调用 不得阻塞
    fn snapshot(&self) -> Option<ProviderState>;

    fn capabilities(&self) -> Capabilities;

    /// false 则聚合回退
    fn control(&self, action: Control) -> AppResult<bool>;

    fn artwork_bytes(&self) -> Option<Vec<u8>> {
        None
    }
}

pub fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
