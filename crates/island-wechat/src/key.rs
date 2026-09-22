use std::collections::HashSet;
use std::path::{Path, PathBuf};

use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac_array;
use rayon::prelude::*;
use regex::bytes::Regex;
use sha2::Sha512;
use zeroize::Zeroizing;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Memory::{
    VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_IMAGE, MEM_MAPPED, MEM_PRIVATE, PAGE_READWRITE,
    PAGE_WRITECOPY,
};
use windows::Win32::System::ProcessStatus::GetMappedFileNameW;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

use crate::decrypt::{self, HMAC_SIZE, KDF_ROUNDS, KEY_SIZE, PAGE_SIZE, SALT_SIZE};
use crate::error::{Result, WeChatError};

/// Weixin.exe 为 4.x WeChat.exe 旧名
const WECHAT_EXE_NAMES: [&str; 2] = ["weixin.exe", "wechat.exe"];

#[derive(Debug)]
pub struct RecoveredKey {
    pub key: Zeroizing<[u8; KEY_SIZE]>,
    /// 目录名推断 仅供展示
    pub wxid: String,
}

struct MemRegion {
    base: usize,
    size: usize,
    protect: u32,
    mtype: u32,
    filename: Option<String>,
}

/// 主辅进程同名 全部返回逐个试
fn find_wechat_pids() -> Vec<u32> {
    let mut pids = vec![];
    unsafe {
        let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return pids,
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
                if WECHAT_EXE_NAMES.contains(&name.as_str()) {
                    pids.push(entry.th32ProcessID);
                }
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    pids
}

/// 含映射文件名 定位 Weixin.dll 用
fn get_mem_list(hprocess: HANDLE) -> Vec<MemRegion> {
    let mut out = vec![];
    unsafe {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        let mut p: usize = 0x10000;
        while VirtualQueryEx(
            hprocess,
            Some(p as _),
            &mut mbi,
            std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
        ) == std::mem::size_of::<MEMORY_BASIC_INFORMATION>()
        {
            let base = mbi.BaseAddress as usize;
            let size = mbi.RegionSize;
            if size == 0 {
                break;
            }
            let mut filename = None;
            if mbi.Type == MEM_IMAGE || mbi.Type == MEM_MAPPED {
                let mut buf = [0u16; 512];
                let n = GetMappedFileNameW(hprocess, mbi.BaseAddress, &mut buf) as usize;
                if n > 0 {
                    filename = Some(String::from_utf16_lossy(&buf[..n]));
                }
            }
            out.push(MemRegion {
                base,
                size,
                protect: mbi.Protect.0,
                mtype: mbi.Type.0,
                filename,
            });
            p = base + size;
        }
    }
    out
}

fn read_mem(hprocess: HANDLE, addr: usize, size: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; size];
    let mut read = 0usize;
    let ok = unsafe {
        ReadProcessMemory(
            hprocess,
            addr as _,
            buf.as_mut_ptr() as _,
            size,
            Some(&mut read as *mut usize),
        )
    };
    if ok.is_ok() && read == size {
        Some(buf)
    } else {
        None
    }
}

/// 汇编特征 4 条 48 BA <imm64> 间隔 3~8 字节 收尾 48 85 C0 同 dll_key_scan.py
fn internal_key_regex() -> &'static Regex {
    /// 每个内存区域都跑一遍 只编译一次
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?s-u)\x48\xBA(.{8}).{3,8}?\x48\xBA(.{8}).{3,8}?\x48\xBA(.{8}).{3,8}?\x48\xBA(.{8}).{3,8}?\x48\x85\xC0",
        )
        .expect("internal-key 正则是常量，编译期已验证")
    })
}

fn extract_internal_keys(buf: &[u8]) -> Vec<[u8; KEY_SIZE]> {
    let re = internal_key_regex();
    let mut keys = vec![];
    let mut seen = HashSet::new();
    for cap in re.captures_iter(buf) {
        let mut key = [0u8; KEY_SIZE];
        for i in 0..4 {
            key[i * 8..i * 8 + 8]
                .copy_from_slice(cap.get(i + 1).expect("捕获组数量固定").as_bytes());
        }
        if seen.insert(key) {
            keys.push(key);
        }
    }
    keys
}

