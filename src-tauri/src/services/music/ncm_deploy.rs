use std::fs::File;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, FILETIME, MAX_PATH};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

use super::hash::content_hash;

/// build.rs 嵌入 未构建为空
static PROXY_DLL: &[u8] = include_bytes!(env!("NCM_BRIDGE_DLL"));

const PROXY_NAME: &str = "msimg32.dll";
const BACKUP_NAME: &str = "msimg32_original.dll";
const STAGED_NAME: &str = "msimg32.dll.new";
/// 记代理 hash 区分外来
const MARKER_NAME: &str = "msimg32.dll.topisland";
const PE_MACHINE_AMD64: u16 = 0x8664;

const RESTART_MIN_GAP_MS: i64 = 60_000;
const RESTART_MAX_ATTEMPTS: u32 = 3;
/// 太年轻或在自更新
const YOUNG_PROC_MS: i64 = 60_000;
/// 目录新动 或在自更新
const DIR_FRESH_MS: i64 = 180_000;

static LAST_RESTART_MS: AtomicI64 = AtomicI64::new(0);
static RESTART_ATTEMPTS: AtomicU32 = AtomicU32::new(0);
/// 永久原因上闩 重开才清
static DEPLOY_BLOCKED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BridgeStatus {
    NotDetected,
    NeedsRestart,
    Installed,
    Connecting,
    Connected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeployClass {
    Absent,
    OursCurrent,
    OursStale,
    /// 外来副本不碰
    Foreign,
}

struct RunningNcm {
    exe: PathBuf,
    start_ms: i64,
}

fn available() -> bool {
    !PROXY_DLL.is_empty()
}

fn our_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| content_hash(PROXY_DLL))
}

fn reset_restart_budget() {
    RESTART_ATTEMPTS.store(0, Ordering::Relaxed);
    LAST_RESTART_MS.store(0, Ordering::Relaxed);
    DEPLOY_BLOCKED.store(false, Ordering::Relaxed);
}

static WANTED: AtomicBool = AtomicBool::new(false);
static KICK: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());
const HEAL_EVERY: Duration = Duration::from_secs(20);

/// 并发会双杀 部署走单线程
pub fn set_wanted(want: bool) {
    let prev = WANTED.swap(want, Ordering::Relaxed);
    if want && !prev {
        // 仅关转开才重置
        reset_restart_budget();
    }
    start_executor();
    if want != prev {
        kick();
    }
}

fn kick() {
    let (lock, cv) = &KICK;
    *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
    cv.notify_one();
}

fn start_executor() {
    static START: std::sync::Once = std::sync::Once::new();
    START.call_once(|| {
        let hb = crate::infra::watchdog::register("ncm-deploy", Duration::from_secs(30));
        let spawned = std::thread::Builder::new()
            .name("ncm-deploy".into())
            .spawn(move || {
                let mut was_wanted = false;
                loop {
                    {
                        let (lock, cv) = &KICK;
                        let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                        let (mut guard, _) = cv
                            .wait_timeout(guard, HEAL_EVERY)
                            .unwrap_or_else(|e| e.into_inner());
                        *guard = false;
                    }
                    hb.beat();
                    let want = WANTED.load(Ordering::Relaxed);
                    hb.busy(true);
                    if want {
                        let _ = ensure_deployed();
                    } else if was_wanted {
                        revert();
                    }
                    hb.busy(false);
                    was_wanted = want;
                }
            });
        if let Err(e) = spawned {
            eprintln!("[ncm-deploy] 执行线程启动失败: {e}");
        }
    });
}

fn ensure_deployed() -> bool {
    if !available() {
        eprintln!("[ncm-deploy] 本次构建未嵌入 bridge DLL，自动部署不可用");
        return false;
    }
    if DEPLOY_BLOCKED.load(Ordering::Relaxed) {
        return false;
    }
    let running = running_ncm();
    let Some(dir) = ncm_dir(running.as_ref()) else {
        return false;
    };

    match classify(&dir) {
        DeployClass::Foreign => {
            eprintln!("[ncm-deploy] 网易云目录已存在非本应用的 msimg32，不覆盖");
            DEPLOY_BLOCKED.store(true, Ordering::Relaxed);
            false
        }
        class => {
            // 32 位不部署
            if ncm_arch(&dir) != Some(PE_MACHINE_AMD64) {
                eprintln!("[ncm-deploy] 网易云不是 x64，不部署");
                DEPLOY_BLOCKED.store(true, Ordering::Relaxed);
                return false;
            }
            match running {
                Some(ncm) => ensure_while_running(&dir, class, &ncm),
                None => ensure_while_stopped(&dir, class),
            }
        }
    }
}

