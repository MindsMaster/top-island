use std::collections::HashSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac_array;
use rayon::prelude::*;
use regex::bytes::Regex;
use sha2::Sha512;

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
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
};

const KEY_SIZE: usize = 32;
const SALT_SIZE: usize = 16;
const IV_SIZE: usize = 16;
const HMAC_SHA512_SIZE: usize = 64;
const AES_BLOCK_SIZE: usize = 16;
const PAGE_SIZE: usize = 4096;
const ROUND_COUNT: u32 = 256000;

/// 微信主进程可执行名。Weixin.exe 为 4.x，WeChat.exe 兼容旧命名
const WECHAT_EXE_NAMES: [&str; 2] = ["weixin.exe", "wechat.exe"];

struct MemRegion {
    base: usize,
    size: usize,
    protect: u32,
    mtype: u32,
    filename: Option<String>,
}

/// 找到所有微信主进程 pid（按可执行名匹配；主/辅进程都可能同名，全部返回后逐个尝试）。
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

/// 枚举进程内存区域（含映射文件名，用于定位 Weixin.dll）。
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

/// 从目标进程读 `size` 字节；失败（部分区域不可读）返回 None。
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

/// 扫 Weixin.dll 映射区里的 internal key（4 条 `48 BA <imm64>` 后接 `48 85 C0`，拼 4×8=32 字节）。
/// 与 dll_key_scan.py 的 PATTERN 等价。返回去重后的候选（可能为空）。
fn scan_dll_internal_keys(hprocess: HANDLE, regions: &[MemRegion]) -> Vec<[u8; KEY_SIZE]> {
    // (?s-u)：. 匹配任意字节（含 \n），关闭 unicode 让 \xNN 匹配单字节。
    let re = Regex::new(
        r"(?s-u)\x48\xBA(.{8}).{3,8}?\x48\xBA(.{8}).{3,8}?\x48\xBA(.{8}).{3,8}?\x48\xBA(.{8}).{3,8}?\x48\x85\xC0",
    )
    .expect("internal-key regex");

    let mut keys = vec![];
    let mut seen = HashSet::new();
    for r in regions
        .iter()
        .filter(|r| matches!(&r.filename, Some(f) if f.to_lowercase().contains("weixin.dll")))
    {
        let Some(buf) = read_mem(hprocess, r.base, r.size) else {
            continue;
        };
        for cap in re.captures_iter(&buf) {
            let mut key = [0u8; KEY_SIZE];
            for i in 0..4 {
                key[i * 8..i * 8 + 8].copy_from_slice(cap.get(i + 1).unwrap().as_bytes());
            }
            if seen.insert(key) {
                keys.push(key);
            }
        }
    }
    keys
}

