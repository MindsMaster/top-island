//! 系统通用对话框。

use crate::error::{Result, WinError};
use crate::wide::{from_wide_nul, to_wide_nul};

/// 通用「打开文件」对话框（comdlg32）。取消选择返回 None。
pub fn pick_open_file(
    title: &str,
    filter_label: &str,
    extensions: &[&str],
) -> Result<Option<String>> {
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