fn ensure_while_stopped(dir: &Path, class: DeployClass) -> bool {
    if class == DeployClass::OursCurrent {
        return true;
    }
    if !ensure_backup(dir) {
        DEPLOY_BLOCKED.store(true, Ordering::Relaxed);
        return false;
    }
    if write_proxy(dir) {
        true
    } else {
        DEPLOY_BLOCKED.store(true, Ordering::Relaxed);
        false
    }
}

fn ensure_while_running(dir: &Path, class: DeployClass, ncm: &RunningNcm) -> bool {
    let proxy = dir.join(PROXY_NAME);
    let proxy_mtime = file_mtime_ms(&proxy).unwrap_or(i64::MAX);
    if class == DeployClass::OursCurrent && ncm.start_ms >= proxy_mtime {
        return true;
    }
    if now_ms() - ncm.start_ms < YOUNG_PROC_MS || dir_recently_modified(dir) {
        return false;
    }
    // 杀前先验可写
    if !ensure_backup(dir) || !stage_proxy(dir) {
        eprintln!("[ncm-deploy] 网易云目录不可写，放弃自动部署");
        DEPLOY_BLOCKED.store(true, Ordering::Relaxed);
        return false;
    }
    if !allow_restart() {
        return false;
    }
    commit_restart(dir)
}

/// 失败须拉回并上闩
fn commit_restart(dir: &Path) -> bool {
    println!("[ncm-deploy] 部署增强，正在重启网易云…");
    let staged = dir.join(STAGED_NAME);
    let proxy = dir.join(PROXY_NAME);

    if !stop_ncm_and_wait() {
        eprintln!("[ncm-deploy] 网易云进程未能全部退出，放弃本次部署");
        relaunch_ncm(dir);
        DEPLOY_BLOCKED.store(true, Ordering::Relaxed);
        return false;
    }
    if !wait_unlocked(&proxy) {
        eprintln!("[ncm-deploy] 代理文件仍被占用，放弃本次部署");
        relaunch_ncm(dir);
        DEPLOY_BLOCKED.store(true, Ordering::Relaxed);
        return false;
    }

    let swapped = swap_in(&staged, &proxy);
    if swapped {
        let _ = std::fs::write(dir.join(MARKER_NAME), our_hash());
    } else {
        eprintln!("[ncm-deploy] 替换代理失败");
        DEPLOY_BLOCKED.store(true, Ordering::Relaxed);
    }
    relaunch_ncm(dir);
    swapped
}

fn swap_in(staged: &Path, proxy: &Path) -> bool {
    if std::fs::rename(staged, proxy).is_ok() {
        return true;
    }
    let ok = std::fs::copy(staged, proxy).is_ok();
    let _ = std::fs::remove_file(staged);
    ok
}

