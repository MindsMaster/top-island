use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

use crate::error::{Result, WinError};

/// 重复调用返回 S_FALSE RPC_E_CHANGED_MODE 均可用
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
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
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

pub fn set_master_volume_scalar(level: f32) -> Result<()> {
    with_endpoint_volume("设置主音量", |v| unsafe {
        v.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null())
    })
}

pub fn master_volume_scalar() -> Result<f32> {
    with_endpoint_volume("读取主音量", |v| unsafe {
        v.GetMasterVolumeLevelScalar()
    })
}
