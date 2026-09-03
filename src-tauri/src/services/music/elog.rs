//! 网易云 elog 实时进度源。网易云桌面版把播放生命周期写入
//! %LOCALAPPDATA%/NetEase/CloudMusic/cloudmusic.elog，按字节异或表编码：
//! 低4位 = 高4位^0x3，高4位 = 高4位^0x8^低4位。
//! 事件状态机推算当前位置（计时基于行首毫秒级单调时钟，见 head_counter）：
//!   播放开始/恢复 -> 记录毫秒时钟基准；暂停/停止 -> 累计已播时长；
//!   setPlayingPosition -> 应用内拖动进度，重置基准。

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// 首次读取的尾部窗口；事件很密集，512KB 足够覆盖最近几首歌
const INITIAL_TAIL_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionInfo {
    pub position_ms: i64,
    /// 恒为 0：elog 中的时长字段不可靠（曾误抓无关 JSON 的 "time"），
    /// 由上层按 songId 从网易云 API 取权威时长
    pub duration_ms: i64,
    pub playing: bool,
    /// 平台侧歌曲 ID；有则可按 ID 精确取歌词/时长
    pub song_id: Option<String>,
}

#[derive(Debug, Clone)]
struct PlaybackState {
    song_id: String,
    playing: bool,
    /// 累计已播放（ms），不含当前进行中的区间
    base_ms: f64,
    /// playing 时的基准时刻（elog 毫秒时钟空间，非 epoch）
    since_counter: i64,
}

pub fn decode_byte(b: u8) -> u8 {
    let lo = b & 0xf;
    let hi = (b >> 4) & 0xf;
    (hi ^ 0x3) | ((hi ^ 0x8 ^ lo) << 4)
}

fn decode(buf: &[u8]) -> String {
    let decoded: Vec<u8> = buf.iter().map(|&b| decode_byte(b)).collect();
    String::from_utf8_lossy(&decoded).into_owned()
}

