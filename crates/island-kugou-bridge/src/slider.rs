//! 从界面所有 TrackBar 里认出播放进度条

/// 播放进度条刻度为 10ms
const TICK_MS: i64 = 10;
/// 刻度与进度回调的允许偏差
const MATCH_TOLERANCE_MS: i64 = 1500;
const CAPACITY: usize = 16;

#[derive(Debug, Clone, Copy, Default)]
struct Slider {
    this: usize,
    max: i32,
    pos: i32,
}

#[derive(Debug, Default)]
pub struct Sliders {
    items: Vec<Slider>,
}

impl Sliders {
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn entry(&mut self, this: usize) -> &mut Slider {
        let idx = match self.items.iter().position(|s| s.this == this) {
            Some(i) => i,
            None => {
                if self.items.len() >= CAPACITY {
                    self.items.remove(0);
                }
                self.items.push(Slider {
                    this,
                    ..Slider::default()
                });
                self.items.len() - 1
            }
        };
        &mut self.items[idx]
    }

    pub fn set_range(&mut self, this: usize, max: i32) {
        self.entry(this).max = max;
    }

    pub fn set_pos(&mut self, this: usize, pos: i32) {
        self.entry(this).pos = pos;
    }

    /// 音量条等也会凑巧对上 取量程最大者
    pub fn duration_ms(&self, position_ms: i64) -> Option<i64> {
        self.items
            .iter()
            .filter(|s| s.max > 0 && s.pos <= s.max)
            .filter(|s| (i64::from(s.pos) * TICK_MS - position_ms).abs() <= MATCH_TOLERANCE_MS)
            .map(|s| i64::from(s.max) * TICK_MS)
            .max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_slider_whose_pos_tracks_progress() {
        let mut s = Sliders::default();
        s.set_range(1, 25647);
        s.set_pos(1, 3361);
        s.set_range(2, 100);
        s.set_pos(2, 92);
        assert_eq!(s.duration_ms(33615), Some(256_470));
    }

    #[test]
    fn volume_slider_alone_near_start_loses_to_play_slider() {
        let mut s = Sliders::default();
        s.set_range(2, 100);
        s.set_pos(2, 92);
        s.set_range(1, 19375);
        s.set_pos(1, 90);
        assert_eq!(s.duration_ms(900), Some(193_750));
    }

    #[test]
    fn placeholder_range_below_pos_is_ignored() {
        let mut s = Sliders::default();
        s.set_range(1, 1000);
        s.set_pos(1, 3361);
        assert_eq!(s.duration_ms(33615), None);
    }

    #[test]
    fn stale_pos_does_not_match() {
        let mut s = Sliders::default();
        s.set_range(1, 27937);
        s.set_pos(1, 4750);
        assert_eq!(s.duration_ms(100), None);
    }
}
