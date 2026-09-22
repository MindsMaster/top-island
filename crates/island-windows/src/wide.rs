pub fn to_wide_nul(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn from_wide_nul(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

#[cfg(test)]
mod tests {
    use super::{from_wide_nul, to_wide_nul};

    #[test]
    fn to_wide_nul_appends_exactly_one_nul_terminator() {
        let wide = to_wide_nul("ab");
        assert_eq!(wide, vec![b'a' as u16, b'b' as u16, 0]);
    }

    #[test]
    fn from_wide_nul_stops_at_the_first_nul_and_drops_trailing_garbage() {
        let wide = [b'h' as u16, b'i' as u16, 0, 0xFFFF, 0x1234];
        assert_eq!(from_wide_nul(&wide), "hi");
    }

    #[test]
    fn wide_round_trip_preserves_unicode_text() {
        let original = "闹钟 Alarm 01 — 剪切 ✅";
        assert_eq!(from_wide_nul(&to_wide_nul(original)), original);
    }
}