/// 行首形如 [pid:tid:MMDD/HHMMSS:98637531:INFO:...]，第 4 段是毫秒级单调时钟
fn head_counter(line: &str) -> Option<i64> {
    let rest = line.strip_prefix('[')?;
    let mut segs = rest.splitn(5, ':');
    let s1 = segs.next()?;
    let s2 = segs.next()?;
    let s3 = segs.next()?;
    let s4 = segs.next()?;
    // 时钟段后必须还有冒号分隔（对齐 JS 正则 `:(\d+):`）
    segs.next()?;
    if s1.is_empty() || !s1.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if s2.is_empty() || !s2.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (date, time) = s3.split_once('/')?;
    if date.len() != 4 || time.len() != 6 {
        return None;
    }
    if !date.bytes().chain(time.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    if s4.is_empty() || !s4.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s4.parse().ok()
}

fn take_digits(s: &str) -> (&str, &str) {
    let end = s
        .bytes()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(s.len());
    s.split_at(end)
}

/// 播放状态: "native播放state",<0|1>,"<songId>_XXX"
fn parse_native_state(line: &str) -> Option<(bool, String)> {
    const PAT: &str = "\"native播放state\",";
    let idx = line.find(PAT)?;
    let rest = &line[idx + PAT.len()..];
    let (digits, rest) = take_digits(rest);
    if digits.is_empty() {
        return None;
    }
    let rest = rest.strip_prefix(",\"")?;
    let (song_id, rest) = take_digits(rest);
    if song_id.is_empty() || !rest.starts_with('_') {
        return None;
    }
    Some((digits == "1", song_id.to_string()))
}

/// 开始播放命令: "nativePlay","playCommand",,,"songId"（含真实 songId，非"重置"）
fn parse_play_command(line: &str) -> Option<String> {
    const PAT: &str = "\"nativePlay\",\"playCommand\",";
    let idx = line.find(PAT)?;
    let rest = &line[idx + PAT.len()..];
    // 逗号分隔的空字段后第一个引号包裹的 4 位以上数字是 songId
    let quote = rest.find('"')?;
    let after = &rest[quote + 1..];
    let (song_id, tail) = take_digits(after);
    if song_id.len() >= 4 && tail.starts_with('"') {
        Some(song_id.to_string())
    } else {
        None
    }
}

/// 应用内拖动进度: "setPlayingPosition",<value>
fn parse_set_position(line: &str) -> Option<f64> {
    const PAT: &str = "\"setPlayingPosition\",";
    let idx = line.find(PAT)?;
    let rest = line[idx + PAT.len()..].trim_start();
    let end = rest
        .bytes()
        .position(|b| !(b.is_ascii_digit() || b == b'.'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn elog_path() -> PathBuf {
    PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default())
        .join("NetEase")
        .join("CloudMusic")
        .join("cloudmusic.elog")
}

#[derive(Debug, Default)]
pub struct NeteaseElog {
    offset: i64,
    carry: String,
    state: Option<PlaybackState>,
    available: Option<bool>,
    /**
     * epoch - counter 的估计值，取最新一行的 (读取时刻 - 行首 counter)。
     * counter 是实时推进的毫秒时钟，用任一行建立映射后，now - delta 就是当前
     * counter，staleness 不产生误差；counter 时代切换（网易云重启）时新一轮
     * 日志行会重建映射，无需显式探测。
     */
    counter_delta: Option<i64>,
}

impl NeteaseElog {
    pub const fn new() -> Self {
        Self { offset: -1, carry: String::new(), state: None, available: None, counter_delta: None }
    }

    /// 是否负责该 SMTC 会话（按 SourceAppUserModelId 判断）；命中则读取 elog
    pub fn poll_if_matches(&mut self, source_app_id: &str) -> Option<PositionInfo> {
        if !source_app_id.to_lowercase().contains("cloudmusic") {
            return None;
        }
        self.poll()
    }

    pub fn poll(&mut self) -> Option<PositionInfo> {
        match self.poll_inner() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[music:elog] 读取失败: {e}");
                None
            }
        }
    }

    fn poll_inner(&mut self) -> std::io::Result<Option<PositionInfo>> {
        let path = elog_path();
        let available = match self.available {
            Some(a) => a,
            None => {
                let a = path.exists();
                self.available = Some(a);
                a
            }
        };
        if !available {
            return Ok(None);
        }

        let size = std::fs::metadata(&path)?.len();
        if self.offset < 0 || size < self.offset as u64 {
            // 首次读取或日志被轮转：从尾部窗口重建状态
            self.offset = size.saturating_sub(INITIAL_TAIL_BYTES) as i64;
            self.carry.clear();
            self.state = None;
        }
        if size > self.offset as u64 {
            let mut file = std::fs::File::open(&path)?;
            file.seek(SeekFrom::Start(self.offset as u64))?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            self.offset = size as i64;
            let now = epoch_ms();
            let mut text = std::mem::take(&mut self.carry);
            text.push_str(&decode(&buf));
            let mut lines: Vec<&str> = text.split('\n').collect();
            self.carry = lines.pop().unwrap_or("").to_string();
            for line in lines {
                self.consume(line, now);
            }
        }
        Ok(self.snapshot(epoch_ms()))
    }

    fn ensure_state(&mut self, song_id: &str, cnt: i64) {
        let rebuild = match &self.state {
            None => true,
            Some(s) => !song_id.is_empty() && s.song_id != song_id,
        };
        if rebuild {
            self.state = Some(PlaybackState {
                song_id: song_id.to_string(),
                playing: false,
                base_ms: 0.0,
                since_counter: cnt,
            });
        }
    }

    fn consume(&mut self, line: &str, read_epoch_ms: i64) {
        // 每行都提取（顺带持续校准 counter_delta）
        let cnt = head_counter(line).unwrap_or(0);
        if cnt > 0 {
            self.counter_delta = Some(read_epoch_ms - cnt);
        }
        if cnt == 0 || !line.contains("playing") {
            return;
        }

        if let Some((playing, song_id)) = parse_native_state(line) {
            self.ensure_state(&song_id, cnt);
            if let Some(s) = self.state.as_mut() {
                if playing {
                    mark_playing(s, cnt);
                } else {
                    mark_stopped(s, cnt);
                }
            }
            return;
        }
        if let Some(song_id) = parse_play_command(line) {
            self.ensure_state(&song_id, cnt);
            if let Some(s) = self.state.as_mut() {
                // playCommand = 从头播放该曲目。songId 相同（单曲循环重播）时
                // ensure_state 不会重建状态，须显式归零，否则进度卡在上一遍末尾
                s.base_ms = 0.0;
                s.playing = false;
                mark_playing(s, cnt);
            }
            return;
        }
        if let Some(v) = parse_set_position(line) {
            if let Some(s) = self.state.as_mut() {
                // 单位兼容：大于 10000 视为 ms，否则视为秒
                s.base_ms = if v > 10000.0 { v } else { v * 1000.0 };
                s.since_counter = cnt;
            }
            return;
        }
        // 停止/播完
        if line.contains("\"stop playId=\"") || line.contains("\"onPlayEnd handle reason\"") {
            if let Some(s) = self.state.as_mut() {
                mark_stopped(s, cnt);
            }
        }
    }

    fn snapshot(&self, now_epoch_ms: i64) -> Option<PositionInfo> {
        let s = self.state.as_ref()?;
        // 当前时刻换算到 counter 空间再外推；delta 必然已由日志行校准过
        let now_counter = self
            .counter_delta
            .map(|d| now_epoch_ms - d)
            .unwrap_or(s.since_counter);
        let position = if s.playing {
            s.base_ms + (now_counter - s.since_counter).max(0) as f64
        } else {
            s.base_ms
        };
        Some(PositionInfo {
            position_ms: position.max(0.0).round() as i64,
            duration_ms: 0,
            playing: s.playing,
            song_id: if s.song_id.is_empty() { None } else { Some(s.song_id.clone()) },
        })
    }
}

fn mark_playing(s: &mut PlaybackState, cnt: i64) {
    if !s.playing {
        s.playing = true;
        s.since_counter = cnt;
    }
}

fn mark_stopped(s: &mut PlaybackState, cnt: i64) {
    if s.playing {
        s.base_ms += (cnt - s.since_counter).max(0) as f64;
        s.playing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(counter: i64, body: &str) -> String {
        format!("[123:456:0701/120000:{counter}:INFO:x] 【playing】{body}")
    }

    #[test]
    fn decode_byte_matches_the_documented_xor_table() {
        // 低4位 = 高4位^0x3 = 0^3 = 3；高4位 = 高4位^0x8^低4位 = 0^8^0 = 8
        assert_eq!(decode_byte(0x00), 0x83, "0x00 经异或表解码应为 0x83，错了说明解码表移植有误");
        // hi=0xf, lo=0x5：低 = f^3=c；高 = f^8^5=2
        assert_eq!(decode_byte(0xF5), 0x2C, "0xF5 经异或表解码应为 0x2C，错了说明高低位异或顺序有误");
    }

    #[test]
    fn head_counter_extracts_the_millisecond_clock_from_line_prefix() {
        assert_eq!(
            head_counter("[1:2:0701/120000:98637531:INFO:x] rest"),
            Some(98637531),
            "行首第 4 段是毫秒时钟，提取失败说明前缀格式解析有误"
        );
        assert_eq!(head_counter("no bracket"), None, "没有行首括号的行应视为无时钟");
        assert_eq!(head_counter("[1:2:070/120000:9:x]"), None, "日期段不足 4 位应拒绝");
    }

    #[test]
    fn native_state_line_parses_playing_flag_and_song_id() {
        let l = line(100, "\"native播放state\",1,\"1234567890_abc\"");
        assert_eq!(
            parse_native_state(&l),
            Some((true, "1234567890".to_string())),
            "播放状态行应解析出播放位与 songId"
        );
        let l = line(100, "\"native播放state\",0,\"9876543210_x\"");
        assert_eq!(parse_native_state(&l), Some((false, "9876543210".to_string())), "0 应为暂停");
    }

    #[test]
    fn play_command_line_parses_real_song_id() {
        let l = line(100, "\"nativePlay\",\"playCommand\",,,\"1234567890\"");
        assert_eq!(
            parse_play_command(&l),
            Some("1234567890".to_string()),
            "playCommand 行应取空字段后的数字 songId"
        );
        let l = line(100, "\"nativePlay\",\"playCommand\",,,\"重置\"");
        assert_eq!(parse_play_command(&l), None, "非数字的伪 songId（如「重置」）应拒绝");
    }

    #[test]
    fn set_position_line_parses_numeric_value() {
        let l = line(100, "\"setPlayingPosition\", 65.5");
        assert_eq!(parse_set_position(&l), Some(65.5), "拖动进度行应解析出浮点位置");
    }

    #[test]
    fn playing_extrapolates_position_with_the_millisecond_clock() {
        let mut elog = NeteaseElog::new();
        // playCommand 于 counter=1000 开始播放；读取时刻 epoch=101000
        elog.consume(&line(1000, "\"nativePlay\",\"playCommand\",,,\"1234567890\""), 101_000);
        let info = elog.snapshot(103_000).expect("应有播放状态");
        assert!(info.playing, "playCommand 后应处于播放态");
        assert_eq!(info.position_ms, 2000, "播放 2 秒后进度应为 2000ms，错了说明 counter→epoch 映射有误");
        assert_eq!(info.song_id.as_deref(), Some("1234567890"), "应带出 songId 供上层按 ID 取歌词");
    }

    #[test]
    fn pause_accumulates_elapsed_and_freezes_position() {
        let mut elog = NeteaseElog::new();
        elog.consume(&line(1000, "\"nativePlay\",\"playCommand\",,,\"1234567890\""), 101_000);
        elog.consume(&line(3000, "\"native播放state\",0,\"1234567890_x\""), 103_000);
        let info = elog.snapshot(110_000).expect("应有播放状态");
        assert!(!info.playing, "播放态 0 后应处于暂停态");
        assert_eq!(info.position_ms, 2000, "暂停后进度应冻结在累计值 2000ms，继续增长说明暂停累计有误");
    }

    #[test]
    fn replaying_the_same_song_resets_progress_to_zero() {
        let mut elog = NeteaseElog::new();
        elog.consume(&line(1000, "\"nativePlay\",\"playCommand\",,,\"1234567890\""), 101_000);
        elog.consume(&line(5000, "\"native播放state\",0,\"1234567890_x\""), 105_000);
        elog.consume(&line(6000, "\"nativePlay\",\"playCommand\",,,\"1234567890\""), 106_000);
        let info = elog.snapshot(107_000).expect("应有播放状态");
        assert_eq!(info.position_ms, 1000, "单曲循环重播同一首歌必须归零，错了进度会卡在上一遍末尾");
    }

    #[test]
    fn set_playing_position_rebases_in_seconds_or_ms() {
        let mut elog = NeteaseElog::new();
        elog.consume(&line(1000, "\"nativePlay\",\"playCommand\",,,\"1234567890\""), 101_000);
        elog.consume(&line(2000, "\"setPlayingPosition\", 65.5"), 102_000);
        let info = elog.snapshot(103_000).expect("应有播放状态");
        assert_eq!(info.position_ms, 66500, "小于 10000 的值按秒解释：65.5s 拖动后再播 1s 应为 66500ms");
        elog.consume(&line(4000, "\"setPlayingPosition\", 20000"), 104_000);
        let info = elog.snapshot(105_000).expect("应有播放状态");
        assert_eq!(info.position_ms, 21000, "大于 10000 的值按毫秒解释：20000ms 拖动后再播 1s 应为 21000ms");
    }

    #[test]
    fn lines_without_playing_marker_are_ignored_but_still_calibrate_delta() {
        let mut elog = NeteaseElog::new();
        // 无 "playing" 关键字的行不参与状态机，但仍校准时钟映射
        elog.consume("[1:2:0701/120000:5000:INFO:x] noise", 105_000);
        assert_eq!(elog.counter_delta, Some(100_000), "普通日志行也应校准 counter_delta，否则外推基准缺失");
        assert!(elog.snapshot(106_000).is_none(), "没有播放事件时不应有状态");
    }
}
