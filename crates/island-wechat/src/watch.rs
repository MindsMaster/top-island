//! 消息目录变更监听：ReadDirectoryChangesW + 事件超时切片等待。
//! 用 overlapped + 500ms 等待切片而不是同步阻塞调用，是为了 stop 标志能在
//! 半秒内生效（同步阻塞版本只能靠 CancelSynchronousIo 跨线程取消，更脆）。

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadDirectoryChangesW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED,
    FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::{CancelIoEx, OVERLAPPED};
use windows::Win32::System::Threading::{CreateEventW, ResetEvent, WaitForSingleObject};

const BUFFER_SIZE: usize = 64 * 1024;
const WAIT_SLICE_MS: u32 = 500;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 阻塞等目录（不含子目录）发生变化。返回 true 表示有变化；stop 置位或出错返回 false。
pub fn wait_for_change(dir: &Path, stop: &AtomicBool) -> bool {
    let path_wide = wide(&dir.to_string_lossy());
    unsafe {
        let handle = match CreateFileW(
            PCWSTR(path_wide.as_ptr()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[wechat] watch: 打开目录失败 {}: {e}", dir.display());
                return false;
            }
        };
        let event = match CreateEventW(None, true, false, PCWSTR::null()) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[wechat] watch: 创建事件失败: {e}");
                let _ = CloseHandle(handle);
                return false;
            }
        };

        let filter: FILE_NOTIFY_CHANGE =
            FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_SIZE;
        let mut buf = vec![0u8; BUFFER_SIZE];
        let changed = loop {
            if stop.load(Ordering::Relaxed) {
                break false;
            }
            let _ = ResetEvent(event);
            let mut overlapped = OVERLAPPED { hEvent: event, ..Default::default() };
            let mut bytes_returned = 0u32;
            // overlapped 模式下 Ok 表示异步请求已挂起；错误（如目录被删）直接退出
            if let Err(e) = ReadDirectoryChangesW(
                handle,
                buf.as_mut_ptr() as *mut _,
                buf.len() as u32,
                false,
                filter,
                Some(&mut bytes_returned),
                Some(&mut overlapped),
                None,
            ) {
                eprintln!("[wechat] watch: ReadDirectoryChangesW 失败: {e}");
                break false;
            }
            if WaitForSingleObject(event, WAIT_SLICE_MS) == WAIT_OBJECT_0 {
                break true;
            }
            // 超时切片：回收这次请求，下一轮先看 stop
            let _ = CancelIoEx(handle, Some(&overlapped));
            let _ = WaitForSingleObject(event, WAIT_SLICE_MS);
        };

        let _ = CloseHandle(event);
        let _ = CloseHandle(handle);
        changed
    }
}
