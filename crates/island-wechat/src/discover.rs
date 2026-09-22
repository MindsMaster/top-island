use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
use windows::Win32::UI::Shell::{FOLDERID_Documents, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

/// DRIVE_FIXED 只扫固定盘
const DRIVE_FIXED: u32 = 3;

#[derive(Debug, Clone)]
pub struct Account {
    /// 账号目录名去 _xxxx 后缀
    pub wxid: String,
    /// db_storage 目录
    pub data_dir: PathBuf,
}

/// 扫盘时跳过的系统目录
const SCAN_SKIP_NAMES: [&str; 12] = [
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "$recycle.bin",
    "system volume information",
    "recovery",
    "perflogs",
    "msocache",
    "onedrivetemp",
    "appdata",
    "node_modules",
];

pub fn fixed_drive_roots() -> Vec<PathBuf> {
    let mask = unsafe { GetLogicalDrives() };
    let mut roots = vec![];
    for i in 0..26u8 {
        if (mask >> i) & 1 == 0 {
            continue;
        }
        let letter = (b'A' + i) as char;
        let root = format!("{letter}:\\");
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        // 失败按 0 自然跳过
        let dtype = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
        if dtype == DRIVE_FIXED {
            roots.push(PathBuf::from(root));
        }
    }
    roots
}

/// 兼容重定向 失败回退 %USERPROFILE%\Documents
pub fn current_documents_dir() -> Option<PathBuf> {
    let from_shell = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_Documents,
            KF_FLAG_DEFAULT,
            windows::Win32::Foundation::HANDLE::default(),
        )
        .ok()
        .map(|p| {
            let s = p.to_string().unwrap_or_default();
            windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
            s
        })
    };
    from_shell
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Documents")))
}

fn safe_subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect()
}

/// 浅层候选 不做全盘深扫
pub fn xwechat_dir_candidates(docs: Option<&Path>, roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = vec![];
    let mut seen = std::collections::HashSet::new();
    let mut consider = |dir: PathBuf| {
        let key = dir.to_string_lossy().to_lowercase();
        if seen.insert(key) {
            out.push(dir);
        }
    };

    if let Some(docs) = docs {
        consider(docs.join("xwechat_files"));
    }
    for root in roots {
        consider(root.join("xwechat_files"));
        for child in safe_subdirs(root) {
            let name = child
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if SCAN_SKIP_NAMES.contains(&name.as_str()) {
                continue;
            }
            consider(child.join("xwechat_files"));
        }
    }
    out
}

/// 消息库最新 mtime ms 无则 0
fn account_newest_mtime(account_dir: &Path) -> u64 {
    let msg_dir = account_dir.join("db_storage").join("message");
    let Ok(entries) = std::fs::read_dir(&msg_dir) else {
        return 0;
    };
    let mut m = 0;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        if !is_message_db_name(&name) {
            continue;
        }
        if let Ok(meta) = e.metadata() {
            if let Ok(t) = meta.modified() {
                m = m.max(
                    t.duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0),
                );
            }
        }
    }
    m
}

pub fn is_message_db_name(name: &str) -> bool {
    let n = name.to_lowercase();
    let Some(stem) = n.strip_prefix("message_") else {
        return false;
    };
    let Some(digits) = stem.strip_suffix(".db") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// 去尾部 _4位hex 后缀 微信 4.x 约定
pub fn strip_wxid_suffix(name: &str) -> String {
    if let Some(idx) = name.rfind('_') {
        let suffix = &name[idx + 1..];
        if suffix.len() == 4 && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
            return name[..idx].to_string();
        }
    }
    name.to_string()
}

/// 目录名不必 wxid_ 开头 微信允许自定义号
pub fn discover_in(candidates: &[PathBuf]) -> Option<Account> {
    let mut best: Option<Account> = None;
    let mut best_time = 0u64;
    for xw in candidates {
        for account_dir in safe_subdirs(xw) {
            let name = account_dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name.eq_ignore_ascii_case("all_users") {
                continue; // 共享目录非账号
            }
            let mtime = account_newest_mtime(&account_dir);
            if mtime > best_time {
                best_time = mtime;
                best = Some(Account {
                    wxid: strip_wxid_suffix(&name),
                    data_dir: account_dir.join("db_storage"),
                });
            }
        }
    }
    best
}

