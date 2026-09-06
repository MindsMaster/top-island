use serde::{Deserialize, Serialize};

/// 锚点式播放状态：`position_ms` 是 `anchor_epoch_ms` 时刻的位置，渲染层按 `rate` 本地外推。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicState {
    /// "ncm-bridge" | "smtc" | ""
    pub provider: String,
    pub is_playing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_app_id: Option<String>,
    /// 平台歌曲 ID，有它歌词按 ID 精确获取而非标题搜索
    #[serde(skip_serializing_if = "Option::is_none")]
    pub song_id: Option<String>,
    pub position_ms: i64,
    pub anchor_epoch_ms: i64,
    /// 播放 1.0，暂停或无时间轴 0.0
    pub rate: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    pub seek_supported: bool,
    /// 封面直链，可直接作 <img> src
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    /// 封面位图 hash，渲染层据此调 music_artwork 拉取一次
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork_hash: Option<String>,
    /// 歌词就绪标识，渲染层据此调 music_lyrics 拉取一次
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lyrics_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicArtwork {
    pub hash: String,
    /// data: URL，可直接作为 <img> src
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    pub time_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsData {
    pub lines: Vec<LyricLine>,
    /// 歌词源提供的歌曲时长（ms），SMTC 无时间轴时用于估算进度
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MusicAction {
    Play,
    Pause,
    Next,
    Prev,
    Volume,
}
