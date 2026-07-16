using System;
using System.Runtime.InteropServices;

namespace TopIsland.WinBridge.Interop
{
    /// <summary>
    /// Core Audio 主音量（IAudioEndpointVolume）。COM 接口须完整声明 vtable，不能占位截断；
    /// 事件上下文参数传 IntPtr.Zero 表示不带上下文。
    /// </summary>
    internal static class CoreAudio
    {
        private const int CLSCTX_ALL = 0x17;
        private const int DEVICE_RENDER = 0;    // EDataFlow.eRender
        private const int ROLE_MULTIMEDIA = 1;  // ERole.eMultimedia

        [ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")]
        private class MMDeviceEnumeratorComObject { }

        [ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"),
         InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IMMDeviceEnumerator
        {
            [PreserveSig] int EnumAudioEndpoints(int dataFlow, int stateMask, out IntPtr devices);
            [PreserveSig] int GetDefaultAudioEndpoint(int dataFlow, int role, out IMMDevice endpoint);
            [PreserveSig] int GetDevice([MarshalAs(UnmanagedType.LPWStr)] string id, out IMMDevice device);
            [PreserveSig] int RegisterEndpointNotificationCallback(IntPtr client);
            [PreserveSig] int UnregisterEndpointNotificationCallback(IntPtr client);
        }

        [ComImport, Guid("D666063F-1587-4E43-81F1-B948E807363F"),
         InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IMMDevice
        {
            [PreserveSig] int Activate(ref Guid iid, int clsCtx, IntPtr activationParams,
                [MarshalAs(UnmanagedType.IUnknown)] out object iface);
            [PreserveSig] int OpenPropertyStore(int stgmAccess, out IntPtr properties);
            [PreserveSig] int GetId(out IntPtr id);
            [PreserveSig] int GetState(out int state);
        }

        [ComImport, Guid("5CDF2C82-841E-4546-9722-0CF74078229A"),
         InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IAudioEndpointVolume
        {
            [PreserveSig] int RegisterControlChangeNotify(IntPtr notify);
            [PreserveSig] int UnregisterControlChangeNotify(IntPtr notify);
            [PreserveSig] int GetChannelCount(out uint channelCount);
            [PreserveSig] int SetMasterVolumeLevel(float levelDb, IntPtr eventContext);
            [PreserveSig] int SetMasterVolumeLevelScalar(float level, IntPtr eventContext);
            [PreserveSig] int GetMasterVolumeLevel(out float levelDb);
            [PreserveSig] int GetMasterVolumeLevelScalar(out float level);
            [PreserveSig] int SetChannelVolumeLevel(uint channel, float levelDb, IntPtr eventContext);
            [PreserveSig] int SetChannelVolumeLevelScalar(uint channel, float level, IntPtr eventContext);
            [PreserveSig] int GetChannelVolumeLevel(uint channel, out float levelDb);
            [PreserveSig] int GetChannelVolumeLevelScalar(uint channel, out float level);
            [PreserveSig] int SetMute([MarshalAs(UnmanagedType.Bool)] bool mute, IntPtr eventContext);
            [PreserveSig] int GetMute([MarshalAs(UnmanagedType.Bool)] out bool mute);
            [PreserveSig] int GetVolumeStepInfo(out uint step, out uint stepCount);
            [PreserveSig] int VolumeStepUp(IntPtr eventContext);
            [PreserveSig] int VolumeStepDown(IntPtr eventContext);
            [PreserveSig] int QueryHardwareSupport(out uint mask);
            [PreserveSig] int GetVolumeRange(out float min, out float max, out float increment);
        }

        /// <summary>设置默认输出设备主音量（0.0 - 1.0）。</summary>
        public static void SetMasterVolumeScalar(float level)
        {
            WithEndpointVolume(v =>
                Marshal.ThrowExceptionForHR(v.SetMasterVolumeLevelScalar(level, IntPtr.Zero)));
        }

        /// <summary>读当前主音量（0.0 - 1.0）。诊断用（--probe）。</summary>
        public static float GetMasterVolumeScalar()
        {
            float level = 0;
            WithEndpointVolume(v =>
                Marshal.ThrowExceptionForHR(v.GetMasterVolumeLevelScalar(out level)));
            return level;
        }

        private static void WithEndpointVolume(Action<IAudioEndpointVolume> action)
        {
            IMMDeviceEnumerator enumerator = null;
            IMMDevice device = null;
            object volume = null;
            try
            {
                enumerator = (IMMDeviceEnumerator)new MMDeviceEnumeratorComObject();
                Marshal.ThrowExceptionForHR(
                    enumerator.GetDefaultAudioEndpoint(DEVICE_RENDER, ROLE_MULTIMEDIA, out device));
                var iid = typeof(IAudioEndpointVolume).GUID;
                Marshal.ThrowExceptionForHR(device.Activate(ref iid, CLSCTX_ALL, IntPtr.Zero, out volume));
                action((IAudioEndpointVolume)volume);
            }
            finally
            {
                // 常驻进程按次创建按次释放，不留悬挂 RCW
                if (volume != null) Marshal.ReleaseComObject(volume);
                if (device != null) Marshal.ReleaseComObject(device);
                if (enumerator != null) Marshal.ReleaseComObject(enumerator);
            }
        }
    }
}
