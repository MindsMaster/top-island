use windows::core::HSTRING;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
};

use crate::error::{AppError, AppResult};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// 值名用 identifier 而不是产品名：Run 下所有软件共用一个命名空间，
/// 叫 TopIsland 的另一个软件会直接覆盖掉我们的自启项（反之亦然）。
const VALUE_NAME: &str = "cc.azuramc.topisland";
/// 历来用过的值名：0.0.2 及以前的 Tauri 版用产品名，Electron 版是
/// app.setLoginItemSettings 生成的 electron.app.<产品名>。每次 sync 都删一遍，
/// 否则升上来的用户会留一条指向老路径的重复自启。
const STALE_VALUE_NAMES: &[&str] = &["TopIsland", "electron.app.TopIsland"];

/// 开机自启：写/删 HKCU Run。dev（target 目录下的 exe）跳过，否则会登记调试产物。
pub fn sync(enabled: bool) -> AppResult<()> {
    let exe = std::env::current_exe().map_err(|e| AppError::io_at("定位可执行文件", &e))?;
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
                return Err(AppError::new(format!("error.io: 写自启注册表: win32 {}", err.0)));
            }
        } else {
            // 不存在时返回错误属正常路径
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &subkey, &name);
        }
    }
    Ok(())
}