fn scan_dll_internal_keys(hprocess: HANDLE, regions: &[MemRegion]) -> Vec<[u8; KEY_SIZE]> {
    let mut keys = vec![];
    for r in regions
        .iter()
        .filter(|r| matches!(&r.filename, Some(f) if f.to_lowercase().contains("weixin.dll")))
    {
        let Some(buf) = read_mem(hprocess, r.base, r.size) else {
            continue;
        };
        keys.extend(extract_internal_keys(&buf));
    }
    let mut seen = HashSet::new();
    keys.retain(|k| seen.insert(*k));
    keys
}

/// stub 前 8 字节为指向 key 的指针 布局 <ptr:8> 00 00 | 00×8 | 20 | 00×7 | 2f | 00×7
/// 同 wechat-dump-rs GetKeyAddrStub
fn stub_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?s-u).{6}\x00{2}\x00{8}\x20\x00{7}\x2f\x00{7}").expect("key-stub 正则是常量")
    })
}

fn extract_stub_pointers(buf: &[u8]) -> HashSet<u64> {
    stub_regex()
        .find_iter(buf)
        .map(|m| {
            u64::from_le_bytes(
                buf[m.start()..m.start() + 8]
                    .try_into()
                    .expect("匹配至少 8 字节"),
            )
        })
        .collect()
}

/// 只扫可写 MEM_PRIVATE 区
fn scan_key_candidates(hprocess: HANDLE, regions: &[MemRegion]) -> Vec<[u8; KEY_SIZE]> {
    let mut ptr_set: HashSet<u64> = HashSet::new();
    for r in regions.iter().filter(|r| {
        (r.protect & (PAGE_READWRITE.0 | PAGE_WRITECOPY.0)) != 0 && r.mtype == MEM_PRIVATE.0
    }) {
        let Some(buf) = read_mem(hprocess, r.base, r.size) else {
            continue;
        };
        ptr_set.extend(extract_stub_pointers(&buf));
    }

    let mut cands = vec![];
    let mut seen = HashSet::new();
    for ptr in ptr_set {
        if let Some(bytes) = read_mem(hprocess, ptr as usize, KEY_SIZE) {
            let mut key = [0u8; KEY_SIZE];
            key.copy_from_slice(&bytes);
            if is_potential_key(&key) && seen.insert(key) {
                cands.push(key);
            }
        }
    }
    cands
}

/// 滤掉非随机文本 同 key_v4.py is_potential_key
fn is_potential_key(key: &[u8; KEY_SIZE]) -> bool {
    let distinct = key.iter().collect::<HashSet<_>>().len();
    if distinct < 15 {
        return false;
    }
    let printable = key.iter().filter(|&&b| (32..=126).contains(&b)).count();
    printable <= 24
}

/// 首页校验 raw XOR internal 命中即明文 key
fn verify(raw: &[u8; KEY_SIZE], internal: &[u8; KEY_SIZE], page: &[u8]) -> Option<[u8; KEY_SIZE]> {
    let mut passphrase = [0u8; KEY_SIZE];
    for i in 0..KEY_SIZE {
        passphrase[i] = raw[i] ^ internal[i];
    }

    let salt = &page[..SALT_SIZE];
    let mac_salt = decrypt::mac_salt_of(salt);

    let new_key = pbkdf2_hmac_array::<Sha512, KEY_SIZE>(&passphrase, salt, KDF_ROUNDS);
    let mac_key = pbkdf2_hmac_array::<Sha512, KEY_SIZE>(&new_key, &mac_salt, 2);

    let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(&mac_key).ok()?;
    let hmac_start = PAGE_SIZE - HMAC_SIZE;
    mac.update(&page[SALT_SIZE..hmac_start]);
    mac.update(&1u32.to_le_bytes()); // 页号 1
    let digest = mac.finalize().into_bytes();

    if digest.as_slice() == &page[hmac_start..hmac_start + HMAC_SIZE] {
        Some(passphrase)
    } else {
        None
    }
}

const V4_DB_NAME_PRIORITY: [&str; 7] = [
    "msg0.db",
    "msg.db",
    "micromsg.db",
    "favorite.db",
    "mediamsg0.db",
    "media_msg0.db",
    "sns.db",
];

fn db_priority(name: &str) -> usize {
    let n = name.to_lowercase();
    V4_DB_NAME_PRIORITY
        .iter()
        .position(|&x| x == n)
        .unwrap_or(V4_DB_NAME_PRIORITY.len())
}

/// 收 >=4096 的 .db 跳过 key_info.db
fn collect_db_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_db_files(&p, out);
        } else if p
            .extension()
            .map(|x| x.eq_ignore_ascii_case("db"))
            .unwrap_or(false)
        {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if name == "key_info.db" {
                continue;
            }
            if std::fs::metadata(&p)
                .map(|m| m.len() >= PAGE_SIZE as u64)
                .unwrap_or(false)
            {
                out.push(p);
            }
        }
    }
}

