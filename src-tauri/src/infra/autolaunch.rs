use windows::core::HSTRING;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
};

use crate::error::{AppError, AppResult};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Run 值名共用命名空间 撞名会互相覆盖
const VALUE_NAME: &str = "cc.azuramc.topisland";
/// 历代版本用过的值名 每次 sync 清一遍
const STALE_VALUE_NAMES: &[&str] = &["TopIsland", "electron.app.TopIsland"];

pub fn sync(enabled: bool) -> AppResult<()> {
    let exe = std::env::current_exe().map_err(|e| AppError::io_at("定位可执行文件", &e))?;
    // dev 产物不登记自启
    if exe.to_string_lossy().contains("\\target\\") {
        return Ok(());
    }
    let subkey = HSTRING::from(RUN_KEY);
    let name = HSTRING::from(VALUE_NAME);
    unsafe {
        for stale in STALE_VALUE_NAMES {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &subkey, &HSTRING::from(*stale));
        }
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
                return Err(AppError::new(format!(
                    "error.io: 写自启注册表: win32 {}",
                    err.0
                )));
            }
        } else {
            // 不存在时返回错误属正常路径
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &subkey, &name);
        }
    }
    Ok(())
}
