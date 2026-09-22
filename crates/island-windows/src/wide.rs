//! Win32 宽字符串互转。

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
        assert_eq!(wide, vec![b'a' as u16, b'b' as u16, 0], "宽串必须以且仅以 1 个 NUL 结尾，否则剪贴板读取会越界");
    }

    #[test]
    fn from_wide_nul_stops_at_the_first_nul_and_drops_trailing_garbage() {
        let wide = [b'h' as u16, b'i' as u16, 0, 0xFFFF, 0x1234];
        assert_eq!(from_wide_nul(&wide), "hi", "NUL 之后的残留内存不能混进读出的文本");
    }

    #[test]
    fn wide_round_trip_preserves_unicode_text() {
        let original = "闹钟 Alarm 01 — 剪切 ✅";
        assert_eq!(from_wide_nul(&to_wide_nul(original)), original, "UTF-16 往返不应丢任何 Unicode 字符");
    }
}
