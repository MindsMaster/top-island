//! 音乐源抽象。网易云 bridge 是权威源（真 songId、真进度、可 seek），SMTC 是通用兜底。
//! 源之间的差异只通过 `Capabilities` 表达；歌词、封面等资源由 resolver 统一解析，不在这一层。

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
    /// 上一首/下一首在播放列表层，bridge 不覆盖
    pub skip: bool,
    /// 封面以位图字节流提供（SMTC）；否则走直链或按 songId 解析
    pub artwork_bitmap: bool,
    /// 其它源控制未送达时回退到它
    pub fallback: bool,
}

/// 锚点式快照：`position_ms` 是 `anchor_epoch_ms` 时刻的位置
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderState {
    pub is_playing: bool,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    /// SMTC SourceAppUserModelId，如 "cloudmusic.exe"
    pub source_app_id: String,
    pub song_id: Option<String>,
    pub position_ms: i64,
    pub anchor_epoch_ms: i64,
    /// 播放 1.0，暂停 0.0
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

    /// 推送线程每帧调用，必须快速返回，不得阻塞
    fn snapshot(&self) -> Option<ProviderState>;

    fn capabilities(&self) -> Capabilities;

    /// 返回是否送达；不支持或失败返回 false，聚合器据此回退
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