/// 扫 key stub：匹配处前 8 字节是指向 32 字节 key 的指针，顺指针读出候选（被掩码）。
/// 规则同 wechat-dump-rs 的 GetKeyAddrStub。只扫可写的 MEM_PRIVATE 区。
fn scan_key_candidates(hprocess: HANDLE, regions: &[MemRegion]) -> Vec<[u8; KEY_SIZE]> {
    let re = Regex::new(r"(?s-u).{6}\x00{2}\x00{8}\x20\x00{7}\x2f\x00{7}").expect("key-stub regex");

    let mut ptr_set: HashSet<u64> = HashSet::new();
    for r in regions.iter().filter(|r| {
        (r.protect & (PAGE_READWRITE.0 | PAGE_WRITECOPY.0)) != 0 && r.mtype == MEM_PRIVATE.0
    }) {
        let Some(buf) = read_mem(hprocess, r.base, r.size) else {
            continue;
        };
        for m in re.find_iter(&buf) {
            let s = m.start();
            let ptr = u64::from_le_bytes(buf[s..s + 8].try_into().unwrap());
            ptr_set.insert(ptr);
        }
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

/// 熵/可打印字符初筛：快速滤掉明显不是随机密钥的普通文本（同 key_v4.py 的 is_potential_key）。
fn is_potential_key(key: &[u8; KEY_SIZE]) -> bool {
    let distinct = key.iter().collect::<HashSet<_>>().len();
    if distinct < 15 {
        return false;
    }
    let printable = key.iter().filter(|&&b| (32..=126).contains(&b)).count();
    printable <= 24
}

/// 用一段加密 db 首页校验 `raw XOR internal` 是否为正确 key；是则返回明文 key。
fn verify(raw: &[u8; KEY_SIZE], internal: &[u8; KEY_SIZE], page: &[u8]) -> Option<[u8; KEY_SIZE]> {
    let mut passphrase = [0u8; KEY_SIZE];
    for i in 0..KEY_SIZE {
        passphrase[i] = raw[i] ^ internal[i];
    }

    let salt = &page[..SALT_SIZE];
    let mac_salt: Vec<u8> = salt.iter().map(|x| x ^ 0x3a).collect();

    let new_key = pbkdf2_hmac_array::<Sha512, KEY_SIZE>(&passphrase, salt, ROUND_COUNT);
    let mac_key = pbkdf2_hmac_array::<Sha512, KEY_SIZE>(&new_key, &mac_salt, 2);

    // hash 校验保留区，向上对齐到 AES 块
    let mut reserve = IV_SIZE + HMAC_SHA512_SIZE;
    if reserve % AES_BLOCK_SIZE != 0 {
        reserve = (reserve / AES_BLOCK_SIZE + 1) * AES_BLOCK_SIZE;
    }

    type HmacSha512 = Hmac<Sha512>;
    let mut mac = HmacSha512::new_from_slice(&mac_key).ok()?;
    let hmac_start = PAGE_SIZE - reserve + IV_SIZE;
    mac.update(&page[SALT_SIZE..hmac_start]);
    mac.update(&1u32.to_le_bytes()); // page number = 1
    let digest = mac.finalize().into_bytes();

    if digest.as_slice() == &page[hmac_start..hmac_start + HMAC_SHA512_SIZE] {
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

/// 递归收集 dir 下 >=4096 字节的 .db（跳过 key_info.db）。
fn collect_db_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_db_files(&p, out);
        } else if p.extension().map(|x| x.eq_ignore_ascii_case("db")).unwrap_or(false) {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            if name == "key_info.db" {
                continue;
            }
            if std::fs::metadata(&p).map(|m| m.len() >= PAGE_SIZE as u64).unwrap_or(false) {
                out.push(p);
            }
        }
    }
}

/// 从 `--db=` 传入的目录/文件里，选一个用于校验的加密 db，返回其首页（4096 字节）。
fn read_probe_page(db_arg: &str) -> Result<Vec<u8>, String> {
    let p = PathBuf::from(db_arg);
    let chosen = if p.is_file() {
        if !p.extension().map(|x| x.eq_ignore_ascii_case("db")).unwrap_or(false) {
            return Err(format!("--db 指向的不是 .db 文件: {}", p.display()));
        }
        p
    } else if p.is_dir() {
        let mut dbs = vec![];
        collect_db_files(&p, &mut dbs);
        if dbs.is_empty() {
            return Err(format!("目录内未找到可用于校验的 .db: {}", p.display()));
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
        dbs.into_iter().next().unwrap()
    } else {
        return Err(format!("--db 路径不存在: {}", p.display()));
    };

    let buf = std::fs::read(&chosen).map_err(|e| format!("读取 {} 失败: {}", chosen.display(), e))?;
    if buf.len() < PAGE_SIZE {
        return Err(format!("db 文件过小: {}", chosen.display()));
    }
    Ok(buf[..PAGE_SIZE].to_vec())
}

fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// 从 db_storage 目录路径推断 wxid（账号目录名去掉 `_xxxx` 后缀）。仅供展示，top-island 不依赖。
fn infer_wxid(db_arg: &str) -> String {
    let mut p = PathBuf::from(db_arg);
    // 若传的是 db_storage 目录，其父目录名即账号目录
    if p.file_name().map(|n| n.eq_ignore_ascii_case("db_storage")).unwrap_or(false) {
        if let Some(parent) = p.parent() {
            p = parent.to_path_buf();
        }
    }
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    // 去掉尾部 _<4位hex>
    if let Some(idx) = name.rfind('_') {
        let suffix = &name[idx + 1..];
        if suffix.len() == 4 && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
            return name[..idx].to_string();
        }
    }
    name
}

/// 对单个 pid 尝试取 key；成功返回明文 key hex。
fn recover_from_pid(pid: u32, page: &[u8]) -> Option<String> {
    let hprocess = unsafe {
        OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, false, pid).ok()?
    };
    let result = (|| {
        let regions = get_mem_list(hprocess);

        // internal key 候选：DLL 扫描结果 + 全零（等价不异或，兼容不做掩码的旧版本）
        let mut internals = scan_dll_internal_keys(hprocess, &regions);
        internals.push([0u8; KEY_SIZE]);
        eprintln!(
            "[*] pid {}: DLL internal-key candidates={}, memory regions={}",
            pid,
            internals.len() - 1,
            regions.len()
        );

        let candidates = scan_key_candidates(hprocess, &regions);
        eprintln!("[*] pid {}: key candidates after filtering={}", pid, candidates.len());
        if candidates.is_empty() {
            return None;
        }

        // 摊平成 (候选 × internal) 全部配对，一并行池里跑满多核，命中即返回明文 key。
        // 校验瓶颈是 PBKDF2-SHA512(256000)，摊平比「候选并行×internal 串行」更满地吃核。
        let pairs: Vec<(&[u8; KEY_SIZE], &[u8; KEY_SIZE])> = candidates
            .iter()
            .flat_map(|raw| internals.iter().map(move |internal| (raw, internal)))
            .collect();
        pairs
            .par_iter()
            .find_map_any(|(raw, internal)| verify(raw, internal, page).map(hex::encode))
    })();
    unsafe {
        let _ = CloseHandle(hprocess);
    }
    result
}

