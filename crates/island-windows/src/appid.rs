use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use windows::core::{Interface, GUID, HSTRING, PWSTR};
use windows::Win32::Foundation::{PROPERTYKEY, SIZE};
use windows::Win32::Graphics::Gdi::{DeleteObject, GetObjectW, DIBSECTION};
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, IBindCtx, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
};
use windows::Win32::UI::Shell::{
    IShellItem2, IShellItemImageFactory, SHCreateItemFromParsingName, SHLoadIndirectString,
    SIGDN_NORMALDISPLAY, SIIGBF_ICONONLY,
};

const ICON_PX: i32 = 64;
/// 过期后未命中才重建 命中不触发扫描
const INDEX_TTL: Duration = Duration::from_secs(60);

const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3),
    pid: 5,
};

struct Index {
    built: Instant,
    links: HashMap<String, PathBuf>,
}

static INDEX: Mutex<Option<Index>> = Mutex::new(None);

fn lock_index() -> MutexGuard<'static, Option<Index>> {
    INDEX.lock().unwrap_or_else(|e| e.into_inner())
}

/// HKCU 优先其次 HKLM
pub(crate) fn registry_value(aumid: &str, value: &str) -> Option<String> {
    let subkey = HSTRING::from(format!(r"Software\Classes\AppUserModelId\{aumid}"));
    let value = HSTRING::from(value);
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let mut size: u32 = 0;
        let ok = unsafe {
            RegGetValueW(
                root,
                &subkey,
                &value,
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut size),
            )
        };
        if ok.is_err() || size == 0 {
            continue;
        }
        let mut buf = vec![0u16; (size / 2) as usize];
        let ok = unsafe {
            RegGetValueW(
                root,
                &subkey,
                &value,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr() as *mut _),
                Some(&mut size),
            )
        };
        if ok.is_ok() {
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            return Some(String::from_utf16_lossy(&buf[..end]));
        }
    }
    None
}

/// 首扫连带 COM 冷启动慢 预热别落在首条通知
pub fn prewarm() {
    let _ = shell_item(std::ffi::OsStr::new(
        "shell:AppsFolder\\Microsoft.Windows.Explorer",
    ));
    let mut guard = lock_index();
    if guard.is_none() {
        *guard = Some(build_index());
    }
}

pub fn display_name(aumid: &str) -> Option<String> {
    if let Some(name) = registry_value(aumid, "DisplayName") {
        let name = if name.starts_with('@') {
            load_indirect(&name).unwrap_or(name)
        } else {
            name
        };
        if !name.trim().is_empty() {
            return Some(name);
        }
    }
    let item = resolve(aumid)?;
    let name = unsafe { item.GetDisplayName(SIGDN_NORMALDISPLAY) }.ok()?;
    take_pwstr(name).filter(|s| !s.is_empty())
}

/// 返回 PNG 或 IconUri 原图 调用方按魔数认格式
pub fn icon_bytes(aumid: &str) -> Option<Vec<u8>> {
    if let Some(uri) = registry_value(aumid, "IconUri") {
        let path = uri.strip_prefix("file:///").unwrap_or(&uri);
        if let Ok(bytes) = std::fs::read(path) {
            return Some(bytes);
        }
    }
    let item = resolve(aumid)?;
    let factory: IShellItemImageFactory = item.cast().ok()?;
    let hbm = unsafe {
        factory.GetImage(
            SIZE {
                cx: ICON_PX,
                cy: ICON_PX,
            },
            SIIGBF_ICONONLY,
        )
    }
    .ok()?;
    let png = dib_to_png(hbm);
    unsafe {
        let _ = DeleteObject(hbm.into());
    }
    png
}

fn resolve(aumid: &str) -> Option<IShellItem2> {
    if aumid.is_empty() {
        return None;
    }
    let path = Path::new(aumid);
    if path.is_absolute() && path.is_file() {
        return shell_item(path.as_os_str());
    }
    if let Some(link) = lookup_link(aumid) {
        return shell_item(link.as_os_str());
    }
    shell_item(std::ffi::OsStr::new(&format!("shell:AppsFolder\\{aumid}")))
}

fn lookup_link(aumid: &str) -> Option<PathBuf> {
    let mut guard = lock_index();
    if let Some(index) = guard.as_ref() {
        if let Some(link) = index.links.get(aumid) {
            return Some(link.clone());
        }
        if index.built.elapsed() < INDEX_TTL {
            return None;
        }
    }
    let index = build_index();
    let hit = index.links.get(aumid).cloned();
    *guard = Some(index);
    hit
}

