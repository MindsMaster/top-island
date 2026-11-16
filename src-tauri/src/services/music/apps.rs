use super::lyrics::LyricSource;

const DEFAULT_SOURCES: &[LyricSource] =
    &[LyricSource::Netease, LyricSource::Qq, LyricSource::Kugou];

#[derive(Debug)]
struct AppProfile {
    /// SMTC SourceAppUserModelId 小写子串
    needle: &'static str,
    /// 按名搜歌词的先后
    lyric_sources: &'static [LyricSource],
}

const fn app(needle: &'static str) -> AppProfile {
    AppProfile {
        needle,
        lyric_sources: DEFAULT_SOURCES,
    }
}

/// 在表内才按标题搜歌词
const KNOWN_APPS: &[AppProfile] = &[
    app("cloudmusic"),
    AppProfile {
        needle: "qqmusic",
        lyric_sources: &[LyricSource::Qq, LyricSource::Netease, LyricSource::Kugou],
    },
    app("spotify"),
    AppProfile {
        needle: "kugou",
        lyric_sources: &[LyricSource::Kugou, LyricSource::Netease, LyricSource::Qq],
    },
    app("kuwo"),
    app("itunes"),
    app("applemusic"),
    app("foobar"),
    app("musicbee"),
    app("aimp"),
    app("lx-music"),
    app("listen1"),
    app("yesplaymusic"),
    app("winamp"),
];

fn profile(source_app_id: &str) -> Option<&'static AppProfile> {
    let id = source_app_id.to_lowercase();
    KNOWN_APPS.iter().find(|p| id.contains(p.needle))
}

pub fn lyrics_supported(source_app_id: &str) -> bool {
    profile(source_app_id).is_some()
}

pub fn lyric_sources(source_app_id: &str) -> &'static [LyricSource] {
    profile(source_app_id).map_or(DEFAULT_SOURCES, |p| p.lyric_sources)
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
