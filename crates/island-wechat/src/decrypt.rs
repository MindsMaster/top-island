//! SQLCipher 4（微信 4.x 定制）逐页解密 + WAL 帧叠加。
//!
//! 页布局（4096 字节）：[salt(16, 仅第 1 页) | 密文 | iv(16) | hmac-sha512(64)]，
//! 保留区 80 字节向上对齐 AES 块后仍是 80。mac_key 的 salt 是页 salt 逐字节异或 0x3a。
//! Electron 版 decrypt.ts 里还有一份每 256 页让出事件循环的 async 拷贝——那是
//! 为不堵 JS 主线程写的逐字重复，Rust 走阻塞线程池，只保留一份同步实现。

use std::collections::BTreeMap;
use std::sync::Mutex;

use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac_array;
use sha2::Sha512;

pub const PAGE_SIZE: usize = 4096;
pub const SALT_SIZE: usize = 16;
pub const IV_SIZE: usize = 16;
pub const HMAC_SIZE: usize = 64;
pub const RESERVE: usize = IV_SIZE + HMAC_SIZE;
pub const KEY_SIZE: usize = 32;
pub const KDF_ROUNDS: u32 = 256000;

const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";
const WAL_HEADER_SIZE: usize = 32;
const WAL_FRAME_HEADER_SIZE: usize = 24;

type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
type HmacSha512 = Hmac<Sha512>;

/// key 可能是裸 enc_key，也可能是 SQLCipher passphrase（微信 4.x 是后者）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMode {
    RawEncKey,
    SqlcipherPassphrase,
}

#[derive(Debug, Clone)]
pub struct KeyMaterial {
    pub enc_key: [u8; KEY_SIZE],
    pub mac_key: [u8; KEY_SIZE],
    pub mode: KeyMode,
}

pub fn mac_salt_of(salt: &[u8]) -> [u8; SALT_SIZE] {
    let mut m = [0u8; SALT_SIZE];
    for (i, b) in salt.iter().take(SALT_SIZE).enumerate() {
        m[i] = b ^ 0x3a;
    }
    m
}

/// 页 HMAC：内容区间（第 1 页跳过 salt）+ 页号 u32 LE
fn page_hmac(mac_key: &[u8; KEY_SIZE], page: &[u8], page_num: u32) -> [u8; HMAC_SIZE] {
    let start = if page_num == 1 { SALT_SIZE } else { 0 };
    let mut mac = <HmacSha512 as Mac>::new_from_slice(mac_key).expect("HMAC-SHA512 任意长度密钥都合法");
    mac.update(&page[start..PAGE_SIZE - RESERVE + IV_SIZE]);
    mac.update(&page_num.to_le_bytes());
    mac.finalize().into_bytes().into()
}

fn hmac_matches(mac_key: &[u8; KEY_SIZE], page1: &[u8]) -> bool {
    let stored = &page1[PAGE_SIZE - HMAC_SIZE..PAGE_SIZE];
    page_hmac(mac_key, page1, 1) == stored
}

/// 按第 1 页 HMAC 判定 key 的形态；对不上返回 None
pub fn resolve_key_material(key: &[u8; KEY_SIZE], page1: &[u8]) -> Option<KeyMaterial> {
    if page1.len() < PAGE_SIZE {
        return None;
    }
    let salt = &page1[..SALT_SIZE];
    let mac_salt = mac_salt_of(salt);

    // 1) key 直接作为 enc_key
    let mac_key = pbkdf2_hmac_array::<Sha512, KEY_SIZE>(key, &mac_salt, 2);
    if hmac_matches(&mac_key, page1) {
        return Some(KeyMaterial { enc_key: *key, mac_key, mode: KeyMode::RawEncKey });
    }
    // 2) key 作为 passphrase 派生 enc_key
    let enc_key = pbkdf2_hmac_array::<Sha512, KEY_SIZE>(key, salt, KDF_ROUNDS);
    let mac_key = pbkdf2_hmac_array::<Sha512, KEY_SIZE>(&enc_key, &mac_salt, 2);
    if hmac_matches(&mac_key, page1) {
        return Some(KeyMaterial { enc_key, mac_key, mode: KeyMode::SqlcipherPassphrase });
    }
    None
}

// passphrase 模式的 PBKDF2(256000) 是明确的热路径开销：同一库 salt 不变，按 salt 缓存派生结果
static KM_CACHE: Mutex<Option<std::collections::HashMap<[u8; SALT_SIZE], KeyMaterial>>> =
    Mutex::new(None);

