#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Media {
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

impl Media {
    /// 认魔数不认扩展名
    pub fn image(bytes: Vec<u8>) -> Option<Self> {
        let mime = sniff_image(&bytes)?;
        Some(Self { mime, bytes })
    }

    /// 播放器给的封面格式不保证可识别
    pub fn image_or_jpeg(bytes: Vec<u8>) -> Self {
        let mime = sniff_image(&bytes).unwrap_or("image/jpeg");
        Self { mime, bytes }
    }
}

fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(b"BM") {
        return Some("image/bmp");
    }
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some("image/x-icon");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_image_recognizes_common_formats() {
        assert_eq!(sniff_image(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image(b"GIF89a...."), Some("image/gif"));
        assert_eq!(
            sniff_image(b"RIFF\x00\x00\x00\x00WEBPvp8"),
            Some("image/webp")
        );
        assert_eq!(sniff_image(b"BMxxxx"), Some("image/bmp"));
        assert_eq!(sniff_image(&[0x00, 0x00, 0x01, 0x00]), Some("image/x-icon"));
        assert_eq!(sniff_image(b"MZ\x90\x00"), None);
        assert_eq!(sniff_image(b""), None);
    }
}
