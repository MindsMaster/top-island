//! 前台进程名查询：前台是微信时岛不弹消息通知（用户正在看，弹了是噪音）。
//! Electron 版走 C# bridge 的 foreground 请求；这里直接 GetForegroundWindow。

use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// 前台窗口所属进程的可执行文件名（不含扩展名，小写）。取不到返回 None。
pub fn foreground_process_stem() -> Option<String> {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        if GetWindowThreadProcessId(hwnd, Some(&mut pid)) == 0 || pid == 0 {
            return None;
        }
        let hprocess = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let path = QueryFullProcessImageNameW(
            hprocess,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .ok()
        .map(|_| String::from_utf16_lossy(&buf[..len as usize]));
        let _ = CloseHandle(hprocess);
        let path = path?;
        let name = path.rsplit(['\\', '/']).next()?;
        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
        Some(stem.to_lowercase())
    }
}