pub fn key_material(key: &[u8; KEY_SIZE], page1: &[u8]) -> Option<KeyMaterial> {
    if page1.len() < SALT_SIZE {
        return None;
    }
    let salt: [u8; SALT_SIZE] = page1[..SALT_SIZE].try_into().ok()?;
    {
        let guard = KM_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(km) = guard.as_ref().and_then(|m| m.get(&salt)) {
            return Some(km.clone());
        }
    }
    let km = resolve_key_material(key, page1)?;
    let mut guard = KM_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    guard.get_or_insert_with(Default::default).insert(salt, km.clone());
    Some(km)
}

/// 解密一页为明文页：第 1 页补回 SQLite 头（密文里这段被 salt 占了），尾部保留区清零
fn decrypt_page(enc_key: &[u8; KEY_SIZE], page: &[u8], page_num: u32) -> [u8; PAGE_SIZE] {
    debug_assert_eq!(page.len(), PAGE_SIZE);
    let iv = &page[PAGE_SIZE - RESERVE..PAGE_SIZE - RESERVE + IV_SIZE];
    let offset = if page_num == 1 { SALT_SIZE } else { 0 };
    let body = &page[offset..PAGE_SIZE - RESERVE];

    let mut out = [0u8; PAGE_SIZE];
    let mut body_buf = body.to_vec();
    let dec = Aes256CbcDec::new(enc_key.into(), iv.into())
        .decrypt_padded_mut::<NoPadding>(&mut body_buf)
        .expect("密文区间长度恒为 AES 块的整数倍");
    let body_start = if page_num == 1 {
        out[..SQLITE_HEADER.len()].copy_from_slice(SQLITE_HEADER);
        SQLITE_HEADER.len()
    } else {
        0
    };
    out[body_start..body_start + dec.len()].copy_from_slice(dec);
    out
}

/// 全零页是 SQLite 的空洞页，不解密直接留零（每 512 字节抽样足够判定）
fn is_all_zero(page: &[u8]) -> bool {
    (0..PAGE_SIZE).step_by(512).all(|i| page[i] == 0)
}

fn read_u32_be(buf: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(buf[at..at + 4].try_into().expect("切片长度已检查"))
}

/// 解析 WAL，收集每个页号的最新加密帧。checkpoint 重置 WAL 会换 salt，
/// salt 与头部不一致的帧是上一轮的残留，遇到即截断。
pub fn collect_wal_frames(wal: Option<&[u8]>) -> BTreeMap<u32, Vec<u8>> {
    let mut frames = BTreeMap::new();
    let Some(wal) = wal else { return frames };
    if wal.len() < WAL_HEADER_SIZE + WAL_FRAME_HEADER_SIZE + PAGE_SIZE {
        return frames;
    }
    if read_u32_be(wal, 8) != PAGE_SIZE as u32 {
        return frames; // WAL 页大小必须与库一致，否则不是配套的 WAL
    }
    let salt1 = read_u32_be(wal, 16);
    let salt2 = read_u32_be(wal, 20);
    let mut o = WAL_HEADER_SIZE;
    while o + WAL_FRAME_HEADER_SIZE + PAGE_SIZE <= wal.len() {
        if read_u32_be(wal, o + 8) != salt1 || read_u32_be(wal, o + 12) != salt2 {
            break;
        }
        let page_num = read_u32_be(wal, o);
        if page_num >= 1 {
            let start = o + WAL_FRAME_HEADER_SIZE;
            frames.insert(page_num, wal[start..start + PAGE_SIZE].to_vec()); // 后帧覆盖前帧
        }
        o += WAL_FRAME_HEADER_SIZE + PAGE_SIZE;
    }
    frames
}

/// 解密整个库并叠加 WAL 里尚未 checkpoint 的最新页——新消息先落 WAL，
/// 不等主库 checkpoint 就能读到。key 对不上返回 None。
pub fn decrypt_database_with_wal(
    key: &[u8; KEY_SIZE],
    file: &[u8],
    wal: Option<&[u8]>,
) -> Option<Vec<u8>> {
    if file.len() < PAGE_SIZE {
        return None;
    }
    let km = key_material(key, &file[..PAGE_SIZE])?;

    let main_pages = file.len() / PAGE_SIZE;
    let frames = collect_wal_frames(wal);
    let max_page = frames.keys().copied().fold(main_pages as u32, |m, p| m.max(p));

    let mut out = vec![0u8; max_page as usize * PAGE_SIZE];
    for p in 0..main_pages {
        let page_num = p as u32 + 1;
        if frames.contains_key(&page_num) {
            continue; // 被 WAL 覆盖的页以 WAL 帧为准
        }
        let page = &file[p * PAGE_SIZE..(p + 1) * PAGE_SIZE];
        if is_all_zero(page) {
            continue;
        }
        let dec = decrypt_page(&km.enc_key, page, page_num);
        out[p * PAGE_SIZE..(p + 1) * PAGE_SIZE].copy_from_slice(&dec);
    }
    for (page_num, enc_page) in &frames {
        let dec = decrypt_page(&km.enc_key, enc_page, *page_num);
        let at = (*page_num as usize - 1) * PAGE_SIZE;
        out[at..at + PAGE_SIZE].copy_from_slice(&dec);
    }
    Some(out)
}

