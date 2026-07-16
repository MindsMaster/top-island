using System;
using System.Diagnostics;
using System.Runtime.InteropServices;

namespace TopIsland.WinBridge.Interop
{
    /// <summary>user32 互操作：媒体键（SendInput）、定向 WM_APPCOMMAND、前台进程名。</summary>
    internal static class NativeInput
    {
        // 全局媒体键走 SendInput（keybd_event 已被微软标记为过时）

        private const uint INPUT_KEYBOARD = 1;
        private const uint KEYEVENTF_EXTENDEDKEY = 0x0001;
        private const uint KEYEVENTF_KEYUP = 0x0002;

        [StructLayout(LayoutKind.Sequential)]
        private struct KEYBDINPUT
        {
            public ushort wVk;
            public ushort wScan;
            public uint dwFlags;
            public uint time;
            public IntPtr dwExtraInfo;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct MOUSEINPUT
        {
            public int dx;
            public int dy;
            public uint mouseData;
            public uint dwFlags;
            public uint time;
            public IntPtr dwExtraInfo;
        }

        // union 里带上最大的 MOUSEINPUT，保证 INPUT 结构体尺寸与原生一致
        [StructLayout(LayoutKind.Explicit)]
        private struct InputUnion
        {
            [FieldOffset(0)] public MOUSEINPUT mi;
            [FieldOffset(0)] public KEYBDINPUT ki;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct INPUT
        {
            public uint type;
            public InputUnion U;
        }

        [DllImport("user32.dll", SetLastError = true)]
        private static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

        public static void PressMediaKey(ushort vk)
        {
            var inputs = new[]
            {
                Key(vk, KEYEVENTF_EXTENDEDKEY),
                Key(vk, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP),
            };
            SendInput((uint)inputs.Length, inputs, Marshal.SizeOf(typeof(INPUT)));
        }

        private static INPUT Key(ushort vk, uint flags)
        {
            return new INPUT
            {
                type = INPUT_KEYBOARD,
                U = new InputUnion { ki = new KEYBDINPUT { wVk = vk, dwFlags = flags } },
            };
        }

        // 定向 WM_APPCOMMAND：用 SendMessageTimeout，目标窗口挂起时不拖死 helper

        private const uint WM_APPCOMMAND = 0x0319;
        private const uint SMTO_ABORTIFHUNG = 0x0002;

        [DllImport("user32.dll", SetLastError = true)]
        private static extern IntPtr SendMessageTimeoutW(
            IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam,
            uint flags, uint timeoutMs, out IntPtr result);

        /// <summary>APPCOMMAND 编码在 lParam 高 16 位。返回是否送达（目标挂起/超时为 false）。</summary>
        public static bool SendAppCommand(IntPtr hwnd, int cmd)
        {
            var delivered = SendMessageTimeoutW(
                hwnd, WM_APPCOMMAND, hwnd, (IntPtr)((long)cmd << 16),
                SMTO_ABORTIFHUNG, 2000, out _);
            return delivered != IntPtr.Zero;
        }

        // 前台窗口进程名：用户正看着来源应用时就不弹岛

        [DllImport("user32.dll")]
        private static extern IntPtr GetForegroundWindow();

        [DllImport("user32.dll")]
        private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);

        /// <summary>前台窗口进程名（不含 .exe）；取不到返回 ""。</summary>
        public static string ForegroundProcessName()
        {
            try
            {
                var hwnd = GetForegroundWindow();
                if (hwnd == IntPtr.Zero) return "";
                GetWindowThreadProcessId(hwnd, out var pid);
                if (pid == 0) return "";
                using (var p = Process.GetProcessById((int)pid)) return p.ProcessName;
            }
            catch
            {
                return "";
            }
        }
    }
}
