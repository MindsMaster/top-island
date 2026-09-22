use std::time::Duration;

use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

use crate::error::{Result, WinError};

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


/// 通用「打开文件」对话框（comdlg32）。按域拆分本域只分到这一个 windows 侧文件，
/// 函数本身与剪贴板无关；后续有别的域需要时应抽成独立的 shell 对话框模块。
pub fn pick_open_file(title: &str, filter_label: &str, extensions: &[&str]) -> Result<Option<String>> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::UI::Controls::Dialogs::{
        CommDlgExtendedError, GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR,
        OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };

    // OPENFILENAME 的过滤器是「显示名\0模式\0\0」的双 NUL 结尾格式
    let mut filter: Vec<u16> = filter_label.encode_utf16().collect();
    filter.push(0);
    let patterns = extensions
        .iter()
        .map(|ext| format!("*.{ext}"))
        .collect::<Vec<_>>()
        .join(";");
    filter.extend(patterns.encode_utf16());
    filter.push(0);
    filter.push(0);

    // 首字符置 0 表示不带初始文件名
    let mut file_buf = vec![0u16; 4096];
    let title_wide = to_wide_nul(title);
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file_buf.as_mut_ptr()),
        nMaxFile: file_buf.len() as u32,
        lpstrTitle: if title.is_empty() { PCWSTR::null() } else { PCWSTR(title_wide.as_ptr()) },
        // NOCHANGEDIR：对话框默认会把进程 CWD 改成所选目录，之后的相对路径读写全歪
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    if unsafe { GetOpenFileNameW(&mut ofn) }.as_bool() {
        return Ok(Some(from_wide_nul(&file_buf)));
    }
    // CommDlgExtendedError 返回 0 代表用户取消，其余才是对话框本身出错
    let code = unsafe { CommDlgExtendedError() };
    if code.0 == 0 {
        Ok(None)
    } else {
        Err(WinError::api("打开文件对话框", format!("错误码 {}", code.0)))
    }
}

fn to_wide_nul(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide_nul(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

#[cfg(test)]
mod tests {
    use super::{from_wide_nul, to_wide_nul};

    #[test]
    fn to_wide_nul_appends_exactly_one_nul_terminator() {
        let wide = to_wide_nul("ab");
        assert_eq!(wide, vec![b'a' as u16, b'b' as u16, 0], "宽串必须以且仅以 1 个 NUL 结尾，否则剪贴板读取会越界");
    }

    #[test]
    fn from_wide_nul_stops_at_the_first_nul_and_drops_trailing_garbage() {
        let wide = [b'h' as u16, b'i' as u16, 0, 0xFFFF, 0x1234];
        assert_eq!(from_wide_nul(&wide), "hi", "NUL 之后的残留内存不能混进读出的文本");
    }

    #[test]
    fn wide_round_trip_preserves_unicode_text() {
        let original = "闹钟 Alarm 01 — 剪切 ✅";
        assert_eq!(from_wide_nul(&to_wide_nul(original)), original, "UTF-16 往返不应丢任何 Unicode 字符");
    }
}
