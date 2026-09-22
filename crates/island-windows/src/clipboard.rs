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

// 预定义剪贴板格式号（Win32 ABI 常量）。windows crate 把它们定义在 Win32::System::Ole
// 且是 u16 新类型，这里直接用 u32 本地常量，免得为几个常量再拉一个特性。
const CF_BITMAP: u32 = 2;
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const CF_DIBV5: u32 = 17;

/// 进程内串行化剪贴板访问：OpenClipboard 是全系统互斥资源，本进程两个线程同时
/// Open 必然有一个失败，重试只会放大撞锁。
static CLIPBOARD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct ClipboardGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

/// 剪贴板任一时刻全系统只能一个进程打开；别的应用（办公套件最常见）会短暂持有，
/// 撞锁时退避重试几次再认输。
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
    Err(WinError::api("打开剪贴板", last_err.expect("重试过必有错误")))
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
    let handle =
        unsafe { GetClipboardData(CF_UNICODETEXT) }.map_err(|e| WinError::api("读剪贴板文本", e))?;
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

    // 所有权移交发生在 SetClipboardData 成功那一刻；此前任何失败路径都要自己 GlobalFree
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
    // SetClipboardData 成功后内存所有权移交系统，绝不能再 GlobalFree；
    // 失败时所有权还在我们手里，必须自己释放。
    if let Err(e) = unsafe { SetClipboardData(CF_UNICODETEXT, HANDLE(hglobal.0)) } {
        let _ = unsafe { GlobalFree(hglobal) };
        return Err(WinError::api("写入剪贴板", e));
    }
    Ok(())
}

/// 只探测格式存在性，绝不解码位图——主进程同步解码会拖垮全局鼠标钩子（Electron 版原注释）
pub fn has_image() -> bool {
    let available = |fmt: u32| unsafe { IsClipboardFormatAvailable(fmt) }.is_ok();
    if available(CF_BITMAP) || available(CF_DIB) || available(CF_DIBV5) {
        return true;
    }
    // 浏览器/QQ 等复制图片常只放注册格式（PNG），系统格式反而没有，也探一下
    let png = unsafe { RegisterClipboardFormatW(w!("PNG")) };
    png != 0 && available(png)
}

pub fn read_file_paths() -> Result<Vec<String>> {
    if unsafe { IsClipboardFormatAvailable(CF_HDROP) }.is_err() {
        return Ok(Vec::new());
    }
    let _guard = open_clipboard()?;
    let handle = unsafe { GetClipboardData(CF_HDROP) }
        .map_err(|e| WinError::api("读剪贴板文件列表", e))?;
    let hdrop = HDROP(handle.0);
    let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
    let mut paths = Vec::with_capacity(count as usize);
    for i in 0..count {
        // 第一趟拿长度（不含 NUL），第二趟才真正拷字符串
        let len = unsafe { DragQueryFileW(hdrop, i, None) } as usize;
        let mut buf = vec![0u16; len + 1];
        let got = unsafe { DragQueryFileW(hdrop, i, Some(&mut buf)) } as usize;
        if got > 0 {
            paths.push(String::from_utf16_lossy(&buf[..got]));
        }
    }
    Ok(paths)
}

