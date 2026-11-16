//! 多来源时选哪个上岛 在播优先 同在播取最近开播

use std::collections::HashMap;

/// 停得更短视为切歌抖动 不算新开播
const RESUME_GRACE_MS: i64 = 3_000;

#[derive(Debug, Clone, Copy)]
struct Seen {
    playing: bool,
    /// 当前播放态起始时刻
    since_ms: i64,
    started_ms: i64,
    last_playing_ms: i64,
}

#[derive(Debug, Default)]
pub struct Arbiter {
    shown: Option<String>,
    seen: HashMap<String, Seen>,
}

impl Arbiter {
    /// 参数为各会话来源与是否在播 返回选中下标
    pub fn pick(&mut self, sessions: &[(&str, bool)], now_ms: i64) -> Option<usize> {
        self.seen
            .retain(|app, _| sessions.iter().any(|(a, _)| a == app));
        for &(app, playing) in sessions {
            let seen = self.seen.entry(app.to_string()).or_insert(Seen {
                playing,
                since_ms: now_ms,
                started_ms: if playing { now_ms } else { 0 },
                last_playing_ms: if playing { now_ms } else { 0 },
            });
            if seen.playing != playing {
                if playing && now_ms - seen.since_ms >= RESUME_GRACE_MS {
                    seen.started_ms = now_ms;
                }
                seen.playing = playing;
                seen.since_ms = now_ms;
            }
            if playing {
                seen.last_playing_ms = now_ms;
            }
        }

        let shown = self.shown.as_deref();
        let rank = |i: usize, key: fn(&Seen) -> i64| {
            let (app, _) = sessions[i];
            (
                key(&self.seen[app]),
                shown == Some(app),
                std::cmp::Reverse(i),
            )
        };
        let playing: Vec<usize> = (0..sessions.len()).filter(|&i| sessions[i].1).collect();
        let picked = if playing.is_empty() {
            (0..sessions.len()).max_by_key(|&i| rank(i, |s| s.last_playing_ms))
        } else {
            playing
                .into_iter()
                .max_by_key(|&i| rank(i, |s| s.started_ms))
        };
        self.shown = picked.map(|i| sessions[i].0.to_string());
        picked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playing_beats_paused() {
        let mut a = Arbiter::default();
        assert_eq!(a.pick(&[("ncm", false), ("kugou", true)], 0), Some(1));
    }

    #[test]
    fn latest_start_wins_and_stays() {
        let mut a = Arbiter::default();
        assert_eq!(a.pick(&[("kugou", true), ("edge", false)], 0), Some(0));
        assert_eq!(a.pick(&[("kugou", true), ("edge", true)], 10_000), Some(1));
        assert_eq!(a.pick(&[("kugou", true), ("edge", true)], 20_000), Some(1));
    }

    #[test]
    fn track_change_blip_does_not_steal() {
        let mut a = Arbiter::default();
        a.pick(&[("kugou", true), ("edge", false)], 0);
        a.pick(&[("kugou", true), ("edge", true)], 10_000);
        a.pick(&[("kugou", false), ("edge", true)], 20_000);
        assert_eq!(a.pick(&[("kugou", true), ("edge", true)], 21_000), Some(1));
    }

    #[test]
    fn resume_after_real_pause_takes_over() {
        let mut a = Arbiter::default();
        a.pick(&[("kugou", true), ("edge", false)], 0);
        a.pick(&[("kugou", true), ("edge", true)], 10_000);
        a.pick(&[("kugou", false), ("edge", true)], 20_000);
        assert_eq!(a.pick(&[("kugou", true), ("edge", true)], 30_000), Some(0));
    }

    #[test]
    fn all_paused_keeps_last_played() {
        let mut a = Arbiter::default();
        a.pick(&[("ncm", true), ("kugou", false)], 0);
        assert_eq!(a.pick(&[("ncm", false), ("kugou", false)], 5_000), Some(0));
        a.pick(&[("ncm", false), ("kugou", true)], 10_000);
        assert_eq!(a.pick(&[("ncm", false), ("kugou", false)], 15_000), Some(1));
    }

    #[test]
    fn vanished_session_falls_back() {
        let mut a = Arbiter::default();
        a.pick(&[("ncm", false), ("kugou", true)], 0);
        assert_eq!(a.pick(&[("ncm", false)], 1_000), Some(0));
        assert_eq!(a.pick(&[], 2_000), None);
    }

    #[test]
    fn simultaneous_first_sight_keeps_order() {
        let mut a = Arbiter::default();
        assert_eq!(a.pick(&[("ncm", true), ("kugou", true)], 0), Some(0));
        assert_eq!(a.pick(&[("ncm", true), ("kugou", true)], 1_000), Some(0));
    }
}
