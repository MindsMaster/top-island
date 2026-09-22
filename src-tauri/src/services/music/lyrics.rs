use std::sync::OnceLock;
use std::time::Duration;

use island_core::{LyricLine, LyricsData};

use super::b64;
use super::provider::TrackMeta;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36";
const TIMEOUT: Duration = Duration::from_secs(8);

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .build()
            .into()
    })
}

fn fetch_json(url: &str, referer: Option<&str>) -> Option<serde_json::Value> {
    let mut req = agent().get(url).header("User-Agent", UA);
    if let Some(r) = referer {
        req = req.header("Referer", r);
    }
    let mut resp = match req.call() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[music:lyrics] 请求失败 ({url}): {e}");
            return None;
        }
    };
    match resp.body_mut().read_json::<serde_json::Value>() {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("[music:lyrics] 响应不是 JSON ({url}): {e}");
            None
        }
    }
}

/// 等价 encodeURIComponent
fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn norm_eq(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

pub fn query_matches_song(query: &str, song_name: &str) -> bool {
    fn norm(s: &str) -> String {
        s.to_lowercase()
            .chars()
            .filter(|c| {
                !c.is_whitespace()
                    && !matches!(
                        c,
                        '-' | '_'
                            | '('
                            | ')'
                            | '['
                            | ']'
                            | '（'
                            | '）'
                            | '【'
                            | '】'
                            | '\''
                            | '"'
                            | ','
                            | '.'
                            | '，'
                            | '。'
                            | '!'
                            | '！'
                            | '?'
                            | '？'
                    )
            })
            .collect()
    }
    let q = norm(query);
    let n = norm(song_name);
    if q.is_empty() || n.is_empty() {
        return false;
    }
    q.contains(&n) || n.contains(&q)
}

/// 小数位数定单位
fn parse_time_tag(s: &str) -> Option<(i64, usize)> {
    debug_assert!(s.starts_with('['));
    let end = s.as_bytes().iter().position(|&b| b == b']')?;
    let inner = &s[1..end];
    let (mm, rest) = inner.split_once(':')?;
    if mm.is_empty() || mm.len() > 3 || !mm.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (ss, frac_raw) = match rest.split_once(['.', ':']) {
        Some((ss, f)) => {
            // 对齐 JS 正则
            if f.is_empty() {
                return None;
            }
            (ss, f)
        }
        None => (rest, ""),
    };
    if ss.is_empty() || ss.len() > 2 || !ss.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if frac_raw.len() > 3 || !frac_raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let min: i64 = mm.parse().ok()?;
    let sec: i64 = ss.parse().ok()?;
    let frac: i64 = match frac_raw.len() {
        3 => frac_raw.parse().ok()?,
        2 => frac_raw.parse::<i64>().ok()? * 10,
        1 => frac_raw.parse::<i64>().ok()? * 100,
        _ => 0,
    };
    Some(((min * 60 + sec) * 1000 + frac, end + 1))
}

pub fn parse_lrc(lrc: &str) -> Vec<LyricLine> {
    let mut out = Vec::new();
    for raw in lrc.lines() {
        let mut times = Vec::new();
        let mut text = String::new();
        let mut rest = raw;
        while !rest.is_empty() {
            if rest.starts_with('[') {
                if let Some((t, len)) = parse_time_tag(rest) {
                    times.push(t);
                    rest = &rest[len..];
                    continue;
                }
            }
            let ch = rest.chars().next().expect("rest 非空");
            text.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        for &t in &times {
            out.push(LyricLine {
                time_ms: t,
                text: text.to_string(),
            });
        }
    }
    out.sort_by_key(|l| l.time_ms);
    out
}

fn search_163(title: &str, artist: &str) -> Option<(i64, i64)> {
    let query = if artist.is_empty() {
        title.to_string()
    } else {
        format!("{title} {artist}")
    };
    let url = format!(
        "https://music.163.com/api/search/get/web?s={}&type=1&offset=0&total=true&limit=10",
        url_encode(&query)
    );
    let json = fetch_json(&url, None)?;
    let songs = json.get("result")?.get("songs")?.as_array()?;
    if songs.is_empty() {
        return None;
    }

    let id_duration = |s: &serde_json::Value| -> Option<(i64, i64)> {
        Some((
            s.get("id")?.as_i64()?,
            s.get("duration").and_then(|d| d.as_i64()).unwrap_or(0),
        ))
    };

    if !artist.is_empty() {
        for s in songs {
            let artist_hit = s
                .get("artists")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                        .any(|n| norm_eq(n, artist))
                })
                .unwrap_or(false);
            if artist_hit {
                return id_duration(s);
            }
        }
    }
    let first = &songs[0];
    if let Some(name) = first.get("name").and_then(|n| n.as_str()) {
        if !query_matches_song(&query, name) {
            return None;
        }
    }
    id_duration(first)
}

