//! 代理 DLL 旁载部署 每个播放器一份 Target

use std::fs::File;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU8, Ordering};
use std::sync::{Condvar, Mutex, Once, OnceLock};
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
use super::integration::{GrantResult, IntegrationStatus};

const PE_MACHINE_AMD64: u16 = 0x8664;

const RESTART_MIN_GAP_MS: i64 = 60_000;
const RESTART_MAX_ATTEMPTS: u32 = 3;
/// 太年轻或在自更新
const YOUNG_PROC_MS: i64 = 60_000;
/// 目录新动 或在自更新
const DIR_FRESH_MS: i64 = 180_000;
const HEAL_EVERY: Duration = Duration::from_secs(20);

#[derive(Debug)]
pub struct Target {
    /// 日志前缀兼看门狗名
    pub tag: &'static str,
    /// 进程名 也是目录内主程序
    pub exe: &'static str,
    pub proxy: &'static str,
    /// 代理优先转发到它 否则直转 System32
    pub backup: Option<&'static str>,
    pub staged: &'static str,
    /// 记代理 hash 区分外来
    pub marker: &'static str,
    /// build.rs 嵌入 未构建为空
    pub payload: &'static [u8],
    /// 参数为最老同名进程的路径
    pub locate: fn(Option<&Path>) -> Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Block {
    None = 0,
    NoPermission = 1,
    Conflict = 2,
    Unsupported = 3,
    Failed = 4,
}

impl Block {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::NoPermission,
            2 => Self::Conflict,
            3 => Self::Unsupported,
            4 => Self::Failed,
            _ => Self::None,
        }
    }

    fn latched(self) -> bool {
        !matches!(self, Self::None | Self::NoPermission)
    }

    fn status(self) -> Option<IntegrationStatus> {
        match self {
            Self::None => None,
            Self::NoPermission => Some(IntegrationStatus::NoPermission),
            Self::Conflict => Some(IntegrationStatus::Conflict),
            Self::Unsupported => Some(IntegrationStatus::Unsupported),
            Self::Failed => Some(IntegrationStatus::Failed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeployClass {
    Absent,
    OursCurrent,
    OursStale,
    /// 外来副本不碰
    Foreign,
}

struct Running {
    exe: PathBuf,
    start_ms: i64,
}

#[derive(Debug)]
pub struct Deployer {
    target: Target,
    /// 除不可写外均上闩 重开才清
    block: AtomicU8,
    wanted: AtomicBool,
    kick: Mutex<bool>,
    kick_cv: Condvar,
    last_restart_ms: AtomicI64,
    restart_attempts: AtomicU32,
    executor: Once,
    hash: OnceLock<String>,
}

impl Deployer {
    pub const fn new(target: Target) -> Self {
        Self {
            target,
            block: AtomicU8::new(Block::None as u8),
            wanted: AtomicBool::new(false),
            kick: Mutex::new(false),
            kick_cv: Condvar::new(),
            last_restart_ms: AtomicI64::new(0),
            restart_attempts: AtomicU32::new(0),
            executor: Once::new(),
            hash: OnceLock::new(),
        }
    }

    fn block(&self) -> Block {
        Block::from_u8(self.block.load(Ordering::Relaxed))
    }

    fn set_block(&self, b: Block) {
        self.block.store(b as u8, Ordering::Relaxed);
    }

    fn our_hash(&self) -> &str {
        self.hash.get_or_init(|| content_hash(self.target.payload))
    }

    /// 并发会双杀 部署走单线程
    pub fn set_wanted(&'static self, want: bool) {
        let prev = self.wanted.swap(want, Ordering::Relaxed);
        if want && !prev {
            // 仅关转开才重置
            self.restart_attempts.store(0, Ordering::Relaxed);
            self.last_restart_ms.store(0, Ordering::Relaxed);
            self.set_block(Block::None);
        }
        self.start_executor();
        if want != prev {
            self.kick();
        }
    }

    fn kick(&self) {
        *self.kick.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.kick_cv.notify_one();
    }

    fn start_executor(&'static self) {
        self.executor.call_once(|| {
            let tag = self.target.tag;
            let hb = crate::infra::watchdog::register(tag, Duration::from_secs(30));
            let spawned = std::thread::Builder::new().name(tag.into()).spawn(move || {
                let mut was_wanted = false;
                loop {
                    {
                        let guard = self.kick.lock().unwrap_or_else(|e| e.into_inner());
                        let (mut guard, _) = self
                            .kick_cv
                            .wait_timeout(guard, HEAL_EVERY)
                            .unwrap_or_else(|e| e.into_inner());
                        *guard = false;
                    }
                    hb.beat();
                    let want = self.wanted.load(Ordering::Relaxed);
                    hb.busy(true);
                    if want {
                        let _ = self.ensure_deployed();
                    } else if was_wanted {
                        self.revert();
                    }
                    hb.busy(false);
                    was_wanted = want;
                }
            });
            if let Err(e) = spawned {
                eprintln!("[{tag}] 执行线程启动失败: {e}");
            }
        });
    }

    fn dir(&self, running: Option<&Running>) -> Option<PathBuf> {
        (self.target.locate)(running.map(|r| r.exe.as_path()))
    }

    fn ensure_deployed(&self) -> bool {
        let tag = self.target.tag;
        if self.target.payload.is_empty() {
            eprintln!("[{tag}] 本次构建未嵌入 bridge DLL，自动部署不可用");
            return false;
        }
        if self.block().latched() {
            return false;
        }
        let running = self.running();
        let Some(dir) = self.dir(running.as_ref()) else {
            return false;
        };

        match self.classify(&dir) {
            DeployClass::Foreign => {
                eprintln!("[{tag}] 目录已存在非本应用的 {}，不覆盖", self.target.proxy);
                self.set_block(Block::Conflict);
                false
            }
            class => {
                // 32 位不部署
                if exe_arch(&dir.join(self.target.exe)) != Some(PE_MACHINE_AMD64) {
                    eprintln!("[{tag}] 播放器不是 x64，不部署");
                    self.set_block(Block::Unsupported);
                    return false;
                }
                let ok = match running {
                    Some(r) => self.ensure_while_running(&dir, class, &r),
                    None => self.ensure_while_stopped(&dir, class),
                };
                if ok {
                    self.set_block(Block::None);
                }
                ok
            }
        }
    }

    fn ensure_while_stopped(&self, dir: &Path, class: DeployClass) -> bool {
        if class == DeployClass::OursCurrent {
            return true;
        }
        self.ensure_backup(dir) && self.write_proxy(dir)
    }

    fn ensure_while_running(&self, dir: &Path, class: DeployClass, running: &Running) -> bool {
        let proxy = dir.join(self.target.proxy);
        let proxy_mtime = file_mtime_ms(&proxy).unwrap_or(i64::MAX);
        if class == DeployClass::OursCurrent && running.start_ms >= proxy_mtime {
            return true;
        }
        if now_ms() - running.start_ms < YOUNG_PROC_MS || dir_recently_modified(dir) {
            return false;
        }
        // 杀前先验可写
        if !self.ensure_backup(dir) || !self.stage_proxy(dir) {
            return false;
        }
        if !self.allow_restart() {
            return false;
        }
        self.commit_restart(dir)
    }

    /// 失败须拉回并上闩
    fn commit_restart(&self, dir: &Path) -> bool {
        let tag = self.target.tag;
        println!("[{tag}] 部署增强，正在重启播放器…");
        let staged = dir.join(self.target.staged);
        let proxy = dir.join(self.target.proxy);

        if !self.stop_and_wait() {
            eprintln!("[{tag}] 播放器进程未能全部退出，放弃本次部署");
            self.relaunch(dir);
            self.set_block(Block::Failed);
            return false;
        }
        if !wait_unlocked(&proxy) {
            eprintln!("[{tag}] 代理文件仍被占用，放弃本次部署");
            self.relaunch(dir);
            self.set_block(Block::Failed);
            return false;
        }

        let swapped = swap_in(&staged, &proxy);
        if swapped {
            let _ = std::fs::write(dir.join(self.target.marker), self.our_hash());
        } else {
            eprintln!("[{tag}] 替换代理失败");
            self.set_block(Block::Failed);
        }
        self.relaunch(dir);
        swapped
    }

    /// 先删代理再删备份
    fn revert(&self) {
        let Some(dir) = self.dir(self.running().as_ref()) else {
            return;
        };
        let _ = std::fs::remove_file(dir.join(self.target.staged));
        let _ = std::fs::remove_file(dir.join(self.target.marker));
        let proxy = dir.join(self.target.proxy);
        let proxy_gone = !proxy.exists() || std::fs::remove_file(&proxy).is_ok();
        if !proxy_gone {
            eprintln!("[{}] 代理被锁定，重启播放器后再清", self.target.tag);
        } else if let Some(backup) = self.target.backup {
            let _ = std::fs::remove_file(dir.join(backup));
        }
    }

    pub fn grant_access(&self) -> GrantResult {
        let tag = self.target.tag;
        let Some(dir) = self.dir(self.running().as_ref()) else {
            return GrantResult::Failed;
        };
        let Some(sid) = current_user_sid() else {
            eprintln!("[{tag}] 取当前用户 SID 失败");
            return GrantResult::Failed;
        };
        let Some(icacls) = system32("icacls.exe") else {
            return GrantResult::Failed;
        };
        // 尾部反斜杠会转义引号
        let dir = dir.to_string_lossy();
        let params = format!("\"{}\" /grant *{sid}:(OI)(CI)M", dir.trim_end_matches('\\'));
        let result = run_elevated(tag, &icacls, &params);
        if result == GrantResult::Granted {
            self.kick();
        }
        result
    }

    pub fn status(&self, connected: bool) -> IntegrationStatus {
        let running = self.running();
        let dir = self.dir(running.as_ref());
        let (proxy_ours, proxy_mtime) = match &dir {
            Some(d) => (
                self.classify(d) == DeployClass::OursCurrent,
                file_mtime_ms(&d.join(self.target.proxy)).unwrap_or(i64::MAX),
            ),
            None => (false, i64::MAX),
        };
        decide_status(
            dir.is_some(),
            connected,
            running.map(|r| r.start_ms),
            proxy_ours,
            proxy_mtime,
            self.block().status(),
        )
    }

    fn allow_restart(&self) -> bool {
        let ok = allow_restart_at(
            now_ms(),
            self.last_restart_ms.load(Ordering::Relaxed),
            self.restart_attempts.load(Ordering::Relaxed),
        );
        if ok {
            self.last_restart_ms.store(now_ms(), Ordering::Relaxed);
            self.restart_attempts.fetch_add(1, Ordering::Relaxed);
        }
        ok
    }

    /// 备份真身作转发目标
    fn ensure_backup(&self, dir: &Path) -> bool {
        let Some(name) = self.target.backup else {
            return true;
        };
        let backup = dir.join(name);
        if backup.exists() {
            return true;
        }
        let Some(system) = system32(self.target.proxy) else {
            eprintln!(
                "[{}] 找不到系统 {}，放弃部署",
                self.target.tag, self.target.proxy
            );
            self.set_block(Block::Unsupported);
            return false;
        };
        self.write_ok(std::fs::copy(&system, &backup).map(|_| ()))
    }

    /// 兼作目录可写探测
    fn stage_proxy(&self, dir: &Path) -> bool {
        self.write_ok(std::fs::write(
            dir.join(self.target.staged),
            self.target.payload,
        ))
    }

    /// 不可写只记不闩 日志仅首次
    fn write_ok(&self, res: std::io::Result<()>) -> bool {
        let Err(e) = res else {
            return true;
        };
        if self.block() != Block::NoPermission {
            eprintln!("[{}] 目录不可写，暂停自动部署: {e}", self.target.tag);
        }
        self.set_block(Block::NoPermission);
        false
    }

    fn write_proxy(&self, dir: &Path) -> bool {
        if !self.write_ok(std::fs::write(
            dir.join(self.target.proxy),
            self.target.payload,
        )) {
            return false;
        }
        let _ = std::fs::write(dir.join(self.target.marker), self.our_hash());
        true
    }

    /// taskkill 拿不到失败原因
    fn stop_and_wait(&self) -> bool {
        use windows::Win32::Foundation::WAIT_OBJECT_0;
        use windows::Win32::System::Threading::{
            TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
        };

        let tag = self.target.tag;
        // 播放器收尾常超 5 秒
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            // 快照含已退出的僵尸进程
            let alive: Vec<_> = enum_pids(self.target.exe)
                .into_iter()
                .filter_map(|pid| unsafe {
                    let h =
                        OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid).ok()?;
                    if WaitForSingleObject(h, 0) == WAIT_OBJECT_0 {
                        let _ = CloseHandle(h);
                        return None;
                    }
                    Some((pid, h))
                })
                .collect();
            if alive.is_empty() {
                return true;
            }
            let expired = std::time::Instant::now() >= deadline;
            for (pid, h) in &alive {
                unsafe {
                    if !expired {
                        if let Err(e) = TerminateProcess(*h, 1) {
                            eprintln!("[{tag}] TerminateProcess pid={pid} 失败: {e}");
                        }
                        let remaining =
                            deadline.saturating_duration_since(std::time::Instant::now());
                        let _ = WaitForSingleObject(*h, remaining.as_millis() as u32);
                    }
                    let _ = CloseHandle(*h);
                }
            }
            if expired {
                let left: Vec<u32> = alive.iter().map(|(pid, _)| *pid).collect();
                eprintln!("[{tag}] 仍存活的 {} 进程: {left:?}", self.target.exe);
                return false;
            }
        }
    }

    /// 断 stdio 防管道堵
    fn relaunch(&self, dir: &Path) {
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

        let spawned = Command::new(dir.join(self.target.exe))
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn();
        if let Err(e) = spawned {
            eprintln!("[{}] 重新拉起播放器失败: {e}", self.target.tag);
        }
    }

    fn classify(&self, dir: &Path) -> DeployClass {
        if !dir.join(self.target.proxy).exists() {
            return DeployClass::Absent;
        }
        match std::fs::read_to_string(dir.join(self.target.marker)) {
            Ok(marker) if marker.trim() == self.our_hash() => DeployClass::OursCurrent,
            Ok(_) => DeployClass::OursStale,
            Err(_) => DeployClass::Foreign,
        }
    }

    /// 子进程同名 取最老
    fn running(&self) -> Option<Running> {
        enum_pids(self.target.exe)
            .into_iter()
            .filter_map(query_process)
            .min_by_key(|r| r.start_ms)
    }
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

fn run_elevated(tag: &str, exe: &Path, params: &str) -> GrantResult {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{ERROR_CANCELLED, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let file = HSTRING::from(exe.as_os_str());
    let params = HSTRING::from(params);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
        if e.code() == ERROR_CANCELLED.to_hresult() {
            return GrantResult::Cancelled;
        }
        eprintln!("[{tag}] 提权启动 icacls 失败: {e}");
        return GrantResult::Failed;
    }
    if info.hProcess.is_invalid() {
        return GrantResult::Failed;
    }
    let mut code = 1u32;
    unsafe {
        let exited = WaitForSingleObject(info.hProcess, 60_000) == WAIT_OBJECT_0;
        if !exited || GetExitCodeProcess(info.hProcess, &mut code).is_err() {
            code = 1;
        }
        let _ = CloseHandle(info.hProcess);
    }
    if code == 0 {
        GrantResult::Granted
    } else {
        eprintln!("[{tag}] icacls 授权失败 exit={code}");
        GrantResult::Failed
    }
}

fn current_user_sid() -> Option<String> {
    use windows::Win32::Foundation::{LocalFree, HANDLE, HLOCAL};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
        // u64 保证 TOKEN_USER 对齐
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        let queried = GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr().cast()),
            len,
            &mut len,
        );
        let _ = CloseHandle(token);
        queried.ok()?;
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut sid = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut sid).ok()?;
        let out = sid.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(sid.0.cast())));
        out
    }
}

