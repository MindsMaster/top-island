use windows::core::HSTRING;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
};

use crate::error::{AppError, AppResult};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "TopIsland";

/// 开机自启：写/删 HKCU Run。dev（target 目录下的 exe）跳过，否则会登记调试产物。
pub fn sync(enabled: bool) -> AppResult<()> {
    let exe = std::env::current_exe().map_err(|e| AppError::io_at("定位可执行文件", &e))?;
    if exe.to_string_lossy().contains("\\target\\") {
        return Ok(());
    }
    let subkey = HSTRING::from(RUN_KEY);
    let name = HSTRING::from(VALUE_NAME);
    unsafe {
        if enabled {
            let path = HSTRING::from(exe.as_os_str());
            let err = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &subkey,
                &name,
                REG_SZ.0,
                Some(path.as_wide().as_ptr() as *const _),
                (path.len() + 1) as u32 * 2,
            );
            if err.0 != 0 {
                return Err(AppError::new(format!("error.io: 写自启注册表: win32 {}", err.0)));
            }
        } else {
            // 不存在时返回错误属正常路径
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &subkey, &name);
        }
    }
    Ok(())
}