pub fn decrypt_database(key: &[u8; KEY_SIZE], file: &[u8]) -> Option<Vec<u8>> {
    decrypt_database_with_wal(key, file, None)
}

/// 测试/密钥扫描共用的夹具构造：按同一套页格式把明文页加密回去
#[cfg(test)]
pub(crate) mod fixture {
    use super::*;
    use aes::cipher::BlockEncryptMut;

    type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

    /// 用 passphrase 模式派生 enc/mac key，把明文页打包成 SQLCipher 加密页
    pub(crate) fn encrypt_page(passphrase: &[u8; KEY_SIZE], salt: &[u8; SALT_SIZE], page_num: u32, plain: &[u8]) -> Vec<u8> {
        assert_eq!(plain.len(), PAGE_SIZE);
        let enc_key = pbkdf2_hmac_array::<Sha512, KEY_SIZE>(passphrase, salt, KDF_ROUNDS);
        let mac_key = pbkdf2_hmac_array::<Sha512, KEY_SIZE>(&enc_key, &mac_salt_of(salt), 2);

        let mut page = vec![0u8; PAGE_SIZE];
        if page_num == 1 {
            page[..SALT_SIZE].copy_from_slice(salt);
        }
        let offset = if page_num == 1 { SALT_SIZE } else { 0 };
        let body = &plain[offset..PAGE_SIZE - RESERVE];
        let mut body_buf = body.to_vec();
        let body_len = body_buf.len();
        let enc = Aes256CbcEnc::new(&enc_key.into(), &[0x5au8; IV_SIZE].into())
            .encrypt_padded_mut::<NoPadding>(&mut body_buf, body_len)
            .expect("明文区间长度恒为 AES 块的整数倍");
        page[offset..offset + enc.len()].copy_from_slice(enc);
        page[PAGE_SIZE - RESERVE..PAGE_SIZE - RESERVE + IV_SIZE].copy_from_slice(&[0x5au8; IV_SIZE]);
        let digest = page_hmac(&mac_key, &page, page_num);
        page[PAGE_SIZE - HMAC_SIZE..].copy_from_slice(&digest);
        page
    }

    /// 造一个合法明文页：page1 带 SQLite 头，其余填指定字节
    pub(crate) fn plain_page(page_num: u32, fill: u8) -> Vec<u8> {
        let mut p = vec![fill; PAGE_SIZE];
        if page_num == 1 {
            p[..SQLITE_HEADER.len()].copy_from_slice(SQLITE_HEADER);
        }
        p[PAGE_SIZE - RESERVE..].fill(0);
        p
    }

