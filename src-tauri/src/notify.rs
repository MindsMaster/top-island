use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use windows::core::HSTRING;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows::Win32::UI::Shell::{ApplicationActivationManager, IApplicationActivationManager, AO_NONE};

#[derive(Serialize)]
pub struct ToastRow {
    pub id: i64,
    pub arrival_ms: i64,
    pub aumid: String,
    pub display_name: String,
    pub snippet: String,
}

// wpndatabase 的 Payload 是 BLOB：UTF-16LE（带 BOM 或次字节为 NUL）或 UTF-8，见 winbridge WpnDatabase.cs
fn decode_payload(blob: &[u8]) -> String {
    if blob.len() >= 2 && blob[0] == 0xFF && blob[1] == 0xFE {
        return utf16(&blob[2..]);
    }
    if blob.len() >= 2 && blob[1] == 0x00 {
        return utf16(blob);
    }
    let skip = if blob.len() >= 3 && blob[..3] == [0xEF, 0xBB, 0xBF] { 3 } else { 0 };
    String::from_utf8_lossy(&blob[skip..]).into_owned()
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

// toast XML 里只关心 <text> 正文
fn text_snippet(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(gt) = rest.find('>') else { break };
        rest = &rest[gt + 1..];
        let Some(end) = rest.find("</text>") else { break };
        if !out.is_empty() {
            out.push_str(" / ");
        }
        out.push_str(rest[..end].trim());
        rest = &rest[end + 7..];
    }
    out.chars().take(120).collect()
}

fn copy_db() -> Result<std::path::PathBuf, String> {
    let local = std::env::var("LOCALAPPDATA").map_err(|e| e.to_string())?;
    let src = std::path::Path::new(&local).join(r"Microsoft\Windows\Notifications\wpndatabase.db");
    let dir = std::env::temp_dir().join("island-spike-wpn");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for suffix in ["", "-wal", "-shm"] {
        let from = src.with_file_name(format!("wpndatabase.db{suffix}"));
        if from.exists() {
            std::fs::copy(&from, dir.join(format!("wpndatabase.db{suffix}"))).map_err(|e| e.to_string())?;
        }
    }
    Ok(dir.join("wpndatabase.db"))
}

fn filetime_to_unix_ms(filetime: i64) -> i64 {
    (filetime - 116_444_736_000_000_000) / 10_000
}

pub fn recent_toasts(limit: i64) -> Result<Vec<ToastRow>, String> {
    let path = copy_db()?;
    // 临时拷贝走 READWRITE：-wal 帧需要写权限才能恢复合并，READONLY 看不到新数据
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT n.Id, n.ArrivalTime, h.PrimaryId, \
             (SELECT AssetValue FROM HandlerAssets WHERE HandlerId=n.HandlerId AND AssetKey='DisplayName'), \
             n.Payload \
             FROM Notification n JOIN NotificationHandler h ON n.HandlerId=h.RecordId \
             WHERE n.Type='toast' ORDER BY n.Id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([limit], |row| {
            let payload: Vec<u8> = row.get::<_, Option<Vec<u8>>>(4)?.unwrap_or_default();
            Ok(ToastRow {
                id: row.get(0)?,
                arrival_ms: filetime_to_unix_ms(row.get::<_, i64>(1)?),
                aumid: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                display_name: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                snippet: text_snippet(&decode_payload(&payload)),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

fn custom_activator(aumid: &str) -> Option<String> {
    let subkey = HSTRING::from(format!(r"Software\Classes\AppUserModelId\{aumid}"));
    let value = HSTRING::from("CustomActivator");
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let mut size: u32 = 0;
        let ok = unsafe {
            RegGetValueW(root, &subkey, &value, RRF_RT_REG_SZ, None, None, Some(&mut size))
        };
        if ok.is_err() || size == 0 {
            continue;
        }
        let mut buf = vec![0u16; (size / 2) as usize];
        let ok = unsafe {
            RegGetValueW(root, &subkey, &value, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size))
        };
        if ok.is_ok() {
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            return Some(String::from_utf16_lossy(&buf[..end]));
        }
    }
    None
}

// 激活链路 spike：protocol 由前端自行验证，这里验证 CustomActivator 注册查询 + AAM 拉起
pub fn activate(aumid: &str) -> Result<String, String> {
    if let Some(clsid) = custom_activator(aumid) {
        return Ok(format!("com-registered:{clsid}"));
    }
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let mgr: IApplicationActivationManager = CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_ALL)
            .map_err(|e| e.message())?;
        let pid = mgr
            .ActivateApplication(&HSTRING::from(aumid), &HSTRING::new(), AO_NONE)
            .map_err(|e| e.message())?;
        Ok(format!("aam:pid={pid}"))
    }
}
