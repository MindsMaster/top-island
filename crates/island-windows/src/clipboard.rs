use std::time::Duration;

use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

use crate::error::{Result, WinError};
use crate::wide::{from_wide_nul, to_wide_nul};

/// Win32 预定义格式号 crate 里在 Ole 特性故本地定义
const CF_BITMAP: u32 = 2;
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const CF_DIBV5: u32 = 17;

/// OpenClipboard 全系统互斥 进程内先串行化
static CLIPBOARD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct ClipboardGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

/// 他进程会短暂持有剪贴板 撞锁退避重试
fn open_clipboard() -> Result<ClipboardGuard> {
    let lock = CLIPBOARD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut last_err = None;
    for _ in 0..5 {
        match unsafe { OpenClipboard(HWND::default()) } {
            Ok(()) => return Ok(ClipboardGuard { _lock: lock }),
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    Err(WinError::api(
        "打开剪贴板",
        last_err.expect("重试过必有错误"),
    ))
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        if let Err(e) = unsafe { CloseClipboard() } {
            eprintln!("[clipboard] CloseClipboard 失败: {e}");
        }
    }
}

pub fn read_text() -> Result<Option<String>> {
    if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) }.is_err() {
        return Ok(None);
    }
    let _guard = open_clipboard()?;
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT) }
        .map_err(|e| WinError::api("读剪贴板文本", e))?;
    let hglobal = HGLOBAL(handle.0);
    let ptr = unsafe { GlobalLock(hglobal) };
    if ptr.is_null() {
        return Err(WinError::api("读剪贴板文本", "GlobalLock 返回空指针"));
    }
    let text = unsafe {
        let len = GlobalSize(hglobal) / 2;
        let slice = std::slice::from_raw_parts(ptr as *const u16, len);
        from_wide_nul(slice)
    };
    if let Err(e) = unsafe { GlobalUnlock(hglobal) } {
        eprintln!("[clipboard] GlobalUnlock 失败: {e}");
    }
    Ok(Some(text))
}

pub fn write_text(text: &str) -> Result<()> {
    let wide = to_wide_nul(text);
    let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) }
        .map_err(|e| WinError::api("分配剪贴板内存", e))?;
    let ptr = unsafe { GlobalLock(hglobal) };
    if ptr.is_null() {
        let _ = unsafe { GlobalFree(hglobal) };
        return Err(WinError::api("写入剪贴板", "GlobalLock 返回空指针"));
    }
    unsafe { std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len()) };
    if let Err(e) = unsafe { GlobalUnlock(hglobal) } {
        let _ = unsafe { GlobalFree(hglobal) };
        return Err(WinError::api("写入剪贴板", e));
    }

    // SetClipboardData 成功前失败路径须自己 GlobalFree
    let _guard = match open_clipboard() {
        Ok(guard) => guard,
        Err(e) => {
            let _ = unsafe { GlobalFree(hglobal) };
            return Err(e);
        }
    };
    if let Err(e) = unsafe { EmptyClipboard() } {
        let _ = unsafe { GlobalFree(hglobal) };
        return Err(WinError::api("清空剪贴板", e));
    }
    // 成功后所有权归系统 不能再 GlobalFree
    if let Err(e) = unsafe { SetClipboardData(CF_UNICODETEXT, HANDLE(hglobal.0)) } {
        let _ = unsafe { GlobalFree(hglobal) };
        return Err(WinError::api("写入剪贴板", e));
    }
    Ok(())
}

/// 只探测不解码 同步解码会卡鼠标钩子
pub fn has_image() -> bool {
    let available = |fmt: u32| unsafe { IsClipboardFormatAvailable(fmt) }.is_ok();
    if available(CF_BITMAP) || available(CF_DIB) || available(CF_DIBV5) {
        return true;
    }
    // 有应用只放 PNG 注册格式
    let png = unsafe { RegisterClipboardFormatW(w!("PNG")) };
    png != 0 && available(png)
}

pub fn read_file_paths() -> Result<Vec<String>> {
    if unsafe { IsClipboardFormatAvailable(CF_HDROP) }.is_err() {
        return Ok(Vec::new());
    }
    let _guard = open_clipboard()?;
    let handle =
        unsafe { GetClipboardData(CF_HDROP) }.map_err(|e| WinError::api("读剪贴板文件列表", e))?;
    let hdrop = HDROP(handle.0);
    let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
    let mut paths = Vec::with_capacity(count as usize);
    for i in 0..count {
        // 首趟取长度 次趟取内容
        let len = unsafe { DragQueryFileW(hdrop, i, None) } as usize;
        let mut buf = vec![0u16; len + 1];
        let got = unsafe { DragQueryFileW(hdrop, i, Some(&mut buf)) } as usize;
        if got > 0 {
            paths.push(String::from_utf16_lossy(&buf[..got]));
        }
    }
    Ok(paths)
}
