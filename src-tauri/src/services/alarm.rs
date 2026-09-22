use std::path::Path;

use serde::Serialize;

use crate::error::AppResult;

/// 镜像前端 AlarmSound
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlarmSound {
    /// 亦作唯一标识
    pub path: String,
    pub name: String,
}

/// 走 IPC 限 20MB
const MAX_SOUND_BYTES: u64 = 20 * 1024 * 1024;

const AUDIO_EXTENSIONS: [&str; 5] = ["wav", "mp3", "ogg", "m4a", "flac"];

/// %windir%\Media 下系统音
pub fn list_default_sounds() -> AppResult<Vec<AlarmSound>> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let media_dir = Path::new(&windir).join("Media");
    let entries = match std::fs::read_dir(&media_dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("[alarm] 读取 {} 失败: {e}", media_dir.display());
            return Ok(Vec::new());
        }
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| match entry {
            Ok(e) => Some(e.file_name().to_string_lossy().into_owned()),
            Err(e) => {
                eprintln!("[alarm] 枚举 {} 条目失败: {e}", media_dir.display());
                None
            }
        })
        .filter(|name| is_alarm_wav(name))
        .collect();
    names.sort();
    Ok(names
        .into_iter()
        .map(|name| AlarmSound {
            path: media_dir.join(&name).to_string_lossy().into_owned(),
            name: alarm_display_name(&name),
        })
        .collect())
}

/// file:// 受限 主进程转运
pub fn sound_data_url(path: &str) -> AppResult<Option<String>> {
    let Some(mime) = mime_for_ext(path) else {
        return Ok(None);
    };
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) => {
            eprintln!("[alarm] stat {path} 失败: {e}");
            return Ok(None);
        }
    };
    if meta.len() > MAX_SOUND_BYTES {
        eprintln!(
            "[alarm] {path} 超过 20MB 上限（{} 字节），拒绝转运",
            meta.len()
        );
        return Ok(None);
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("[alarm] 读取 {path} 失败: {e}");
            return Ok(None);
        }
    };
    Ok(Some(format!(
        "data:{mime};base64,{}",
        base64_encode(&bytes)
    )))
}

pub fn pick_sound() -> AppResult<Option<AlarmSound>> {
    let Some(path) = island_windows::dialog::pick_open_file("", "Audio", &AUDIO_EXTENSIONS)? else {
        return Ok(None);
    };
    let name = Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    Ok(Some(AlarmSound { path, name }))
}

fn alarm_display_name(file_name: &str) -> String {
    let stem = if file_name.to_ascii_lowercase().ends_with(".wav") {
        &file_name[..file_name.len() - 4]
    } else {
        file_name
    };
    let rest = stem
        .strip_prefix("Alarm0")
        .or_else(|| stem.strip_prefix("Alarm"))
        .unwrap_or(stem);
    format!("Alarm {rest}")
}

fn is_alarm_wav(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(stem) = lower.strip_suffix(".wav") else {
        return false;
    };
    let Some(digits) = stem.strip_prefix("alarm") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

fn mime_for_ext(path: &str) -> Option<&'static str> {
    let ext = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "wav" => Some("audio/wav"),
        "mp3" => Some("audio/mpeg"),
        "ogg" => Some("audio/ogg"),
        "m4a" => Some("audio/mp4"),
        "flac" => Some("audio/flac"),
        _ => None,
    }
}

/// 无 base64 依赖 手写
const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(B64_ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(B64_ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64_ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{alarm_display_name, base64_encode, is_alarm_wav, mime_for_ext};

    #[test]
    fn alarm_display_name_strips_leading_zero_from_numbered_alarms() {
        assert_eq!(alarm_display_name("Alarm01.wav"), "Alarm 1");
    }

    #[test]
    fn alarm_display_name_keeps_two_digit_numbers_intact() {
        assert_eq!(alarm_display_name("Alarm10.wav"), "Alarm 10");
    }

    #[test]
    fn is_alarm_wav_accepts_only_numbered_alarm_wav_files() {
        assert!(is_alarm_wav("Alarm01.wav"));
        assert!(is_alarm_wav("ALARM3.WAV"));
        assert!(!is_alarm_wav("Alarm.wav"));
        assert!(!is_alarm_wav("AlarmA.wav"));
        assert!(!is_alarm_wav("Alarm01.mp3"));
        assert!(!is_alarm_wav("notify.wav"));
    }

    #[test]
    fn mime_for_ext_maps_supported_audio_and_rejects_the_rest() {
        assert_eq!(mime_for_ext(r"C:\a\b.wav"), Some("audio/wav"));
        assert_eq!(mime_for_ext("x.MP3"), Some("audio/mpeg"));
        assert_eq!(mime_for_ext("x.ogg"), Some("audio/ogg"));
        assert_eq!(mime_for_ext("x.m4a"), Some("audio/mp4"));
        assert_eq!(mime_for_ext("x.flac"), Some("audio/flac"));
        assert_eq!(mime_for_ext("x.exe"), None);
        assert_eq!(mime_for_ext("noext"), None);
    }

    #[test]
    fn base64_encode_matches_rfc4648_test_vectors() {
        let cases = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (input, expected) in cases {
            assert_eq!(base64_encode(input.as_bytes()), expected);
        }
    }

    #[test]
    fn base64_encode_handles_non_ascii_binary_bytes() {
        assert_eq!(base64_encode(&[0x00, 0xFF, 0x80]), "AP+A");
    }
}
