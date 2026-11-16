//! version.dll 代理 置于酷狗安装根目录 只在主进程挂钩

mod hooks;
mod lyric;
mod report;
mod slider;

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows::core::{PCSTR, PCWSTR};
use windows::Win32::Foundation::{HMODULE, MAX_PATH};
use windows::Win32::System::LibraryLoader::{
    DisableThreadLibraryCalls, GetProcAddress, LoadLibraryW,
};
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;

const DLL_PROCESS_ATTACH: u32 = 1;

/// 纯 jmp 不动寄存器与栈
mod forward {
    use super::AtomicUsize;

    macro_rules! forward {
        ($($name:ident),* $(,)?) => {
            #[allow(non_upper_case_globals)]
            mod slot {
                use super::AtomicUsize;
                $(pub static $name: AtomicUsize = AtomicUsize::new(0);)*
            }

            pub(super) static SLOTS: &[(&str, &AtomicUsize)] =
                &[$((concat!(stringify!($name), "\0"), &slot::$name)),*];

            $(
                #[no_mangle]
                #[unsafe(naked)]
                pub extern "system" fn $name() {
                    core::arch::naked_asm!("jmp qword ptr [rip + {slot}]", slot = sym slot::$name);
                }
            )*
        };
    }

    forward!(
        GetFileVersionInfoA,
        GetFileVersionInfoByHandle,
        GetFileVersionInfoExA,
        GetFileVersionInfoExW,
        GetFileVersionInfoSizeA,
        GetFileVersionInfoSizeExA,
        GetFileVersionInfoSizeExW,
        GetFileVersionInfoSizeW,
        GetFileVersionInfoW,
        VerFindFileA,
        VerFindFileW,
        VerInstallFileA,
        VerInstallFileW,
        VerLanguageNameA,
        VerLanguageNameW,
        VerQueryValueA,
        VerQueryValueW,
    );
}

#[no_mangle]
#[allow(non_snake_case)]
extern "system" fn DllMain(module: HMODULE, reason: u32, _reserved: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        unsafe {
            let _ = DisableThreadLibraryCalls(module);
        }
        // 须在 loader lock 内备好转发目标
        let _ = load_real_version();

        // CEF 子进程同名 不挂
        if !std::env::args_os().any(|a| a.to_string_lossy().starts_with("--type=")) {
            // 新线程待 loader lock 释放才跑
            std::thread::spawn(|| {
                if hooks::install() {
                    report::run();
                }
            });
        }
    }
    1
}

/// 绝对路径 相对名会解析回自身
fn load_real_version() -> bool {
    let mut buf = [0u16; MAX_PATH as usize];
    let len = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    if len == 0 || len >= buf.len() {
        return false;
    }
    let mut path: Vec<u16> = buf[..len].to_vec();
    path.extend("\\version.dll".encode_utf16());
    path.push(0);
    let Ok(module) = (unsafe { LoadLibraryW(PCWSTR(path.as_ptr())) }) else {
        return false;
    };
    let mut ok = true;
    for (name, slot) in forward::SLOTS {
        match unsafe { GetProcAddress(module, PCSTR(name.as_ptr())) } {
            Some(f) => slot.store(f as usize, Ordering::Release),
            None => ok = false,
        }
    }
    ok
}
