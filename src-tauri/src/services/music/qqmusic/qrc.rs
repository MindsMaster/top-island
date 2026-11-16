//! QQ 音乐本地歌词缓存 `*_qm.qrc`
//! 整文件异或 QMC1 表 去文件头 QQ 变体 3DES 解密 zlib 解压得 XML

use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::ZlibDecoder;

const HEADER_LEN: usize = 11;
const KEY: &[u8; 24] = b"!@#)(*$%123ZXC!@!@#)(NHL";
const SUFFIX: &str = "_qm.qrc";

const QMC1: [u8; 128] = [
    0xc3, 0x4a, 0xd6, 0xca, 0x90, 0x67, 0xf7, 0x52, 0xd8, 0xa1, 0x66, 0x62, 0x9f, 0x5b, 0x09, 0x00,
    0xc3, 0x5e, 0x95, 0x23, 0x9f, 0x13, 0x11, 0x7e, 0xd8, 0x92, 0x3f, 0xbc, 0x90, 0xbb, 0x74, 0x0e,
    0xc3, 0x47, 0x74, 0x3d, 0x90, 0xaa, 0x3f, 0x51, 0xd8, 0xf4, 0x11, 0x84, 0x9f, 0xde, 0x95, 0x1d,
    0xc3, 0xc6, 0x09, 0xd5, 0x9f, 0xfa, 0x66, 0xf9, 0xd8, 0xf0, 0xf7, 0xa0, 0x90, 0xa1, 0xd6, 0xf3,
    0xc3, 0xf3, 0xd6, 0xa1, 0x90, 0xa0, 0xf7, 0xf0, 0xd8, 0xf9, 0x66, 0xfa, 0x9f, 0xd5, 0x09, 0xc6,
    0xc3, 0x1d, 0x95, 0xde, 0x9f, 0x84, 0x11, 0xf4, 0xd8, 0x51, 0x3f, 0xaa, 0x90, 0x3d, 0x74, 0x47,
    0xc3, 0x0e, 0x74, 0xbb, 0x90, 0xbc, 0x3f, 0x92, 0xd8, 0x7e, 0x11, 0x13, 0x9f, 0x23, 0x95, 0x5e,
    0xc3, 0x00, 0x09, 0x5b, 0x9f, 0x62, 0x66, 0xa1, 0xd8, 0x52, 0xf7, 0x67, 0x90, 0xca, 0xd6, 0x4a,
];

const IP: [u8; 64] = [
    58, 50, 42, 34, 26, 18, 10, 2, 60, 52, 44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6,
    64, 56, 48, 40, 32, 24, 16, 8, 57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3, 61,
    53, 45, 37, 29, 21, 13, 5, 63, 55, 47, 39, 31, 23, 15, 7,
];
const FP: [u8; 64] = [
    40, 8, 48, 16, 56, 24, 64, 32, 39, 7, 47, 15, 55, 23, 63, 31, 38, 6, 46, 14, 54, 22, 62, 30,
    37, 5, 45, 13, 53, 21, 61, 29, 36, 4, 44, 12, 52, 20, 60, 28, 35, 3, 43, 11, 51, 19, 59, 27,
    34, 2, 42, 10, 50, 18, 58, 26, 33, 1, 41, 9, 49, 17, 57, 25,
];
const E: [u8; 48] = [
    32, 1, 2, 3, 4, 5, 4, 5, 6, 7, 8, 9, 8, 9, 10, 11, 12, 13, 12, 13, 14, 15, 16, 17, 16, 17, 18,
    19, 20, 21, 20, 21, 22, 23, 24, 25, 24, 25, 26, 27, 28, 29, 28, 29, 30, 31, 32, 1,
];
const P: [u8; 32] = [
    16, 7, 20, 21, 29, 12, 28, 17, 1, 15, 23, 26, 5, 18, 31, 10, 2, 8, 24, 14, 32, 27, 3, 9, 19,
    13, 30, 6, 22, 11, 4, 25,
];
const PC1: [u8; 56] = [
    57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2, 59, 51, 43, 35, 27, 19, 11, 3, 60,
    52, 44, 36, 63, 55, 47, 39, 31, 23, 15, 7, 62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29,
    21, 13, 5, 28, 20, 12, 4,
];
const PC2: [u8; 48] = [
    14, 17, 11, 24, 1, 5, 3, 28, 15, 6, 21, 10, 23, 19, 12, 4, 26, 8, 16, 7, 27, 20, 13, 2, 41, 52,
    31, 37, 47, 55, 30, 40, 51, 45, 33, 48, 44, 49, 39, 56, 34, 53, 46, 42, 50, 36, 29, 32,
];
const SHIFTS: [u32; 16] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];