fn fetch_163_inner(title: &str, artist: &str) -> Option<LyricsData> {
    let (id, duration_ms) = search_163(title, artist)?;
    let json = fetch_json(
        &format!("https://music.163.com/api/song/lyric?id={id}&lv=-1&kv=-1&tv=-1"),
        None,
    )?;
    let lrc = json
        .get("lrc")
        .and_then(|l| l.get("lyric"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Some(LyricsData {
        lines: parse_lrc(lrc),
        duration_ms,
    })
}

/// 搜索会错配 live
pub fn fetch_163_by_id(song_id: &str) -> Option<LyricsData> {
    let lyric_url = format!("https://music.163.com/api/song/lyric?id={song_id}&lv=-1&kv=-1&tv=-1");
    let detail_url =
        format!("https://music.163.com/api/song/detail?id={song_id}&ids=%5B{song_id}%5D");
    let detail_thread = std::thread::spawn(move || fetch_json(&detail_url, None));
    let lyric = fetch_json(&lyric_url, None);
    let detail = detail_thread.join().ok().flatten();
    let lrc = lyric
        .as_ref()
        .and_then(|j| j.get("lrc"))
        .and_then(|l| l.get("lyric"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let duration_ms = detail
        .as_ref()
        .and_then(|j| j.get("songs"))
        .and_then(|s| s.as_array())
        .and_then(|a| a.first())
        .and_then(|s| s.get("duration"))
        .and_then(|d| d.as_i64())
        .unwrap_or(0);
    if lrc.is_empty() && duration_ms == 0 {
        return None;
    }
    Some(LyricsData {
        lines: parse_lrc(lrc),
        duration_ms,
    })
}

pub fn fetch_163_detail(song_id: &str) -> Option<TrackMeta> {
    let detail = fetch_json(
        &format!("https://music.163.com/api/song/detail?id={song_id}&ids=%5B{song_id}%5D"),
        None,
    )?;
    let song = detail.get("songs")?.as_array()?.first()?;
    let title = song
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let artist = song
        .get("artists")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                .collect::<Vec<_>>()
                .join(" / ")
        })
        .unwrap_or_default();
    let album = song
        .get("album")
        .and_then(|al| al.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let pic = song
        .get("album")
        .and_then(|al| al.get("picUrl"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let cover_url = if pic.is_empty() {
        String::new()
    } else {
        format!("{pic}?param=512y512")
    };
    let duration_ms = song.get("duration").and_then(|d| d.as_i64()).unwrap_or(0);
    if title.is_empty() && cover_url.is_empty() && duration_ms == 0 {
        return None;
    }
    Some(TrackMeta {
        title,
        artist,
        album,
        cover_url,
        duration_ms,
    })
}

fn provider_163_fetch(title: &str, artist: &str) -> Option<LyricsData> {
    let r = fetch_163_inner(title, artist);
    if r.as_ref()
        .is_some_and(|r| !r.lines.is_empty() || r.duration_ms > 0)
    {
        return r;
    }
    // 退化只搜曲名
    if artist.is_empty() {
        None
    } else {
        fetch_163_inner(title, "")
    }
}

const QQ_REFERER: &str = "https://y.qq.com/";

fn search_qq(title: &str, artist: &str) -> Option<(String, i64)> {
    let query = if artist.is_empty() {
        title.to_string()
    } else {
        format!("{title} {artist}")
    };
    let url = format!(
        "https://c.y.qq.com/soso/fcgi-bin/client_search_cp?w={}&format=json&n=10&p=1&cr=1&t=0",
        url_encode(&query)
    );
    let json = fetch_json(&url, Some(QQ_REFERER))?;
    let songs = json.get("data")?.get("song")?.get("list")?.as_array()?;
    if songs.is_empty() {
        return None;
    }

    let mid_duration = |s: &serde_json::Value| -> Option<(String, i64)> {
        Some((
            s.get("songmid")?.as_str()?.to_string(),
            s.get("interval").and_then(|i| i.as_i64()).unwrap_or(0) * 1000,
        ))
    };

    if !artist.is_empty() {
        for s in songs {
            let artist_hit = s
                .get("singer")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                        .any(|n| norm_eq(n, artist))
                })
                .unwrap_or(false);
            if artist_hit {
                return mid_duration(s);
            }
        }
    }
    let first = &songs[0];
    if let Some(name) = first.get("songname").and_then(|n| n.as_str()) {
        if !query_matches_song(&query, name) {
            return None;
        }
    }
    mid_duration(first)
}

fn fetch_qq_inner(title: &str, artist: &str) -> Option<LyricsData> {
    let (songmid, duration_ms) = search_qq(title, artist)?;
    let json = fetch_json(
        &format!(
            "https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg?songmid={songmid}&format=json&nobase64=0&g_tk=5381"
        ),
        Some(QQ_REFERER),
    )?;
    let lrc_b64 = json.get("lyric").and_then(|v| v.as_str()).unwrap_or("");
    if lrc_b64.is_empty() {
        return Some(LyricsData {
            lines: Vec::new(),
            duration_ms,
        });
    }
    let lrc_bytes = b64::decode(lrc_b64);
    let lrc = String::from_utf8_lossy(&lrc_bytes);
    Some(LyricsData {
        lines: parse_lrc(&lrc),
        duration_ms,
    })
}

fn provider_qq_fetch(title: &str, artist: &str) -> Option<LyricsData> {
    let r = fetch_qq_inner(title, artist);
    if r.as_ref()
        .is_some_and(|r| !r.lines.is_empty() || r.duration_ms > 0)
    {
        return r;
    }
    if artist.is_empty() {
        None
    } else {
        fetch_qq_inner(title, "")
    }
}

/// 有行即优 时长兜底
pub fn fetch_lyrics(title: &str, artist: &str) -> Option<LyricsData> {
    if title.is_empty() {
        return None;
    }
    let mut best: Option<LyricsData> = None;
    for fetch in [provider_163_fetch, provider_qq_fetch] {
        let Some(r) = fetch(title, artist) else {
            continue;
        };
        if !r.lines.is_empty() {
            return Some(r);
        }
        if best.is_none() && r.duration_ms > 0 {
            best = Some(r);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encode_keeps_unreserved_and_percent_encodes_utf8() {
        assert_eq!(url_encode("a b"), "a%20b");
        assert_eq!(url_encode("歌"), "%E6%AD%8C");
        assert_eq!(url_encode("a-b_c.!~*'()"), "a-b_c.!~*'()");
    }

    #[test]
    fn parse_lrc_extracts_time_and_text() {
        let lines = parse_lrc("[00:12.34]你好世界");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].time_ms, 12340);
        assert_eq!(lines[0].text, "你好世界");
    }

    #[test]
    fn parse_lrc_handles_multiple_tags_on_one_line() {
        let lines = parse_lrc("[00:01.00][00:30.50]重复行");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].time_ms, 30500);
        assert!(lines.iter().all(|l| l.text == "重复行"));
    }

    #[test]
    fn parse_lrc_interprets_fraction_by_digit_count() {
        let lines = parse_lrc("[01:02:345]x");
        assert_eq!(lines[0].time_ms, 62345);
        let lines = parse_lrc("[00:00.5]x");
        assert_eq!(lines[0].time_ms, 500);
    }

    #[test]
    fn parse_lrc_skips_metadata_and_empty_text() {
        let lines = parse_lrc("[ti:歌名]\n[ar:歌手]\n[00:01.00]\n[00:02.00]有词");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "有词");
    }

    #[test]
    fn parse_lrc_sorts_lines_by_time() {
        let lines = parse_lrc("[00:30.00]后\n[00:01.00]先");
        assert_eq!(lines[0].text, "先");
    }

    #[test]
    fn query_matches_song_requires_lexical_overlap() {
        assert!(query_matches_song("晴天 周杰伦", "晴天"));
        assert!(query_matches_song("晴天(Live)", "晴天 (Live)"));
        assert!(!query_matches_song("晴天", "雨天"));
    }
}
