use serde::Serialize;
use windows::core::RuntimeType;
use windows::Foundation::{AsyncStatus, IAsyncOperation};
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as SessionManager;

use crate::error::{Result, WinError};

#[derive(Debug, Clone, Serialize)]
pub struct NowPlaying {
    pub app_id: String,
    pub title: String,
    pub artist: String,
    pub status: String,
}

// windows 0.58 还没有 IAsyncOperation 的 Future 实现（0.59 起才有 windows-future），
// WinRT 异步操作只能轮询状态。调用方负责放后台线程。
fn block_on<T: RuntimeType>(op: &IAsyncOperation<T>) -> windows::core::Result<T> {
    while op.Status()? == AsyncStatus::Started {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    op.GetResults()
}

pub fn now_playing() -> Result<Option<NowPlaying>> {
    let mgr = block_on(&SessionManager::RequestAsync().map_err(|e| WinError::api("SMTC 管理器", e))?)
        .map_err(|e| WinError::api("SMTC 管理器", e))?;
    let Some(session) = mgr.GetCurrentSession().ok() else {
        return Ok(None);
    };
    let props = block_on(
        &session
            .TryGetMediaPropertiesAsync()
            .map_err(|e| WinError::api("SMTC 曲目信息", e))?,
    )
    .map_err(|e| WinError::api("SMTC 曲目信息", e))?;
    let status = session
        .GetPlaybackInfo()
        .and_then(|i| i.PlaybackStatus())
        .map_err(|e| WinError::api("SMTC 播放状态", e))?;
    Ok(Some(NowPlaying {
        app_id: session
            .SourceAppUserModelId()
            .map_err(|e| WinError::api("SMTC 来源应用", e))?
            .to_string_lossy(),
        title: props.Title().map_err(|e| WinError::api("SMTC 标题", e))?.to_string_lossy(),
        artist: props.Artist().map_err(|e| WinError::api("SMTC 艺术家", e))?.to_string_lossy(),
        status: format!("{status:?}"),
    }))
}
