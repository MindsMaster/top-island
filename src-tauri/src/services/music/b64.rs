//! base64 编解码（artwork data URL 编码、QQ 歌词 base64 解码用），
//! 不引外部 crate：编码表固定，解码容忍空白与尾部填充（对齐 JS Buffer 的宽容行为）。

const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

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

/// 非法字符（含空白）跳过，'=' 结束；残缺尾包丢弃
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
    fn encode_matches_rfc4648_test_vectors() {
        assert_eq!(encode(b""), "", "空输入应得空串");
        assert_eq!(encode(b"M"), "TQ==", "单字节应补两个 '='，错了说明填充逻辑有误");
        assert_eq!(encode(b"Ma"), "TWE=", "双字节应补一个 '='");
        assert_eq!(encode(b"Man"), "TWFu", "三字节整包不应有填充");
    }

    #[test]
    fn decode_matches_rfc4648_test_vectors() {
        assert_eq!(decode("TWFu"), b"Man", "标准向量解码错误");
        assert_eq!(decode("TWE="), b"Ma", "带一个填充的解码错误");
        assert_eq!(decode("TQ=="), b"M", "带两个填充的解码错误");
    }

    #[test]
    fn decode_tolerates_embedded_whitespace() {
        // QQ 歌词接口返回的 base64 可能夹带换行
        assert_eq!(decode("TW\nFu"), b"Man", "base64 中的空白应被跳过而非报错");
    }

    #[test]
    fn roundtrip_preserves_arbitrary_bytes() {
        let data: Vec<u8> = (0..=255u8).collect();
        assert_eq!(decode(&encode(&data)), data, "全字节值往返应无损");
    }
}
