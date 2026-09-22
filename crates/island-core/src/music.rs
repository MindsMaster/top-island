use serde::{Deserialize, Serialize};

// 与 src/platform/types.ts 一一对应

/// position_ms 为 anchor_epoch_ms 时刻位置 按 rate 外推
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
    /// 有则歌词按 ID 精确获取
    #[serde(skip_serializing_if = "Option::is_none")]
    pub song_id: Option<String>,
    pub position_ms: i64,
    pub anchor_epoch_ms: i64,
    /// 播放 1 暂停或无时间轴 0
    pub rate: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    pub seek_supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    /// 据此调 music_artwork 拉取一次
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork_hash: Option<String>,
    /// 据此调 music_lyrics 拉取一次
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lyrics_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicArtwork {
    pub hash: String,
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
    /// SMTC 无时间轴时用于估算进度
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