/// 句柄释放有窗口
fn wait_unlocked(path: &Path) -> bool {
    if !path.exists() {
        return true;
    }
    for _ in 0..50 {
        if std::fs::OpenOptions::new().write(true).open(path).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// 先删代理再删备份
fn revert() {
    let Some(dir) = ncm_dir(running_ncm().as_ref()) else {
        return;
    };
    let _ = std::fs::remove_file(dir.join(STAGED_NAME));
    let _ = std::fs::remove_file(dir.join(MARKER_NAME));
    let proxy = dir.join(PROXY_NAME);
    let proxy_gone = !proxy.exists() || std::fs::remove_file(&proxy).is_ok();
    if proxy_gone {
        let _ = std::fs::remove_file(dir.join(BACKUP_NAME));
    } else {
        eprintln!("[ncm-deploy] 代理被锁定，重启网易云后再清");
    }
}

pub fn status(connected: bool) -> BridgeStatus {
    let running = running_ncm();
    let dir = ncm_dir(running.as_ref());
    let (proxy_ours, proxy_mtime) = match &dir {
        Some(d) => (
            classify(d) == DeployClass::OursCurrent,
            file_mtime_ms(&d.join(PROXY_NAME)).unwrap_or(i64::MAX),
        ),
        None => (false, i64::MAX),
    };
    decide_status(
        dir.is_some(),
        connected,
        running.map(|n| n.start_ms),
        proxy_ours,
        proxy_mtime,
    )
}

fn decide_status(
    dir_found: bool,
    connected: bool,
    running_start_ms: Option<i64>,
    proxy_ours: bool,
    proxy_mtime_ms: i64,
) -> BridgeStatus {
    if connected {
        return BridgeStatus::Connected;
    }
    if !dir_found {
        return BridgeStatus::NotDetected;
    }
    match running_start_ms {
        Some(start) if proxy_ours && start >= proxy_mtime_ms => BridgeStatus::Connecting,
        Some(_) => BridgeStatus::NeedsRestart,
        None => BridgeStatus::Installed,
    }
}

fn allow_restart() -> bool {
    let ok = allow_restart_at(
        now_ms(),
        LAST_RESTART_MS.load(Ordering::Relaxed),
        RESTART_ATTEMPTS.load(Ordering::Relaxed),
    );
    if ok {
        LAST_RESTART_MS.store(now_ms(), Ordering::Relaxed);
        RESTART_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    }
    ok
}

fn allow_restart_at(now: i64, last: i64, attempts: u32) -> bool {
    attempts < RESTART_MAX_ATTEMPTS && now - last >= RESTART_MIN_GAP_MS
}

/// 备份真身作转发目标
fn ensure_backup(dir: &Path) -> bool {
    let backup = dir.join(BACKUP_NAME);
    if backup.exists() {
        return true;
    }
    let Some(system) = system_msimg32() else {
        eprintln!("[ncm-deploy] 找不到系统 msimg32.dll，放弃部署");
        return false;
    };
    std::fs::copy(&system, &backup).is_ok()
}

/// 兼作目录可写探测
fn stage_proxy(dir: &Path) -> bool {
    std::fs::write(dir.join(STAGED_NAME), PROXY_DLL).is_ok()
}

fn write_proxy(dir: &Path) -> bool {
    if std::fs::write(dir.join(PROXY_NAME), PROXY_DLL).is_err() {
        eprintln!("[ncm-deploy] 写入代理 DLL 失败");
        return false;
    }
    let _ = std::fs::write(dir.join(MARKER_NAME), our_hash());
    true
}

/// taskkill 拿不到失败原因
fn stop_ncm_and_wait() -> bool {
    use windows::Win32::Foundation::WAIT_OBJECT_0;
    use windows::Win32::System::Threading::{
        TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    };

    let pids = enum_ncm_pids();
    if pids.is_empty() {
        return true;
    }
    let mut handles = Vec::new();
    for pid in &pids {
        unsafe {
            match OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, *pid) {
                Ok(h) => {
                    if let Err(e) = TerminateProcess(h, 1) {
                        eprintln!("[ncm-deploy] TerminateProcess pid={pid} 失败: {e}");
                    }
                    handles.push(h);
                }
                Err(e) => eprintln!("[ncm-deploy] OpenProcess pid={pid} 失败: {e}"),
            }
        }
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut all_exited = true;
    for h in &handles {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let waited = unsafe { WaitForSingleObject(*h, remaining.as_millis() as u32) };
        if waited != WAIT_OBJECT_0 {
            all_exited = false;
        }
    }
    for h in handles {
        unsafe {
            let _ = CloseHandle(h);
        }
    }
    if !all_exited {
        let left = enum_ncm_pids();
        eprintln!("[ncm-deploy] 仍存活的 cloudmusic 进程: {left:?}");
        return false;
    }
    enum_ncm_pids().is_empty()
}

const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

/// 断 stdio 防管道堵
fn relaunch_ncm(dir: &Path) {
    let exe = dir.join("cloudmusic.exe");
    let spawned = Command::new(&exe)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn();
    if let Err(e) = spawned {
        eprintln!("[ncm-deploy] 重新拉起网易云失败: {e}");
    }
}

fn classify(dir: &Path) -> DeployClass {
    let proxy = dir.join(PROXY_NAME);
    if !proxy.exists() {
        return DeployClass::Absent;
    }
    match std::fs::read_to_string(dir.join(MARKER_NAME)) {
        Ok(marker) if marker.trim() == our_hash() => DeployClass::OursCurrent,
        Ok(_) => DeployClass::OursStale,
        Err(_) => DeployClass::Foreign,
    }
}

fn ncm_arch(dir: &Path) -> Option<u16> {
    let mut file = File::open(dir.join("cloudmusic.exe")).ok()?;
    let mut buf = vec![0u8; 4096];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);
    parse_pe_machine(&buf)
}

fn parse_pe_machine(buf: &[u8]) -> Option<u16> {
    if buf.len() < 0x40 || &buf[0..2] != b"MZ" {
        return None;
    }
    let e_lfanew = u32::from_le_bytes([buf[0x3C], buf[0x3D], buf[0x3E], buf[0x3F]]) as usize;
    if e_lfanew + 6 > buf.len() || &buf[e_lfanew..e_lfanew + 4] != b"PE\0\0" {
        return None;
    }
    Some(u16::from_le_bytes([buf[e_lfanew + 4], buf[e_lfanew + 5]]))
}

fn ncm_dir(running: Option<&RunningNcm>) -> Option<PathBuf> {
    if let Some(ncm) = running {
        if let Some(dir) = ncm.exe.parent() {
            if is_valid_ncm_dir(dir) {
                return Some(dir.to_path_buf());
            }
        }
    }
    if let Some(dir) = from_registry() {
        return Some(dir);
    }
    for dir in default_dirs() {
        if is_valid_ncm_dir(&dir) {
            return Some(dir);
        }
    }
    None
}

fn is_valid_ncm_dir(dir: &Path) -> bool {
    dir.join("cloudmusic.exe").exists() && dir.join("libcef.dll").exists()
}

fn default_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
        if let Ok(base) = std::env::var(var) {
            out.push(PathBuf::from(base).join("Netease").join("CloudMusic"));
        }
    }
    out
}

