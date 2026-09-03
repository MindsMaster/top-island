use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicState {
    pub is_playing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_app_id: Option<String>,
    /// SMTC 时间轴，部分应用（旧版网易云等）不上报，此时两者为 0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    /// 会话是否支持外部改变播放位置（决定进度条能否拖动）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seek_supported: Option<bool>,
    /// 当前曲目封面的内容 hash；渲染层据此调 music_artwork 拉取一次
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork_hash: Option<String>,
    /// SMTC 无时间轴时，来自歌词源的估算时长（ms）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_duration_ms: Option<i64>,
    /// 当前曲目歌词已就绪的标识；渲染层据此调 music_lyrics 拉取一次
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
