//! msimg32.dll 代理 置于网易云安装目录

mod cef;
mod hooks;

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows::core::{s, w, PCWSTR};
use windows::Win32::Foundation::{FARPROC, HMODULE, MAX_PATH};
use windows::Win32::System::LibraryLoader::{
    DisableThreadLibraryCalls, GetModuleFileNameW, GetProcAddress, LoadLibraryW,
};
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::Win32::System::Threading::GetCurrentProcessId;

const DLL_PROCESS_ATTACH: u32 = 1;

/// 纯 jmp 不动寄存器与栈
mod trampolines {
    use super::AtomicUsize;

    pub(super) static REAL_V_SET_DDRAW_FLAG: AtomicUsize = AtomicUsize::new(0);
    pub(super) static REAL_ALPHA_BLEND: AtomicUsize = AtomicUsize::new(0);
    pub(super) static REAL_DLL_INITIALIZE: AtomicUsize = AtomicUsize::new(0);
    pub(super) static REAL_GRADIENT_FILL: AtomicUsize = AtomicUsize::new(0);
    pub(super) static REAL_TRANSPARENT_BLT: AtomicUsize = AtomicUsize::new(0);

    macro_rules! trampoline {
        ($name:ident => $slot:ident) => {
            #[no_mangle]
            #[unsafe(naked)]
            pub extern "system" fn $name() {
                core::arch::naked_asm!(
                    "jmp qword ptr [rip + {slot}]",
                    slot = sym $slot,
                );
            }
        };
    }

    trampoline!(vSetDdrawflag => REAL_V_SET_DDRAW_FLAG);
    trampoline!(AlphaBlend => REAL_ALPHA_BLEND);
    trampoline!(DllInitialize => REAL_DLL_INITIALIZE);
    trampoline!(GradientFill => REAL_GRADIENT_FILL);
    trampoline!(TransparentBlt => REAL_TRANSPARENT_BLT);
}

#[no_mangle]
#[allow(non_snake_case)]
extern "system" fn DllMain(module: HMODULE, reason: u32, _reserved: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        unsafe {
            let _ = DisableThreadLibraryCalls(module);
        }

        // 须在 loader lock 内备好转发目标
        // 真 msimg32 仅依赖已加载模块 此时 LoadLibrary 安全
        let _ = load_real_msimg32();

        // 只有渲染进程创建 V8 上下文
        let outcome = if is_renderer_process() {
            Some(hooks::install())
        } else {
            None
        };

        if diagnostics_enabled() {
            // loader lock 下禁文件 IO 新线程待锁释放
            std::thread::spawn(move || record_host_process(outcome));
        }
    }
    1
}

fn diagnostics_enabled() -> bool {
    cfg!(debug_assertions) || std::env::var_os("TOPISLAND_BRIDGE_LOG").is_some()
}

fn is_renderer_process() -> bool {
    std::env::args_os().any(|a| a.to_string_lossy() == "--type=renderer")
}

fn load_real_msimg32() -> bool {
    let Some(module) = load_forward_target() else {
        return false;
    };

    let mut ok = true;
    let mut bind = |name: windows::core::PCSTR, slot: &AtomicUsize| {
        let proc: FARPROC = unsafe { GetProcAddress(module, name) };
        match proc {
            Some(f) => slot.store(f as usize, Ordering::Release),
            None => ok = false,
        }
    };

    bind(s!("vSetDdrawflag"), &trampolines::REAL_V_SET_DDRAW_FLAG);
    bind(s!("AlphaBlend"), &trampolines::REAL_ALPHA_BLEND);
    bind(s!("DllInitialize"), &trampolines::REAL_DLL_INITIALIZE);
    bind(s!("GradientFill"), &trampolines::REAL_GRADIENT_FILL);
    bind(s!("TransparentBlt"), &trampolines::REAL_TRANSPARENT_BLT);
    ok
}

/// 优先旁边的 msimg32_original.dll 回退 System32
/// 回退须绝对路径 相对名会解析回自身
fn load_forward_target() -> Option<HMODULE> {
    if let Ok(m) = unsafe { LoadLibraryW(w!("msimg32_original.dll")) } {
        if !m.is_invalid() {
            return Some(m);
        }
    }
    let mut buf = [0u16; MAX_PATH as usize];
    let len = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    if len == 0 || len >= buf.len() {
        return None;
    }
    let mut path: Vec<u16> = buf[..len].to_vec();
    path.extend("\\msimg32.dll".encode_utf16());
    path.push(0);
    match unsafe { LoadLibraryW(PCWSTR(path.as_ptr())) } {
        Ok(m) if !m.is_invalid() => Some(m),
        _ => None,
    }
}

fn record_host_process(hook_outcome: Option<hooks::InstallOutcome>) {
    let pid = unsafe { GetCurrentProcessId() };
    let image = current_process_image().unwrap_or_else(|| "<unknown>".to_string());
    let kind = if is_renderer_process() {
        "renderer"
    } else {
        "other"
    };
    let hook = match hook_outcome {
        Some(o) => format!("{o:?}"),
        None => "-".to_string(),
    };

    let Some(marker) = marker_path() else { return };
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&marker)
    {
        let _ = writeln!(file, "{pid}\t{kind}\t{hook}\t{image}");
    }
}

fn current_process_image() -> Option<String> {
    let mut buf = [0u16; MAX_PATH as usize];
    let len = unsafe { GetModuleFileNameW(None, &mut buf) };
    if len == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

fn marker_path() -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("TEMP")?;
    Some(std::path::PathBuf::from(dir).join("top-island-bridge-load.log"))
}
