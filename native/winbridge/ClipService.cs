using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Threading;

namespace TopIsland.WinBridge
{
    /// <summary>
    /// 剪贴板变化监听。协议：
    ///   {"id":1,"cmd":"watch"}   -> {ok}；此后剪贴板每次变化推事件行
    ///                               {"event":"clipboard"}（无 data）。只报"变了"，内容由
    ///                               主进程读取，本 helper 不碰内容以免与主进程争用剪贴板。
    ///   {"id":2,"cmd":"unwatch"} -> 停止监听
    ///
    /// 机制：AddClipboardFormatListener + WM_CLIPBOARDUPDATE，事件驱动零轮询。需一个能收消息的
    /// HWND_MESSAGE 仅消息窗口，在后台线程跑 GetMessage 消息泵。不涉及 OLE，MTA 线程即可。
    /// </summary>
    internal sealed class ClipService
    {
        private const uint WM_CLIPBOARDUPDATE = 0x031D;
        private const uint WM_QUIT = 0x0012;
        private static readonly IntPtr HWND_MESSAGE = new IntPtr(-3);

        private readonly PushEvent _push;
        private Thread _thread;
        private uint _threadId;
        private WndProcDelegate _wndProc; // 持有委托防 GC（原生侧持有其函数指针）

        public ClipService(PushEvent push)
        {
            _push = push;
        }

        public void Handle(string cmd, Dictionary<string, object> req, Dictionary<string, object> resp)
        {
            switch (cmd)
            {
                case "watch":
                    StartWatcher();
                    break;
                case "unwatch":
                    StopWatcher();
                    break;
                default:
                    resp["ok"] = false;
                    resp["error"] = "unknown command";
                    break;
            }
        }

        private void StartWatcher()
        {
            if (_thread != null) return;
            _thread = new Thread(MessageLoop) { IsBackground = true, Name = "clip-listener" };
            _thread.Start();
        }

        private void StopWatcher()
        {
            var tid = _threadId;
            if (tid != 0) PostThreadMessageW(tid, WM_QUIT, IntPtr.Zero, IntPtr.Zero);
            _thread = null;
        }

        private void MessageLoop()
        {
            _threadId = GetCurrentThreadId();
            _wndProc = WndProcImpl;

            var cls = new WNDCLASS
            {
                lpfnWndProc = _wndProc,
                lpszClassName = "TopIslandClipWatcher",
                hInstance = GetModuleHandleW(null),
            };
            // 返回 0 可能只是"类已注册"（unwatch→watch 复用），不据此中止
            RegisterClassW(ref cls);

            var hwnd = CreateWindowExW(
                0, cls.lpszClassName, "", 0, 0, 0, 0, 0, HWND_MESSAGE, IntPtr.Zero, cls.hInstance, IntPtr.Zero);
            if (hwnd == IntPtr.Zero) return;
            AddClipboardFormatListener(hwnd);

            MSG msg;
            while (GetMessageW(out msg, IntPtr.Zero, 0, 0) > 0)
            {
                TranslateMessage(ref msg);
                DispatchMessageW(ref msg);
            }

            RemoveClipboardFormatListener(hwnd);
            DestroyWindow(hwnd);
            _threadId = 0;
        }

        private IntPtr WndProcImpl(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam)
        {
            if (msg == WM_CLIPBOARDUPDATE)
            {
                try { _push?.Invoke("clipboard", null); }
                catch { }
                return IntPtr.Zero;
            }
            return DefWindowProcW(hwnd, msg, wParam, lParam);
        }

        private delegate IntPtr WndProcDelegate(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct WNDCLASS
        {
            public uint style;
            [MarshalAs(UnmanagedType.FunctionPtr)] public WndProcDelegate lpfnWndProc;
            public int cbClsExtra;
            public int cbWndExtra;
            public IntPtr hInstance;
            public IntPtr hIcon;
            public IntPtr hCursor;
            public IntPtr hbrBackground;
            public string lpszMenuName;
            public string lpszClassName;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct MSG
        {
            public IntPtr hwnd;
            public uint message;
            public IntPtr wParam;
            public IntPtr lParam;
            public uint time;
            public int ptX;
            public int ptY;
        }

        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern ushort RegisterClassW(ref WNDCLASS lpWndClass);

        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern IntPtr CreateWindowExW(
            uint dwExStyle, string lpClassName, string lpWindowName, uint dwStyle,
            int x, int y, int nWidth, int nHeight,
            IntPtr hWndParent, IntPtr hMenu, IntPtr hInstance, IntPtr lpParam);

        [DllImport("user32.dll")]
        private static extern IntPtr DefWindowProcW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

        [DllImport("user32.dll")]
        private static extern bool DestroyWindow(IntPtr hWnd);

        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool AddClipboardFormatListener(IntPtr hwnd);

        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool RemoveClipboardFormatListener(IntPtr hwnd);

        [DllImport("user32.dll")]
        private static extern int GetMessageW(out MSG lpMsg, IntPtr hWnd, uint wMsgFilterMin, uint wMsgFilterMax);

        [DllImport("user32.dll")]
        private static extern bool TranslateMessage(ref MSG lpMsg);

        [DllImport("user32.dll")]
        private static extern IntPtr DispatchMessageW(ref MSG lpMsg);

        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool PostThreadMessageW(uint idThread, uint msg, IntPtr wParam, IntPtr lParam);

        [DllImport("kernel32.dll")]
        private static extern uint GetCurrentThreadId();

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
        private static extern IntPtr GetModuleHandleW(string lpModuleName);
    }
}