fn decide_status(
    dir_found: bool,
    connected: bool,
    running_start_ms: Option<i64>,
    proxy_ours: bool,
    proxy_mtime_ms: i64,
    blocked: Option<IntegrationStatus>,
) -> IntegrationStatus {
    if connected {
        return IntegrationStatus::Connected;
    }
    if !dir_found {
        return IntegrationStatus::NotDetected;
    }
    if let Some(start) = running_start_ms {
        if proxy_ours && start >= proxy_mtime_ms {
            return IntegrationStatus::Connecting;
        }
    }
    // 代理已就位 重启即可
    match blocked {
        Some(IntegrationStatus::NoPermission) if proxy_ours => {}
        Some(b) => return b,
        None => {}
    }
    match running_start_ms {
        Some(_) => IntegrationStatus::NeedsRestart,
        None => IntegrationStatus::Installed,
    }
}

fn allow_restart_at(now: i64, last: i64, attempts: u32) -> bool {
    attempts < RESTART_MAX_ATTEMPTS && now - last >= RESTART_MIN_GAP_MS
}

fn exe_arch(exe: &Path) -> Option<u16> {
    let mut file = File::open(exe).ok()?;
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

/// App Paths 默认值为全路径
pub fn app_path_dirs(exe: &str) -> Vec<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let mut out = Vec::new();
    for (hive, base) in [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths",
        ),
        (
            HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths",
        ),
    ] {
        let value: Option<String> = RegKey::predef(hive)
            .open_subkey(format!(r"{base}\{exe}"))
            .ok()
            .and_then(|k| k.get_value("").ok());
        if let Some(full) = value {
            if let Some(dir) = Path::new(full.trim().trim_matches('"')).parent() {
                out.push(dir.to_path_buf());
            }
        }
    }
    out
}

