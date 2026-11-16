use super::lyrics::LyricSource;

const DEFAULT_SOURCES: &[LyricSource] =
    &[LyricSource::Netease, LyricSource::Qq, LyricSource::Kugou];

#[derive(Debug)]
struct AppProfile {
    /// SMTC SourceAppUserModelId 小写子串
    needle: &'static str,
    /// SMTC 时间轴滞后补偿
    lyric_offset_ms: i64,
    /// 按名搜歌词的先后
    lyric_sources: &'static [LyricSource],
}

const fn app(needle: &'static str) -> AppProfile {
    AppProfile {
        needle,
        lyric_offset_ms: 0,
        lyric_sources: DEFAULT_SOURCES,
    }
}

/// 在表内才按标题搜歌词
const KNOWN_APPS: &[AppProfile] = &[
    app("cloudmusic"),
    AppProfile {
        needle: "qqmusic",
        lyric_offset_ms: 400,
        lyric_sources: DEFAULT_SOURCES,
    },
    app("spotify"),
    AppProfile {
        needle: "kugou",
        lyric_offset_ms: 0,
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

pub fn lyric_offset_ms(source_app_id: &str) -> i64 {
    profile(source_app_id).map_or(0, |p| p.lyric_offset_ms)
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

    #[test]
    fn lyric_offset_only_for_lagging_players() {
        assert_eq!(lyric_offset_ms("QQMusic.exe"), 400);
        assert_eq!(lyric_offset_ms("cloudmusic.exe"), 0);
        assert_eq!(lyric_offset_ms(""), 0);
    }
}
