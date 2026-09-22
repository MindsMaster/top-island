pub fn content_hash(bytes: &[u8]) -> String {
    // FNV-1a 算法规定常量
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
        assert_eq!(h, content_hash(b"top island"));
        assert_eq!(h.len(), 32);
        assert!(h
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    }

    #[test]
    fn different_content_produces_different_hashes() {
        assert_ne!(content_hash(b"cover-a"), content_hash(b"cover-b"));
    }
}