/// 标准 DES 表 仅 S2 与 S4 各错一格 照抄 QQ
const S: [[u8; 64]; 8] = [
    [
        14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7, 0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12,
        11, 9, 5, 3, 8, 4, 1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0, 15, 12, 8, 2, 4, 9,
        1, 7, 5, 11, 3, 14, 10, 0, 6, 13,
    ],
    [
        15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10, 3, 13, 4, 7, 15, 2, 8, 15, 12, 0, 1,
        10, 6, 9, 11, 5, 0, 14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15, 13, 8, 10, 1, 3, 15,
        4, 2, 11, 6, 7, 12, 0, 5, 14, 9,
    ],
    [
        10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8, 13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5,
        14, 12, 11, 15, 1, 13, 6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7, 1, 10, 13, 0, 6,
        9, 8, 7, 4, 15, 14, 3, 11, 5, 2, 12,
    ],
    [
        7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15, 13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2,
        12, 1, 10, 14, 9, 10, 6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4, 3, 15, 0, 6, 10,
        10, 13, 8, 9, 4, 5, 11, 12, 7, 2, 14,
    ],
    [
        2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9, 14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15,
        10, 3, 9, 8, 6, 4, 2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14, 11, 8, 12, 7, 1, 14,
        2, 13, 6, 15, 0, 9, 10, 4, 5, 3,
    ],
    [
        12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11, 10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13,
        14, 0, 11, 3, 8, 9, 14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6, 4, 3, 2, 12, 9, 5,
        15, 10, 11, 14, 1, 7, 6, 0, 8, 13,
    ],
    [
        4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1, 13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5,
        12, 2, 15, 8, 6, 1, 4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2, 6, 11, 13, 8, 1, 4,
        10, 7, 9, 5, 0, 15, 14, 2, 3, 12,
    ],
    [
        13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7, 1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6,
        11, 0, 14, 9, 2, 7, 11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8, 2, 1, 14, 7, 4, 10,
        8, 13, 15, 12, 9, 0, 3, 5, 6, 11,
    ],
];

/// 表中位号从 1 起 自高位数
fn permute(input: u64, width: u32, table: &[u8]) -> u64 {
    table.iter().fold(0, |acc, &p| {
        (acc << 1) | ((input >> (width - u32::from(p))) & 1)
    })
}

/// 右半压缩错位一位 越界补零 照抄 QQ
fn compress(cd: u64) -> u64 {
    PC2.iter().enumerate().fold(0, |acc, (i, &p)| {
        let bit = u32::from(p) + u32::from(i >= 24);
        let v = if bit > 56 { 0 } else { (cd >> (56 - bit)) & 1 };
        (acc << 1) | v
    })
}

fn subkeys(key: u64) -> [u64; 16] {
    let k = permute(key, 64, &PC1);
    let (mut c, mut d) = (k >> 28, k & 0x0fff_ffff);
    SHIFTS.map(|s| {
        c = ((c << s) | (c >> (28 - s))) & 0x0fff_ffff;
        d = ((d << s) | (d >> (28 - s))) & 0x0fff_ffff;
        compress((c << 28) | d)
    })
}

