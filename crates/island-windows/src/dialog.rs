use crate::error::{Result, WinError};
use crate::wide::{from_wide_nul, to_wide_nul};

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

    // lpstrFilter 双 NUL 结尾格式
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

    // 首字符 0 即无初始文件名
    let mut file_buf = vec![0u16; 4096];
    let title_wide = to_wide_nul(title);
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file_buf.as_mut_ptr()),
        nMaxFile: file_buf.len() as u32,
        lpstrTitle: if title.is_empty() {
            PCWSTR::null()
        } else {
            PCWSTR(title_wide.as_ptr())
        },
        // NOCHANGEDIR 否则对话框改进程 CWD
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    if unsafe { GetOpenFileNameW(&mut ofn) }.as_bool() {
        return Ok(Some(from_wide_nul(&file_buf)));
    }
    // 返回 0 为用户取消
    let code = unsafe { CommDlgExtendedError() };
    if code.0 == 0 {
        Ok(None)
    } else {
        Err(WinError::api(
            "打开文件对话框",
            format!("错误码 {}", code.0),
        ))
    }
}