pub fn discover_account() -> Option<Account> {
    let docs = current_documents_dir();
    let roots = fixed_drive_roots();
    let candidates = xwechat_dir_candidates(docs.as_deref(), &roots);
    discover_in(&candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "island-wechat-test-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("系统时间必须晚于 1970")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("测试临时目录必须能创建");
        dir
    }

    fn make_account(
        xwechat: &Path,
        dir_name: &str,
        msg_files: &[&str],
        mtime: SystemTime,
    ) -> PathBuf {
        let msg_dir = xwechat.join(dir_name).join("db_storage").join("message");
        std::fs::create_dir_all(&msg_dir).expect("账号消息目录必须能创建");
        for f in msg_files {
            let path = msg_dir.join(f);
            std::fs::write(&path, vec![0u8; 64]).expect("测试消息库必须能写入");
            let file = std::fs::File::options()
                .write(true)
                .open(&path)
                .expect("必须能打开测试库");
            file.set_modified(mtime).expect("必须能设置测试库 mtime");
        }
        xwechat.join(dir_name)
    }

    #[test]
    fn candidates_cover_docs_root_and_custom_subdirs_but_skip_system_dirs() {
        let base = unique_temp_dir("cand");
        let docs = base.join("Documents");
        let root = base.join("D");
        std::fs::create_dir_all(root.join("wechatMSG")).unwrap();
        std::fs::create_dir_all(root.join("Windows")).unwrap();
        std::fs::create_dir_all(root.join("$RECYCLE.BIN")).unwrap();

        let cands = xwechat_dir_candidates(Some(&docs), &[root.clone()]);
        let has = |p: PathBuf| cands.iter().any(|c| *c == p);
        assert!(has(docs.join("xwechat_files")));
        assert!(has(root.join("xwechat_files")));
        assert!(has(root.join("wechatMSG").join("xwechat_files")));
        assert!(!has(root.join("Windows").join("xwechat_files")));
        assert!(!has(root.join("$RECYCLE.BIN").join("xwechat_files")));

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn discover_in_picks_account_with_newest_message_db() {
        let base = unique_temp_dir("discover");
        let xw = base.join("xwechat_files");
        let old = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let new = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2_000_000);
        make_account(&xw, "wxid_old_aaaa", &["message_0.db"], old);
        make_account(&xw, "wxid_new_b1c2", &["message_0.db"], new);
        make_account(&xw, "not_an_account", &[], new); // 无消息库不算账号

        let acct = discover_in(&[xw]).expect("有效账号必须被发现");
        assert_eq!(acct.wxid, "wxid_new");
        assert!(acct.data_dir.ends_with("db_storage"));

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn discover_in_skips_all_users_shared_dir() {
        let base = unique_temp_dir("allusers");
        let xw = base.join("xwechat_files");
        make_account(&xw, "all_users", &["message_0.db"], SystemTime::now());

        assert!(discover_in(&[xw]).is_none());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn strip_wxid_suffix_only_strips_four_hex_chars() {
        assert_eq!(strip_wxid_suffix("wxid_abc_a1b2"), "wxid_abc");
        assert_eq!(strip_wxid_suffix("wxid_abc_12345"), "wxid_abc_12345");
        assert_eq!(strip_wxid_suffix("plain_name"), "plain_name");
        assert_eq!(strip_wxid_suffix("nounderscore"), "nounderscore");
    }

    #[test]
    fn is_message_db_name_matches_message_number_db() {
        assert!(is_message_db_name("message_0.db"));
        assert!(is_message_db_name("MESSAGE_12.DB"));
        assert!(!is_message_db_name("message_.db"));
        assert!(!is_message_db_name("message_x.db"));
        assert!(!is_message_db_name("biz_message_0.db"));
    }
}