fn feistel(r: u64, k: u64) -> u64 {
    let x = permute(r, 32, &E) ^ k;
    let out = (0..8).fold(0u64, |acc, i| {
        let six = (x >> (42 - 6 * i)) & 0x3f;
        let row = ((six >> 4) & 2) | (six & 1);
        let col = (six >> 1) & 0xf;
        (acc << 4) | u64::from(S[i][(row * 16 + col) as usize])
    });
    permute(out, 32, &P)
}

fn des(block: u64, keys: &[u64; 16], decrypt: bool) -> u64 {
    let v = permute(block, 64, &IP);
    let (mut l, mut r) = (v >> 32, v & 0xffff_ffff);
    for i in 0..16 {
        let k = keys[if decrypt { 15 - i } else { i }];
        (l, r) = (r, l ^ feistel(r, k));
    }
    permute((r << 32) | l, 64, &FP)
}

/// 每 4 字节内倒序 块与密钥皆然
fn load(b: &[u8; 8]) -> u64 {
    let hi = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let lo = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    (u64::from(hi) << 32) | u64::from(lo)
}

fn store(v: u64) -> [u8; 8] {
    let mut out = [0u8; 8];
    out[..4].copy_from_slice(&((v >> 32) as u32).to_le_bytes());
    out[4..].copy_from_slice(&(v as u32).to_le_bytes());
    out
}

fn keys() -> [[u64; 16]; 3] {
    let (parts, _) = KEY.as_chunks::<8>();
    [0, 1, 2].map(|i| subkeys(load(&parts[i])))
}

fn qmc1(data: &mut [u8]) {
    for (i, b) in data.iter_mut().enumerate() {
        let idx = if i > 0x7fff {
            (i % 0x7fff) & 0x7f
        } else {
            i & 0x7f
        };
        *b ^= QMC1[idx];
    }
}

pub fn decrypt(file: &[u8]) -> Option<String> {
    let mut data = file.to_vec();
    qmc1(&mut data);
    let body = data.get(HEADER_LEN..)?;
    let [k1, k2, k3] = keys();
    let plain: Vec<u8> = body
        .as_chunks::<8>()
        .0
        .iter()
        .flat_map(|b| store(des(des(des(load(b), &k3, true), &k2, false), &k1, true)))
        .collect();
    let mut xml = String::new();
    ZlibDecoder::new(plain.as_slice())
        .read_to_string(&mut xml)
        .ok()?;
    Some(xml)
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn tag<'a>(content: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("[{name}:");
    let line = content.lines().find(|l| l.trim().starts_with(&open))?;
    line.trim().strip_prefix(&open)?.strip_suffix(']')
}

