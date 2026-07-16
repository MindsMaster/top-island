using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.Win32;

namespace TopIsland.WinBridge.Interop
{
    /// <summary>
    /// 复现一次 toast 点击。链路与系统行为一致：
    /// protocol 直开 → 应用注册的 COM activator（深链，能定位到会话）→
    /// IApplicationActivationManager（带参前台启动）→ shell:AppsFolder 兜底（仅拉起应用）。
    /// </summary>
    internal static class ToastActivation
    {
        [ComImport, Guid("53E31837-6600-4A81-9395-75CFFE746F94"),
         InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface INotificationActivationCallback
        {
            void Activate(
                [MarshalAs(UnmanagedType.LPWStr)] string appUserModelId,
                [MarshalAs(UnmanagedType.LPWStr)] string invokedArgs,
                IntPtr data, uint count);
        }

        [ComImport, Guid("2E941141-7F97-4756-BA1D-9DECDE894A3D"),
         InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IApplicationActivationManager
        {
            void ActivateApplication(
                [MarshalAs(UnmanagedType.LPWStr)] string appUserModelId,
                [MarshalAs(UnmanagedType.LPWStr)] string arguments,
                int options, out uint processId);
            // 只用第一个方法，其余按 vtable 占位（签名兼容即可，不会被调用）
            void ActivateForFile(IntPtr a, IntPtr b, IntPtr c, out uint pid);
            void ActivateForProtocol(IntPtr a, IntPtr b, out uint pid);
        }

        [ComImport, Guid("45BA127D-10A8-46EA-8AB7-56EA9078943C")]
        private class ApplicationActivationManagerComObject { }

        /// <summary>按链路激活，返回实际生效的方式：protocol/com/aam/shell/failed。</summary>
        public static string Run(string aumid, string launch, string atype)
        {
            launch = launch ?? "";

            if (string.Equals(atype, "protocol", StringComparison.OrdinalIgnoreCase) && launch.Length > 0)
            {
                try
                {
                    Process.Start(new ProcessStartInfo(launch) { UseShellExecute = true });
                    return "protocol";
                }
                catch { }
            }

            var clsid = CustomActivatorClsid(aumid);
            if (!string.IsNullOrEmpty(clsid))
            {
                try
                {
                    var type = Type.GetTypeFromCLSID(new Guid(clsid.Trim('{', '}')), throwOnError: true);
                    var callback = (INotificationActivationCallback)Activator.CreateInstance(type);
                    callback.Activate(aumid, launch, IntPtr.Zero, 0);
                    return "com";
                }
                catch { }
            }

            try
            {
                var manager = (IApplicationActivationManager)new ApplicationActivationManagerComObject();
                manager.ActivateApplication(aumid, launch, 0, out _);
                return "aam";
            }
            catch { }

            try
            {
                Process.Start(new ProcessStartInfo("explorer.exe", "shell:AppsFolder\\" + aumid)
                {
                    UseShellExecute = true,
                });
                return "shell";
            }
            catch { }

            return "failed";
        }

        /// <summary>应用注册的 toast activator CLSID（HKCU 优先，其次 HKLM）。</summary>
        private static string CustomActivatorClsid(string aumid)
        {
            foreach (var root in new[] { "HKEY_CURRENT_USER", "HKEY_LOCAL_MACHINE" })
            {
                try
                {
                    if (Registry.GetValue(root + @"\Software\Classes\AppUserModelId\" + aumid,
                            "CustomActivator", null) is string v && v.Length > 0)
                        return v;
                }
                catch { }
            }
            return null;
        }
    }
}