fn read_probe_page(db_dir: &Path) -> Result<Vec<u8>> {
    let chosen = if db_dir.is_file() {
        if !db_dir
            .extension()
            .map(|x| x.eq_ignore_ascii_case("db"))
            .unwrap_or(false)
        {
            return Err(WeChatError::api(
                "探测库",
                format_args!("不是 .db 文件: {}", db_dir.display()),
            ));
        }
        db_dir.to_path_buf()
    } else {
        let mut dbs = vec![];
        collect_db_files(db_dir, &mut dbs);
        if dbs.is_empty() {
            return Err(WeChatError::api(
                "探测库",
                format_args!("目录内未找到可用于校验的 .db: {}", db_dir.display()),
            ));
        }
        dbs.sort_by(|a, b| {
            let (an, bn) = (
                a.file_name().unwrap_or_default().to_string_lossy(),
                b.file_name().unwrap_or_default().to_string_lossy(),
            );
            db_priority(&an)
                .cmp(&db_priority(&bn))
                .then_with(|| a.as_os_str().len().cmp(&b.as_os_str().len()))
        });
        dbs.into_iter().next().expect("已检查非空")
    };

    let buf = std::fs::read(&chosen).map_err(|e| WeChatError::io("读取探测库", e))?;
    if buf.len() < PAGE_SIZE {
        return Err(WeChatError::api(
            "探测库",
            format_args!("文件过小: {}", chosen.display()),
        ));
    }
    Ok(buf[..PAGE_SIZE].to_vec())
}

/// 账号目录名去 _xxxx 后缀 仅供展示
pub fn infer_wxid(db_dir: &Path) -> String {
    let mut p = db_dir.to_path_buf();
    // db_storage 的父目录是账号目录
    if p.file_name()
        .map(|n| n.eq_ignore_ascii_case("db_storage"))
        .unwrap_or(false)
    {
        if let Some(parent) = p.parent() {
            p = parent.to_path_buf();
        }
    }
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    crate::discover::strip_wxid_suffix(&name)
}

fn recover_from_pid(pid: u32, page: &[u8]) -> Option<[u8; KEY_SIZE]> {
    let hprocess =
        unsafe { OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, false, pid).ok()? };
    let result = (|| {
        let regions = get_mem_list(hprocess);

        // 全零 internal 等价不异或 兼容不掩码的旧版
        let mut internals = scan_dll_internal_keys(hprocess, &regions);
        internals.push([0u8; KEY_SIZE]);
        eprintln!(
            "[wechat] pid {}: DLL internal-key candidates={}, memory regions={}",
            pid,
            internals.len() - 1,
            regions.len()
        );

        let candidates = scan_key_candidates(hprocess, &regions);
        eprintln!(
            "[wechat] pid {}: key candidates after filtering={}",
            pid,
            candidates.len()
        );
        if candidates.is_empty() {
            return None;
        }

        // PBKDF2 是瓶颈 全部配对摊平进并行池
        let pairs: Vec<(&[u8; KEY_SIZE], &[u8; KEY_SIZE])> = candidates
            .iter()
            .flat_map(|raw| internals.iter().map(move |internal| (raw, internal)))
            .collect();
        pairs
            .par_iter()
            .find_map_any(|(raw, internal)| verify(raw, internal, page))
    })();
    unsafe {
        let _ = CloseHandle(hprocess);
    }
    result
}