/// 按 DisplayName 匹配卸载项 取 InstallLocation
pub fn uninstall_dirs(matches: fn(&str) -> bool) -> Vec<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let mut out = Vec::new();
    for (hive, path) in [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
    ] {
        let Ok(uninstall) = RegKey::predef(hive).open_subkey(path) else {
            continue;
        };
        for name in uninstall.enum_keys().flatten() {
            let Ok(entry) = uninstall.open_subkey(&name) else {
                continue;
            };
            let display: String = entry.get_value("DisplayName").unwrap_or_default();
            if !matches(&display) {
                continue;
            }
            let loc: String = entry.get_value("InstallLocation").unwrap_or_default();
            if !loc.is_empty() {
                out.push(PathBuf::from(loc.trim().trim_matches('"')));
            }
        }
    }
    out
}

pub fn program_dirs(rel: &[&str]) -> Vec<PathBuf> {
    ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .map(|base| rel.iter().fold(PathBuf::from(base), |p, seg| p.join(seg)))
        .collect()
}

fn system32(name: &str) -> Option<PathBuf> {
    let windir = std::env::var("WINDIR").ok()?;
    let p = PathBuf::from(windir).join("System32").join(name);
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

fn enum_pids(exe: &str) -> Vec<u32> {
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
                if wide_nul_to_string(&entry.szExeFile).eq_ignore_ascii_case(exe) {
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

fn query_process(pid: u32) -> Option<Running> {
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
        Some(Running { exe, start_ms })
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
            decide_status(true, true, Some(5), true, 10, None),
            IntegrationStatus::Connected
        );
        assert_eq!(
            decide_status(false, true, None, false, 0, None),
            IntegrationStatus::Connected
        );
        assert_eq!(
            decide_status(false, false, None, false, 0, None),
            IntegrationStatus::NotDetected
        );
        assert_eq!(
            decide_status(true, false, None, true, 0, None),
            IntegrationStatus::Installed
        );
        assert_eq!(
            decide_status(true, false, Some(100), true, 50, None),
            IntegrationStatus::Connecting
        );
        assert_eq!(
            decide_status(true, false, Some(40), true, 50, None),
            IntegrationStatus::NeedsRestart
        );
        assert_eq!(
            decide_status(true, false, Some(100), false, 50, None),
            IntegrationStatus::NeedsRestart
        );
    }

    #[test]
    fn decide_status_surfaces_block_reason() {
        let np = Some(IntegrationStatus::NoPermission);
        assert_eq!(
            decide_status(true, false, Some(100), false, i64::MAX, np),
            IntegrationStatus::NoPermission
        );
        assert_eq!(
            decide_status(true, false, None, false, i64::MAX, np),
            IntegrationStatus::NoPermission
        );
        assert_eq!(
            decide_status(true, false, Some(40), true, 50, np),
            IntegrationStatus::NeedsRestart
        );
        assert_eq!(
            decide_status(
                true,
                false,
                Some(100),
                false,
                50,
                Some(IntegrationStatus::Conflict)
            ),
            IntegrationStatus::Conflict
        );
        assert_eq!(
            decide_status(
                true,
                false,
                Some(100),
                true,
                50,
                Some(IntegrationStatus::Failed)
            ),
            IntegrationStatus::Connecting
        );
        assert_eq!(
            decide_status(true, true, Some(100), false, 50, np),
            IntegrationStatus::Connected
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
    fn current_user_sid_is_string_form() {
        let sid = current_user_sid().expect("sid");
        assert!(sid.starts_with("S-1-"), "{sid}");
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
