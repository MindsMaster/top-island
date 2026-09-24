fn decode_char(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// 容错解码 对齐 JS Buffer
pub fn decode(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut nbits = 0u32;
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        let Some(v) = decode_char(c) else { continue };
        acc = (acc << 6) | v as u32;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_matches_rfc4648_test_vectors() {
        assert_eq!(decode("TWFu"), b"Man");
        assert_eq!(decode("TWE="), b"Ma");
        assert_eq!(decode("TQ=="), b"M");
    }

    #[test]
    fn decode_tolerates_embedded_whitespace() {
        // QQ 歌词可能夹带换行
        assert_eq!(decode("TW\nFu"), b"Man");
    }

    #[test]
    fn roundtrip_preserves_arbitrary_bytes() {
        let data: Vec<u8> = (0..=255u8).collect();
        assert_eq!(decode(&encode(&data)), data);
    }
}
