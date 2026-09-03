use windows::core::HSTRING;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows::Win32::UI::Shell::{ApplicationActivationManager, IApplicationActivationManager, AO_NONE};

use crate::error::{Result, WinError};

/// 应用注册的 toast activator CLSID（HKCU 优先，其次 HKLM）
fn custom_activator(aumid: &str) -> Option<String> {
    let subkey = HSTRING::from(format!(r"Software\Classes\AppUserModelId\{aumid}"));
    let value = HSTRING::from("CustomActivator");
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let mut size: u32 = 0;
        let ok = unsafe {
            RegGetValueW(root, &subkey, &value, RRF_RT_REG_SZ, None, None, Some(&mut size))
        };
        if ok.is_err() || size == 0 {
            continue;
        }
        let mut buf = vec![0u16; (size / 2) as usize];
        let ok = unsafe {
            RegGetValueW(
                root,
                &subkey,
                &value,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr() as *mut _),
                Some(&mut size),
            )
        };
        if ok.is_ok() {
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            return Some(String::from_utf16_lossy(&buf[..end]));
        }
    }
    None
}

/// 复现一次 toast 点击：CustomActivator（深链）→ AAM（带参前台启动）。
/// 返回实际生效的方式；protocol 直开和 shell:AppsFolder 兜底由调用方处理。
pub fn activate(aumid: &str) -> Result<String> {
    if let Some(clsid) = custom_activator(aumid) {
        return Ok(format!("com-registered:{clsid}"));
    }
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let mgr: IApplicationActivationManager =
            CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_ALL)
                .map_err(|e| WinError::api("创建应用激活管理器", e))?;
        let pid = mgr
            .ActivateApplication(&HSTRING::from(aumid), &HSTRING::new(), AO_NONE)
            .map_err(|e| WinError::api("激活应用", e))?;
        Ok(format!("aam:pid={pid}"))
    }
}
