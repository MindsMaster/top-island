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
        assert!(lyrics_supported("cloudmusic.exe"));
        assert!(lyrics_supported("QQMusic"));
        assert!(lyrics_supported("Spotify.exe"));
    }

    #[test]
    fn unknown_sources_do_not_trigger_lyric_search() {
        assert!(!lyrics_supported("chrome.exe"));
        assert!(!lyrics_supported(""));
    }
}