/// db_dir 是 db_storage 目录或单个 .db
pub fn recover_key(db_dir: &Path) -> Result<RecoveredKey> {
    let page = read_probe_page(db_dir)?;
    let pids = find_wechat_pids();
    if pids.is_empty() {
        return Err(WeChatError::NotRunning);
    }
    eprintln!("[wechat] found wechat pids: {:?}", pids);

    for pid in pids {
        if let Some(key) = recover_from_pid(pid, &page) {
            return Ok(RecoveredKey {
                key: Zeroizing::new(key),
                wxid: infer_wxid(db_dir),
            });
        }
    }
    Err(WeChatError::NoValidKey)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_potential_key_rejects_low_entropy_buffers() {
        assert!(!is_potential_key(&[0u8; KEY_SIZE]));
        assert!(!is_potential_key(&[b'a'; KEY_SIZE]));
    }

    #[test]
    fn is_potential_key_rejects_mostly_printable_text() {
        let key = *b"this is a readable sentence!!012"; // 32 个可打印字符
        assert!(!is_potential_key(&key));
    }

    #[test]
    fn is_potential_key_accepts_looking_random_bytes() {
        let key: [u8; KEY_SIZE] = [
            0x9f, 0x12, 0xab, 0x00, 0x7e, 0xc3, 0x55, 0x01, 0xd8, 0x40, 0x99, 0xfa, 0x03, 0x6b,
            0x88, 0x21, 0xe0, 0x5c, 0x37, 0x94, 0x0a, 0xb1, 0x62, 0xcd, 0x18, 0x7b, 0xe6, 0x2f,
            0x4a, 0x90, 0x05, 0xd3,
        ];
        assert!(is_potential_key(&key));
    }

    #[test]
    fn extract_internal_keys_assembles_four_imm64_operands() {
        let mut buf = vec![0x90u8; 7]; // 前缀噪声
        let mut expect = [0u8; KEY_SIZE];
        for i in 0..4 {
            buf.extend_from_slice(&[0x48, 0xBA]);
            let imm = [i as u8 + 1; 8];
            expect[i * 8..i * 8 + 8].copy_from_slice(&imm);
            buf.extend_from_slice(&imm);
            buf.extend_from_slice(&[0x48, 0x89, 0xD1, 0x90]); // 4 字节间隔
        }
        buf.extend_from_slice(&[0x48, 0x85, 0xC0]);

        let keys = extract_internal_keys(&buf);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], expect);
    }

    #[test]
    fn extract_internal_keys_ignores_incomplete_sequence() {
        let mut buf = vec![];
        for i in 0..3 {
            buf.extend_from_slice(&[0x48, 0xBA]);
            buf.extend_from_slice(&[i; 8]);
            buf.extend_from_slice(&[0x90; 4]);
        }
        buf.extend_from_slice(&[0x48, 0x85, 0xC0]);
        assert!(extract_internal_keys(&buf).is_empty());
    }

    #[test]
    fn extract_stub_pointers_reads_le_pointer_before_marker() {
        let ptr: u64 = 0x000001AB_00020003;
        let mut buf = vec![0xCC; 3]; // 前缀噪声
        buf.extend_from_slice(&ptr.to_le_bytes()); // 高 2 字节为 00 命中 \x00{2}
        buf.extend_from_slice(&[0x00; 8]);
        buf.push(0x20);
        buf.extend_from_slice(&[0x00; 7]);
        buf.push(0x2f);
        buf.extend_from_slice(&[0x00; 7]);

        let ptrs = extract_stub_pointers(&buf);
        assert!(ptrs.contains(&ptr));
    }

    #[test]
    fn verify_accepts_raw_xor_internal_matching_passphrase() {
        let passphrase = [0x66; KEY_SIZE];
        let salt = [0x22; SALT_SIZE];
        let plain = crate::decrypt::fixture::plain_page(1, 0x44);
        let page = crate::decrypt::fixture::encrypt_page(&passphrase, &salt, 1, &plain);

        let internal = [0x0f; KEY_SIZE];
        let mut raw = [0u8; KEY_SIZE];
        for i in 0..KEY_SIZE {
            raw[i] = passphrase[i] ^ internal[i];
        }
        let got = verify(&raw, &internal, &page).expect("raw^internal 等于真实密钥时必须通过校验");
        assert_eq!(got, passphrase);
    }

    #[test]
    fn verify_rejects_wrong_pairing() {
        let passphrase = [0x66; KEY_SIZE];
        let salt = [0x22; SALT_SIZE];
        let plain = crate::decrypt::fixture::plain_page(1, 0x44);
        let page = crate::decrypt::fixture::encrypt_page(&passphrase, &salt, 1, &plain);

        assert!(verify(&[0x01; KEY_SIZE], &[0x02; KEY_SIZE], &page).is_none());
    }

    #[test]
    fn db_priority_prefers_msg0_over_unknown_names() {
        assert!(db_priority("MSG0.DB") < db_priority("random_thing.db"));
        assert!(db_priority("sns.db") < db_priority("zz_other.db"));
    }

    #[test]
    fn infer_wxid_strips_account_dir_suffix_and_db_storage() {
        let p = PathBuf::from(r"D:\xwechat_files\wxid_abc123_a1b2\db_storage");
        assert_eq!(infer_wxid(&p), "wxid_abc123");
        let p2 = PathBuf::from(r"D:\xwechat_files\my_custom_id");
        assert_eq!(infer_wxid(&p2), "my_custom_id");
    }
}
