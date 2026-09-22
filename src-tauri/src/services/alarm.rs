use std::path::Path;

use serde::Serialize;

use crate::error::AppResult;

/// 闹钟提示音条目（镜像 shared/ipc.ts 的 AlarmSound）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlarmSound {
    /// 音频文件绝对路径（亦作唯一标识）
    pub path: String,
    pub name: String,
}

/// data URL 要走 IPC 进渲染层内存，限 20MB（与 Electron 版一致）
const MAX_SOUND_BYTES: u64 = 20 * 1024 * 1024;

const AUDIO_EXTENSIONS: [&str; 5] = ["wav", "mp3", "ogg", "m4a", "flac"];

/// 列出系统默认闹钟音（%windir%\Media\Alarm*.wav）
pub fn list_default_sounds() -> AppResult<Vec<AlarmSound>> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let media_dir = Path::new(&windir).join("Media");
    let entries = match std::fs::read_dir(&media_dir) {
        Ok(entries) => entries,
        Err(e) => {
            // 系统 Media 目录缺失不算致命：返回空列表，前端退化为只显示自定义音
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

/// 读音频文件转 data URL（渲染层 file:// 受限，经主进程转运）。不可读/不支持返回 None。
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
        eprintln!("[alarm] {path} 超过 20MB 上限（{} 字节），拒绝转运", meta.len());
        return Ok(None);
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("[alarm] 读取 {path} 失败: {e}");
            return Ok(None);
        }
    };
    Ok(Some(format!("data:{mime};base64,{}", base64_encode(&bytes))))
}

/// 打开文件对话框选自定义音频；取消返回 None
pub fn pick_sound() -> AppResult<Option<AlarmSound>> {
    let Some(path) = island_windows::dialog::pick_open_file("", "Audio", &AUDIO_EXTENSIONS)?
    else {
        return Ok(None);
    };
    let name = Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    Ok(Some(AlarmSound { path, name }))
}

/// Electron 版命名规则："Alarm01.wav" -> "Alarm 1"，"Alarm10.wav" -> "Alarm 10"
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
    let Some(stem) = lower.strip_suffix(".wav") else { return false };
    let Some(digits) = stem.strip_prefix("alarm") else { return false };
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

// src-tauri 没有 base64 依赖（Cargo.toml 归集成阶段管），手写一个 RFC 4648 标准字母表实现
const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(B64_ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(B64_ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { B64_ALPHABET[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64_ALPHABET[(n & 63) as usize] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{alarm_display_name, base64_encode, is_alarm_wav, mime_for_ext};

    #[test]
    fn alarm_display_name_strips_leading_zero_from_numbered_alarms() {
        assert_eq!(alarm_display_name("Alarm01.wav"), "Alarm 1", "补零编号应去掉前导 0，与 Electron 版一致");
    }

    #[test]
    fn alarm_display_name_keeps_two_digit_numbers_intact() {
        assert_eq!(alarm_display_name("Alarm10.wav"), "Alarm 10", "两位数编号不能被当成补零拆开");
    }

    #[test]
    fn is_alarm_wav_accepts_only_numbered_alarm_wav_files() {
        assert!(is_alarm_wav("Alarm01.wav"), "标准系统闹钟音应入选");
        assert!(is_alarm_wav("ALARM3.WAV"), "大小写不同的 Alarm*.wav 也应入选（Electron 正则是 i 修饰）");
        assert!(!is_alarm_wav("Alarm.wav"), "没有编号的 Alarm.wav 不在 Electron 契约范围内");
        assert!(!is_alarm_wav("AlarmA.wav"), "编号必须是纯数字");
        assert!(!is_alarm_wav("Alarm01.mp3"), "只收 wav，其他格式不属于系统默认音列表");
        assert!(!is_alarm_wav("notify.wav"), "非 Alarm 前缀不能混进默认音列表");
    }

    #[test]
    fn mime_for_ext_maps_supported_audio_and_rejects_the_rest() {
        assert_eq!(mime_for_ext(r"C:\a\b.wav"), Some("audio/wav"));
        assert_eq!(mime_for_ext("x.MP3"), Some("audio/mpeg"), "扩展名大小写不应影响识别");
        assert_eq!(mime_for_ext("x.ogg"), Some("audio/ogg"));
        assert_eq!(mime_for_ext("x.m4a"), Some("audio/mp4"));
        assert_eq!(mime_for_ext("x.flac"), Some("audio/flac"));
        assert_eq!(mime_for_ext("x.exe"), None, "非音频格式必须拒绝，防止借道读出任意文件");
        assert_eq!(mime_for_ext("noext"), None, "无扩展名文件必须拒绝");
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
            assert_eq!(base64_encode(input.as_bytes()), expected, "RFC 4648 向量 {input:?} 编码结果不符");
        }
    }

    #[test]
    fn base64_encode_handles_non_ascii_binary_bytes() {
        assert_eq!(
            base64_encode(&[0x00, 0xFF, 0x80]),
            "AP+A",
            "含高位字节的二进制数据必须按字节编码，不能走字符路径"
        );
    }
}