fn from_registry() -> Option<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let read = |hive, sub: &str, val: &str| -> Option<String> {
        RegKey::predef(hive)
            .open_subkey(sub)
            .ok()?
            .get_value::<String, _>(val)
            .ok()
    };

    // App Paths 默认值为全路径
    for (hive, sub) in [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\cloudmusic.exe",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths\cloudmusic.exe",
        ),
        (
            HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\cloudmusic.exe",
        ),
    ] {
        if let Some(exe) = read(hive, sub, "") {
            if let Some(dir) = Path::new(exe.trim().trim_matches('"')).parent() {
                if is_valid_ncm_dir(dir) {
                    return Some(dir.to_path_buf());
                }
            }
        }
    }

    for (hive, sub) in [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Netease\CloudMusic",
        ),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Netease\CloudMusic"),
        (HKEY_CURRENT_USER, r"SOFTWARE\Netease\CloudMusic"),
    ] {
        if let Some(dir) = read(hive, sub, "install_dir") {
            let dir = PathBuf::from(dir.trim().trim_matches('"'));
            if is_valid_ncm_dir(&dir) {
                return Some(dir);
            }
        }
    }

    const UNINSTALL: &[(&str, &str)] = &[
        (
            "HKLM",
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            "HKLM",
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            "HKCU",
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
    ];
    for (hive, path) in UNINSTALL {
        let root = match *hive {
            "HKLM" => RegKey::predef(HKEY_LOCAL_MACHINE),
            _ => RegKey::predef(HKEY_CURRENT_USER),
        };
        let Ok(uninstall) = root.open_subkey(path) else {
            continue;
        };
        for name in uninstall.enum_keys().flatten() {
            let Ok(entry) = uninstall.open_subkey(&name) else {
                continue;
            };
            let display: String = entry.get_value("DisplayName").unwrap_or_default();
            if !(display.contains("网易云音乐") || display.to_lowercase().contains("cloudmusic"))
            {
                continue;
            }
            let loc: String = entry.get_value("InstallLocation").unwrap_or_default();
            if !loc.is_empty() {
                let dir = PathBuf::from(loc);
                if is_valid_ncm_dir(&dir) {
                    return Some(dir);
                }
            }
        }
    }
    None
}

fn system_msimg32() -> Option<PathBuf> {
    let windir = std::env::var("WINDIR").ok()?;
    let p = PathBuf::from(windir).join("System32").join("msimg32.dll");
    p.exists().then_some(p)
}

fn file_mtime_ms(p: &Path) -> Option<i64> {
    let modified = std::fs::metadata(p).ok()?.modified().ok()?;
    let dur = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(dur.as_millis() as i64)
}

