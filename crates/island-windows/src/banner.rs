use windows::core::HSTRING;
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD,
    RRF_RT_REG_DWORD,
};

use crate::error::{Result, WinError};

/// ShowBanner=0 关弹窗 通知中心照收
const SETTINGS_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings";
const VALUE_NAME: &str = "ShowBanner";

/// 键值不存在即系统默认 返回 None
pub fn get_banner(aumid: &str) -> Result<Option<i32>> {
    let subkey = HSTRING::from(format!(r"{SETTINGS_KEY}\{aumid}"));
    let value = HSTRING::from(VALUE_NAME);
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &subkey,
            &value,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    if err == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if err.is_err() {
        return Err(WinError::api(
            "读取横幅开关",
            format!("WIN32_ERROR({})", err.0),
        ));
    }
    Ok(Some(data as i32))
}

/// None 删值还原系统默认
pub fn set_banner(aumid: &str, value: Option<i32>) -> Result<()> {
    let subkey = HSTRING::from(format!(r"{SETTINGS_KEY}\{aumid}"));
    let name = HSTRING::from(VALUE_NAME);
    match value {
        Some(v) => {
            let data = v as u32;
            let err = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    &subkey,
                    &name,
                    REG_DWORD.0,
                    Some(&data as *const u32 as *const _),
                    std::mem::size_of::<u32>() as u32,
                )
            };
            if err.is_err() {
                return Err(WinError::api(
                    "写入横幅开关",
                    format!("WIN32_ERROR({})", err.0),
                ));
            }
            Ok(())
        }
        None => {
            let err = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &subkey, &name) };
            if err.is_err() && err != ERROR_FILE_NOT_FOUND {
                return Err(WinError::api(
                    "还原横幅开关",
                    format!("WIN32_ERROR({})", err.0),
                ));
            }
            Ok(())
        }
    }
}