/// 去掉逐字时间 `字(起,长)`
fn strip_word_times(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('(') {
        out.push_str(&rest[..open]);
        let tail = &rest[open + 1..];
        let stamp = tail.find(')').map(|close| (&tail[..close], close));
        match stamp {
            Some((inner, close))
                if inner
                    .split_once(',')
                    .is_some_and(|(a, b)| is_num(a) && is_num(b)) =>
            {
                rest = &tail[close + 1..];
            }
            _ => {
                out.push('(');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

fn is_num(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

pub struct Qrc {
    pub title: String,
    pub artist: String,
    pub lines: Vec<(i64, String)>,
}

pub fn parse(xml: &str) -> Option<Qrc> {
    const ATTR: &str = "LyricContent=\"";
    let start = xml.find(ATTR)? + ATTR.len();
    let content = unescape(&xml[start..start + xml[start..].find('"')?]);
    let lines = content
        .lines()
        .filter_map(|l| {
            let (stamp, text) = l.trim().strip_prefix('[')?.split_once(']')?;
            let (time, _) = stamp.split_once(',')?;
            let text = strip_word_times(text).trim().to_string();
            (!text.is_empty()).then_some((time.parse().ok()?, text))
        })
        .collect();
    Some(Qrc {
        title: tag(&content, "ti").unwrap_or_default().to_string(),
        artist: tag(&content, "ar").unwrap_or_default().to_string(),
        lines,
    })
}

pub fn cache_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(Path::new(&appdata).join(r"Tencent\QQMusic\QQMusicCache\QQMusicLyricNew"))
}

/// 文件名为 `歌手 - 歌名 - 秒数 - 专辑_qm.qrc` 歌名可能被改写 按秒数筛再比标签
/// 部分缓存标签为空 退回比文件名
pub fn find(dir: &Path, title: &str, artist: &str, duration_ms: i64) -> Option<(String, Qrc)> {
    let secs = duration_ms / 1000;
    let stamps: Vec<String> = (secs - 1..=secs + 1)
        .map(|s| format!(" - {s} - "))
        .collect();
    let titled = format!(" - {title} - ");
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.ends_with(SUFFIX) || !stamps.iter().any(|s| name.contains(s.as_str())) {
            return None;
        }
        let xml = decrypt(&std::fs::read(e.path()).ok()?)?;
        let qrc = parse(&xml)?;
        let matched = if qrc.title.is_empty() {
            name.contains(&titled)
        } else {
            same(&qrc.title, title) && (qrc.artist.is_empty() || same(&qrc.artist, artist))
        };
        matched.then_some((xml, qrc))
    })
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn encrypt(xml: &str) -> Vec<u8> {
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(xml.as_bytes()).unwrap();
        let mut plain = z.finish().unwrap();
        plain.resize(plain.len().div_ceil(8) * 8, 0);
        let [k1, k2, k3] = keys();
        let mut file = vec![0u8; HEADER_LEN];
        file.extend(
            plain
                .as_chunks::<8>()
                .0
                .iter()
                .flat_map(|b| store(des(des(des(load(b), &k1, false), &k2, true), &k3, false))),
        );
        qmc1(&mut file);
        file
    }

    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<QrcInfos>
<LyricInfo LyricCount="1">
<Lyric_1 LyricType="1" LyricContent="[ti:DD backseat]
[ar:Top Barry/Rapeter]
[offset:0]
[2042,2041]词(2042,408)：(2450,408)Top (2859,408)Barry/(3267,408)Rapeter(3676,408)
[41571,1332]我不是个(41571,300)bad (41871,300)guy(42171,300)
[44000,0]
[45000,800]Tom &amp; Jerry (Live)(45000,800)
"/>
</LyricInfo>
</QrcInfos>"#;

    #[test]
    fn decrypts_own_encryption() {
        assert_eq!(decrypt(&encrypt(XML)).as_deref(), Some(XML));
    }

    #[test]
    fn parses_lines_and_tags() {
        let q = parse(XML).unwrap();
        assert_eq!(q.title, "DD backseat");
        assert_eq!(q.artist, "Top Barry/Rapeter");
        assert_eq!(
            q.lines,
            vec![
                (2042, "词：Top Barry/Rapeter".to_string()),
                (41571, "我不是个bad guy".to_string()),
                (45000, "Tom & Jerry (Live)".to_string()),
            ]
        );
    }

    #[test]
    fn finds_by_tags_or_file_name() {
        let dir = std::env::temp_dir().join(format!("qrc-find-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let untagged = XML
            .replace("[ti:DD backseat]", "[ti:]")
            .replace("[ar:Top Barry/Rapeter]", "[ar:]");
        std::fs::write(
            dir.join("可楼 - 隔岸 - 231 - 隔岸_qm.qrc"),
            encrypt(&untagged),
        )
        .unwrap();
        std::fs::write(
            dir.join("Top Barry_Rapeter - DD backseat - 196 - DD backseat_qm.qrc"),
            encrypt(XML),
        )
        .unwrap();
        assert!(find(&dir, "DD backseat", "Top Barry/Rapeter", 196_875).is_some());
        assert!(find(&dir, "隔岸", "可楼", 231_500).is_some());
        assert!(find(&dir, "DD backseat", "Top Barry/Rapeter", 150_000).is_none());
        assert!(find(&dir, "林宛瑜", "Rapeter", 196_875).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(decrypt(&[0u8; 64]).is_none());
        assert!(decrypt(&[0u8; 4]).is_none());
    }
}