fn dir_recently_modified(dir: &Path) -> bool {
    file_mtime_ms(dir)
        .map(|m| now_ms() - m < DIR_FRESH_MS)
        .unwrap_or(false)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// CEF 子进程同名 取最老
fn running_ncm() -> Option<RunningNcm> {
    let mut best: Option<RunningNcm> = None;
    for pid in enum_ncm_pids() {
        if let Some(ncm) = query_process(pid) {
            if best
                .as_ref()
                .map(|b| ncm.start_ms < b.start_ms)
                .unwrap_or(true)
            {
                best = Some(ncm);
            }
        }
    }
    best
}

fn enum_ncm_pids() -> Vec<u32> {
    let mut pids = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return pids;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if wide_nul_to_string(&entry.szExeFile).eq_ignore_ascii_case("cloudmusic.exe") {
                    pids.push(entry.th32ProcessID);
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    pids
}

fn query_process(pid: u32) -> Option<RunningNcm> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; MAX_PATH as usize + 16];
        let mut len = buf.len() as u32;
        let exe = if QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok()
        {
            PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]))
        } else {
            let _ = CloseHandle(handle);
            return None;
        };

        let mut creation = FILETIME::default();
        let (mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let start_ms =
            if GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user).is_ok() {
                filetime_to_epoch_ms(creation)
            } else {
                0
            };
        let _ = CloseHandle(handle);
        Some(RunningNcm { exe, start_ms })
    }
}

fn filetime_to_epoch_ms(ft: FILETIME) -> i64 {
    ticks_to_epoch_ms(((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64)
}

/// 1601 起 100ns tick
fn ticks_to_epoch_ms(ticks: u64) -> i64 {
    (ticks / 10_000) as i64 - 11_644_473_600_000
}

fn wide_nul_to_string(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_status_covers_all_five_labels() {
        assert_eq!(
            decide_status(true, true, Some(5), true, 10),
            BridgeStatus::Connected
        );
        assert_eq!(
            decide_status(false, true, None, false, 0),
            BridgeStatus::Connected
        );
        assert_eq!(
            decide_status(false, false, None, false, 0),
            BridgeStatus::NotDetected
        );
        assert_eq!(
            decide_status(true, false, None, true, 0),
            BridgeStatus::Installed
        );
        assert_eq!(
            decide_status(true, false, Some(100), true, 50),
            BridgeStatus::Connecting
        );
        assert_eq!(
            decide_status(true, false, Some(40), true, 50),
            BridgeStatus::NeedsRestart
        );
        assert_eq!(
            decide_status(true, false, Some(100), false, 50),
            BridgeStatus::NeedsRestart
        );
    }

    #[test]
    fn allow_restart_respects_gap_and_budget() {
        assert!(allow_restart_at(1_000_000, 0, 0));
        assert!(!allow_restart_at(1_030_000, 1_000_000, 0));
        assert!(allow_restart_at(1_060_000, 1_000_000, 0));
        assert!(!allow_restart_at(10_000_000, 0, RESTART_MAX_ATTEMPTS));
        assert!(!allow_restart_at(10_000_000, 0, RESTART_MAX_ATTEMPTS + 1));
    }

    #[test]
    fn parse_pe_machine_reads_amd64_and_rejects_junk() {
        let mut buf = vec![0u8; 0x90];
        buf[0] = b'M';
        buf[1] = b'Z';
        buf[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        buf[0x80..0x84].copy_from_slice(b"PE\0\0");
        buf[0x84..0x86].copy_from_slice(&PE_MACHINE_AMD64.to_le_bytes());
        assert_eq!(parse_pe_machine(&buf), Some(PE_MACHINE_AMD64));

        buf[0x84..0x86].copy_from_slice(&0x014Cu16.to_le_bytes());
        assert_eq!(parse_pe_machine(&buf), Some(0x014C));

        assert_eq!(parse_pe_machine(b"not a pe"), None);
        assert_eq!(parse_pe_machine(&[]), None);
    }

    #[test]
    fn ticks_to_epoch_ms_maps_1601_to_unix() {
        const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
        assert_eq!(ticks_to_epoch_ms(UNIX_EPOCH_TICKS), 0);
        assert_eq!(ticks_to_epoch_ms(UNIX_EPOCH_TICKS + 10_000_000), 1000);
    }

    #[test]
    fn wide_nul_to_string_stops_at_nul() {
        let w: Vec<u16> = "cloudmusic.exe\0garbage".encode_utf16().collect();
        assert_eq!(wide_nul_to_string(&w), "cloudmusic.exe");
        let no_nul: Vec<u16> = "abc".encode_utf16().collect();
        assert_eq!(wide_nul_to_string(&no_nul), "abc");
    }
}
