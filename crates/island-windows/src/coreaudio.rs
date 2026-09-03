//! Core Audio 主音量（IAudioEndpointVolume），music_control 的 volume 动作用。
//! 每次调用按次创建按次释放 COM 对象，常驻进程不留悬挂引用。

use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

use crate::error::{Result, WinError};

/// 后台线程没有消息泵也未初始化 COM；重复调用返回 S_FALSE / RPC_E_CHANGED_MODE，均可用
pub(crate) fn com_init_mta() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn with_endpoint_volume<T>(
    context: &'static str,
    f: impl FnOnce(&IAudioEndpointVolume) -> windows::core::Result<T>,
) -> Result<T> {
    com_init_mta();
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .map_err(|e| WinError::api("音频设备枚举器", e))?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .map_err(|e| WinError::api("默认音频输出设备", e))?;
        let volume: IAudioEndpointVolume = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| WinError::api("主音量接口", e))?;
        f(&volume).map_err(|e| WinError::api(context, e))
    }
}

/// 设置默认输出设备主音量（0.0 - 1.0）
pub fn set_master_volume_scalar(level: f32) -> Result<()> {
    with_endpoint_volume("设置主音量", |v| unsafe {
        v.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null())
    })
}

/// 读当前主音量（0.0 - 1.0）
pub fn master_volume_scalar() -> Result<f32> {
    with_endpoint_volume("读取主音量", |v| unsafe { v.GetMasterVolumeLevelScalar() })
}
