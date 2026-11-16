use std::sync::OnceLock;
use std::time::Duration;

use island_core::{LyricLine, LyricsData};

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

fn fetch_json(url: &str) -> Option<serde_json::Value> {
    let mut resp = match agent().get(url).header("User-Agent", UA).call() {
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

pub fn fetch_163_by_id(song_id: &str) -> Option<LyricsData> {
    let lyric_url = format!("https://music.163.com/api/song/lyric?id={song_id}&lv=-1&kv=-1&tv=-1");
    let detail_url =
        format!("https://music.163.com/api/song/detail?id={song_id}&ids=%5B{song_id}%5D");
    let detail_thread = std::thread::spawn(move || fetch_json(&detail_url));
    let lyric = fetch_json(&lyric_url);
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
    let detail = fetch_json(&format!(
        "https://music.163.com/api/song/detail?id={song_id}&ids=%5B{song_id}%5D"
    ))?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