fn build_index() -> Index {
    let mut links = HashMap::new();
    let roots = [
        std::env::var("APPDATA").ok(),
        std::env::var("ProgramData").ok(),
    ];
    for root in roots.into_iter().flatten() {
        let dir = Path::new(&root).join(r"Microsoft\Windows\Start Menu\Programs");
        walk(&dir, &mut |lnk| {
            if let Some(id) = shortcut_aumid(lnk) {
                // 同 AUMID 保留先扫到的 用户目录先于公共目录
                links.entry(id).or_insert_with(|| lnk.to_path_buf());
            }
        });
    }
    Index {
        built: Instant::now(),
        links,
    }
}

fn walk(dir: &Path, visit: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, visit);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
        {
            visit(&path);
        }
    }
}

fn shortcut_aumid(lnk: &Path) -> Option<String> {
    let item = shell_item(lnk.as_os_str())?;
    let value = unsafe { item.GetString(&PKEY_APP_USER_MODEL_ID) }.ok()?;
    take_pwstr(value).filter(|s| !s.is_empty())
}

fn shell_item(name: &std::ffi::OsStr) -> Option<IShellItem2> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        SHCreateItemFromParsingName::<_, Option<&IBindCtx>, IShellItem2>(&HSTRING::from(name), None)
            .ok()
    }
}

fn take_pwstr(value: PWSTR) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let text = unsafe { value.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(value.as_ptr() as *const _)) };
    text
}

fn load_indirect(text: &str) -> Option<String> {
    let mut buf = [0u16; 512];
    unsafe { SHLoadIndirectString(&HSTRING::from(text), &mut buf, None) }.ok()?;
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

/// GetImage 给 32bpp 预乘 BGRA 转直通 RGBA 编 PNG
fn dib_to_png(hbm: windows::Win32::Graphics::Gdi::HBITMAP) -> Option<Vec<u8>> {
    let mut ds = DIBSECTION::default();
    let got = unsafe {
        GetObjectW(
            hbm.into(),
            std::mem::size_of::<DIBSECTION>() as i32,
            Some(&mut ds as *mut _ as *mut _),
        )
    };
    if got == 0 || ds.dsBm.bmBitsPixel != 32 || ds.dsBm.bmBits.is_null() {
        return None;
    }
    let width = ds.dsBm.bmWidth as usize;
    let height = ds.dsBm.bmHeight as usize;
    let stride = ds.dsBm.bmWidthBytes as usize;
    let src = unsafe { std::slice::from_raw_parts(ds.dsBm.bmBits as *const u8, stride * height) };
    // biHeight 为正是自底向上
    let bottom_up = ds.dsBmih.biHeight > 0;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        let y = if bottom_up { height - 1 - row } else { row };
        let line = &src[y * stride..y * stride + width * 4];
        for px in line.chunks_exact(4) {
            let a = px[3] as u32;
            let un = |c: u8| {
                if a == 0 {
                    0
                } else {
                    ((c as u32 * 255 + a / 2) / a).min(255) as u8
                }
            };
            rgba.extend_from_slice(&[un(px[2]), un(px[1]), un(px[0]), px[3]]);
        }
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&rgba).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_menu_index_maps_shortcut_aumids() {
        let index = build_index();
        // 任何系统都带 Microsoft.Windows. 前缀项
        let hit = index
            .links
            .iter()
            .find(|(id, _)| id.starts_with("Microsoft.Windows."));
        assert!(hit.is_some());
    }

    #[test]
    fn exe_path_aumid_yields_png_icon() {
        let exe = std::env::var("WINDIR").unwrap() + r"\explorer.exe";
        let bytes = icon_bytes(&exe).expect("exe 路径形式的 AUMID 应能取到图标");
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(display_name(&exe).is_some());
    }

    #[test]
    fn unknown_aumid_resolves_to_nothing() {
        assert_eq!(display_name("top-island.test.no-such-app"), None);
        assert_eq!(icon_bytes("top-island.test.no-such-app"), None);
    }

    #[test]
    fn packaged_app_resolves_via_appsfolder() {
        let aumid =
            "windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel";
        let name = display_name(aumid).expect("UWP 应用名应能解析");
        assert!(!name.starts_with("windows."), "{name}");
        let icon = icon_bytes(aumid).expect("UWP 应用图标应能取到");
        assert!(icon.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}
