//! 封面内容 hash：FNV-1a 128 位，输出 32 位小写十六进制。
//! 只做同曲目去重（前端按 hash 拉取一次封面），无需密码学强度，不引外部 crate。

pub fn content_hash(bytes: &[u8]) -> String {
    // FNV-1a 128：offset basis 与 prime 为算法规定常量
    let mut h: u128 = 0x6c62272e07bb014262b821756295c58d;
    for &b in bytes {
        h ^= b as u128;
        h = h.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013B);
    }
    format!("{h:032x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic_and_32_lowercase_hex_chars() {
        let h = content_hash(b"top island");
        assert_eq!(h, content_hash(b"top island"), "同一输入必须得到同一 hash，否则前端会反复拉取封面");
        assert_eq!(h.len(), 32, "hash 应为 32 位十六进制（128bit）");
        assert!(h.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()), "hash 应为小写十六进制");
    }

    #[test]
    fn different_content_produces_different_hashes() {
        assert_ne!(
            content_hash(b"cover-a"),
            content_hash(b"cover-b"),
            "不同封面内容必须得到不同 hash，否则切歌后前端拿不到新封面"
        );
    }
}
