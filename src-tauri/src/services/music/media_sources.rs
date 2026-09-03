//! 只对已知音乐播放器做标题搜索歌词（commit b8a71e8 语义）：
//! 浏览器/会议软件等也会挂 SMTC 会话，对它们搜歌词既无意义又浪费请求。

/// SMTC SourceAppUserModelId 子串白名单
const LYRIC_SUPPORTED_APPS: &[&str] = &[
    "cloudmusic",
    "qqmusic",
    "spotify",
    "kugou",
    "kuwo",
    "itunes",
    "applemusic",
    "foobar",
    "musicbee",
    "aimp",
    "lx-music",
    "listen1",
    "yesplaymusic",
    "winamp",
];

pub fn lyrics_supported(source_app_id: &str) -> bool {
    let id = source_app_id.to_lowercase();
    LYRIC_SUPPORTED_APPS.iter().any(|k| id.contains(k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_music_players_support_lyric_search() {
        assert!(lyrics_supported("cloudmusic.exe"), "网易云桌面版应支持歌词搜索");
        assert!(lyrics_supported("QQMusic"), "QQ 音乐应支持歌词搜索（大小写不敏感）");
        assert!(lyrics_supported("Spotify.exe"), "Spotify 应支持歌词搜索");
    }

    #[test]
    fn unknown_sources_do_not_trigger_lyric_search() {
        assert!(!lyrics_supported("chrome.exe"), "浏览器不应触发歌词搜索");
        assert!(!lyrics_supported(""), "空来源不应触发歌词搜索");
    }
}