    pub(crate) fn wal_bytes(salt1: u32, salt2: u32, frames: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut wal = Vec::new();
        wal.extend_from_slice(&0x002de218u32.to_be_bytes()); // WAL 魔数
        wal.extend_from_slice(&3007000u32.to_be_bytes()); // 格式版本
        wal.extend_from_slice(&(PAGE_SIZE as u32).to_be_bytes());
        wal.extend_from_slice(&0u32.to_be_bytes()); // checkpoint 序号
        wal.extend_from_slice(&salt1.to_be_bytes());
        wal.extend_from_slice(&salt2.to_be_bytes());
        wal.extend_from_slice(&[0u8; 8]); // header checksum（解析器不校验）
        for (page_num, page) in frames {
            wal.extend_from_slice(&page_num.to_be_bytes());
            wal.extend_from_slice(&0u32.to_be_bytes()); // commit 后的库大小
            wal.extend_from_slice(&salt1.to_be_bytes());
            wal.extend_from_slice(&salt2.to_be_bytes());
            wal.extend_from_slice(&[0u8; 8]); // frame checksum
            wal.extend_from_slice(page);
        }
        wal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASS: [u8; KEY_SIZE] = [0x42; KEY_SIZE];
    const SALT: [u8; SALT_SIZE] = [0x11; SALT_SIZE];

    #[test]
    fn decrypt_roundtrip_restores_plaintext_pages() {
        let p1 = fixture::plain_page(1, 0xAA);
        let p2 = fixture::plain_page(2, 0xBB);
        let mut db = fixture::encrypt_page(&PASS, &SALT, 1, &p1);
        db.extend_from_slice(&fixture::encrypt_page(&PASS, &SALT, 2, &p2));

        let out = decrypt_database(&PASS, &db).expect("正确密钥必须能解密");
        assert_eq!(&out[..16], SQLITE_HEADER, "第 1 页必须补回 SQLite 头，否则 rusqlite 打不开");
        assert_eq!(&out[16..PAGE_SIZE - RESERVE], &p1[16..PAGE_SIZE - RESERVE], "第 1 页正文解密后与明文不一致");
        assert_eq!(&out[PAGE_SIZE..PAGE_SIZE + 64], &p2[..64], "第 2 页正文解密后与明文不一致");
    }

    #[test]
    fn wrong_passphrase_is_rejected_by_page1_hmac() {
        let p1 = fixture::plain_page(1, 0xAA);
        let db = fixture::encrypt_page(&PASS, &SALT, 1, &p1);
        let wrong = [0x99; KEY_SIZE];
        assert!(decrypt_database(&wrong, &db).is_none(), "错误密钥必须返回 None，而不是产出乱码明文");
    }

    #[test]
    fn wal_frame_overlays_main_db_page() {
        let p1 = fixture::plain_page(1, 0xAA);
        let p2_main = fixture::plain_page(2, 0xBB);
        let mut db = fixture::encrypt_page(&PASS, &SALT, 1, &p1);
        db.extend_from_slice(&fixture::encrypt_page(&PASS, &SALT, 2, &p2_main));

        let p2_new = fixture::plain_page(2, 0xCC);
        let enc_p2_new = fixture::encrypt_page(&PASS, &SALT, 2, &p2_new);
        let wal = fixture::wal_bytes(7, 8, &[(2, enc_p2_new)]);

        let out = decrypt_database_with_wal(&PASS, &db, Some(&wal)).expect("解密必须成功");
        assert_eq!(out[PAGE_SIZE], 0xCC, "WAL 里的新页必须覆盖主库旧页，否则新消息要等 checkpoint 才可见");
    }

    #[test]
    fn wal_frame_extends_beyond_main_db() {
        let p1 = fixture::plain_page(1, 0xAA);
        let db = fixture::encrypt_page(&PASS, &SALT, 1, &p1);
        let p9 = fixture::plain_page(9, 0x77);
        let wal = fixture::wal_bytes(1, 2, &[(9, fixture::encrypt_page(&PASS, &SALT, 9, &p9))]);

        let out = decrypt_database_with_wal(&PASS, &db, Some(&wal)).expect("解密必须成功");
        assert_eq!(out.len(), 9 * PAGE_SIZE, "WAL 引用了主库之外的页号时输出必须扩到该页");
        assert_eq!(out[8 * PAGE_SIZE], 0x77, "扩展页内容必须来自 WAL 帧");
    }

    #[test]
    fn wal_frames_with_stale_salt_are_ignored() {
        // checkpoint 后 WAL 头部换了 salt，旧 salt 的帧是残留，读到就停
        let mut wal = fixture::wal_bytes(100, 200, &[(3, vec![0xEE; PAGE_SIZE])]);
        let mut stale = Vec::new();
        stale.extend_from_slice(&4u32.to_be_bytes());
        stale.extend_from_slice(&0u32.to_be_bytes());
        stale.extend_from_slice(&999u32.to_be_bytes()); // salt1 不匹配
        stale.extend_from_slice(&888u32.to_be_bytes());
        stale.extend_from_slice(&[0u8; 8]);
        stale.extend_from_slice(&vec![0xDD; PAGE_SIZE]);
        wal.extend_from_slice(&stale);

        let frames = collect_wal_frames(Some(&wal));
        assert!(frames.contains_key(&3), "salt 匹配的帧必须被收集");
        assert!(!frames.contains_key(&4), "salt 不匹配的残留帧必须被丢弃，否则会把上个 checkpoint 周期的旧数据叠进来");
    }

    #[test]
    fn later_frame_for_same_page_wins() {
        let wal = fixture::wal_bytes(1, 1, &[(2, vec![0x01; PAGE_SIZE]), (2, vec![0x02; PAGE_SIZE])]);
        let frames = collect_wal_frames(Some(&wal));
        assert_eq!(frames[&2][0], 0x02, "同一页号的后一帧必须覆盖前一帧（WAL 语义：越靠后越新）");
    }

    #[test]
    fn resolve_key_material_accepts_raw_enc_key_mode() {
        // raw 模式：key 本身就是 enc_key，mac_key 只经 2 轮 PBKDF2
        let enc_key = [0x07; KEY_SIZE];
        let mac_key = pbkdf2_hmac_array::<Sha512, KEY_SIZE>(&enc_key, &mac_salt_of(&SALT), 2);
        let mut page1 = fixture::plain_page(1, 0x33);
        page1[..SALT_SIZE].copy_from_slice(&SALT);
        let digest = page_hmac(&mac_key, &page1, 1);
        page1[PAGE_SIZE - HMAC_SIZE..].copy_from_slice(&digest);

        let km = resolve_key_material(&enc_key, &page1).expect("raw enc_key 必须通过校验");
        assert_eq!(km.mode, KeyMode::RawEncKey, "key 直接作为 enc_key 时应判定为 RawEncKey 模式");
    }
}
