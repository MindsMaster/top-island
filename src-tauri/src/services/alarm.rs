use std::path::Path;

use serde::Serialize;

use crate::error::{AppError, AppResult};

/// 镜像前端 AlarmSound
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlarmSound {
    /// 亦作唯一标识
    pub path: String,
    pub name: String,
}

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

pub fn sound_bytes(path: &str) -> AppResult<Vec<u8>> {
    if !is_supported_audio(path) {
        return Err(AppError::new("error.denied: 不支持的音频格式"));
    }
    let meta = std::fs::metadata(path).map_err(|e| AppError::io_at("读取铃声", &e))?;
    if meta.len() > MAX_SOUND_BYTES {
        return Err(AppError::new("error.denied: 铃声超过 20MB"));
    }
    std::fs::read(path).map_err(|e| AppError::io_at("读取铃声", &e))
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

fn is_supported_audio(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| AUDIO_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(ext)))
}

#[cfg(test)]
mod tests {
    use super::{alarm_display_name, is_alarm_wav, is_supported_audio};

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
    fn is_supported_audio_matches_extensions_case_insensitively() {
        assert!(is_supported_audio(r"C:\a\b.wav"));
        assert!(is_supported_audio("x.MP3"));
        assert!(is_supported_audio("x.flac"));
        assert!(!is_supported_audio("x.exe"));
        assert!(!is_supported_audio("noext"));
    }
}