fn emit_and_exit(json: String) -> ! {
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "{}", json);
    let _ = stdout.flush();
    std::process::exit(if json.contains("\"ok\":true") { 0 } else { 1 });
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut db_arg: Option<String> = None;
    let mut mask = false;
    for a in &args[1..] {
        if let Some(v) = a.strip_prefix("--db=") {
            db_arg = Some(v.to_string());
        } else if a == "--mask" {
            mask = true; // 安全校验模式：不打印真实 key
        }
    }

    let Some(db_arg) = db_arg else {
        emit_and_exit(r#"{"ok":false,"error":"missing --db=<path>"}"#.to_string());
    };

    let page = match read_probe_page(&db_arg) {
        Ok(p) => p,
        Err(e) => emit_and_exit(format!(r#"{{"ok":false,"error":"{}"}}"#, json_escape(&e))),
    };

    let pids = find_wechat_pids();
    if pids.is_empty() {
        emit_and_exit(r#"{"ok":false,"error":"WeChat/Weixin is not running"}"#.to_string());
    }
    eprintln!("[*] found wechat pids: {:?}", pids);

    let wxid = infer_wxid(&db_arg);
    for pid in pids {
        if let Some(key) = recover_from_pid(pid, &page) {
            let json = if mask {
                format!(
                    r#"{{"ok":true,"keyLen":{},"keyPrefix":"{}","wxid":"{}","dataDir":"{}"}}"#,
                    key.len(),
                    &key[..6.min(key.len())],
                    json_escape(&wxid),
                    json_escape(&db_arg)
                )
            } else {
                format!(
                    r#"{{"ok":true,"key":"{}","wxid":"{}","dataDir":"{}"}}"#,
                    key,
                    json_escape(&wxid),
                    json_escape(&db_arg)
                )
            };
            emit_and_exit(json);
        }
    }

    emit_and_exit(r#"{"ok":false,"error":"no valid key found in memory"}"#.to_string());
}
