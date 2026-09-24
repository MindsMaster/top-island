use island_core::{LyricsData, MusicAction, MusicState};

use super::off_thread;
use crate::error::AppResult;
use crate::services;
use crate::services::music::BridgeStatus;

#[tauri::command]
pub async fn music_state() -> AppResult<MusicState> {
    off_thread(|| Ok(services::music::current_state())).await
}

#[tauri::command]
pub async fn music_control(action: MusicAction, level: Option<i64>) -> AppResult<String> {
    off_thread(move || services::music::control(action, level)).await
}

#[tauri::command]
pub async fn music_seek(position_ms: i64) -> AppResult<bool> {
    off_thread(move || Ok(services::music::seek(position_ms))).await
}

/// id 已切歌时回 None
#[tauri::command]
pub async fn music_lyrics(id: String) -> AppResult<Option<LyricsData>> {
    off_thread(move || Ok(services::music::lyrics(&id))).await
}

#[tauri::command]
pub async fn music_bridge_status() -> AppResult<BridgeStatus> {
    off_thread(|| Ok(services::music::bridge_status())).await
}
