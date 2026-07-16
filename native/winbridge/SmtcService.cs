using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using TopIsland.WinBridge.Interop;
using Windows.Foundation;
using Windows.Media.Control;

namespace TopIsland.WinBridge
{
    /// <summary>
    /// SMTC（系统媒体传输控制）会话查询与控制。命令协议：
    ///   {"id":1,"cmd":"query"}                      -> 曲目/播放状态/时间线（ms）
    ///   {"id":2,"cmd":"thumbnail"}                  -> 当前会话专辑封面 { hash, b64 }
    ///   {"id":3,"cmd":"seek","positionMs":123456}   -> TryChangePlaybackPositionAsync
    ///   {"id":4,"cmd":"play"|"pause"|"next"|"prev"} -> 会话级控制（逐级回退）
    ///   {"id":5,"cmd":"volume","level":50}          -> 系统主音量
    ///   {"id":6,"cmd":"watch"}                      -> 订阅 WinRT 会话事件；之后播放/
    ///        暂停/切歌/时间线变化经事件行 {"event":"state","data":同 query} 推送
    ///        （150ms 防抖归并、内容不变不推）。watch 本身立即推一次当前快照。
    /// </summary>
    internal sealed class SmtcService
    {
        private readonly PushEvent _push;

        /// <summary>会话状态锁：主循环命令线程与事件/定时器线程都会碰 manager/_lastApp</summary>
        private readonly object _gate = new object();

        private GlobalSystemMediaTransportControlsSessionManager _manager;
        private DateTime _managerTime = DateTime.MinValue;

        /// <summary>上一次 query 展示的会话应用；控制命令跟着显示走，避免打到别的会话</summary>
        private string _lastApp = "";

        private bool _watching;
        private Timer _debounce;
        private string _lastPushedJson;
        private GlobalSystemMediaTransportControlsSessionManager _hookedManager;
        private readonly List<GlobalSystemMediaTransportControlsSession> _hookedSessions =
            new List<GlobalSystemMediaTransportControlsSession>();
        private TypedEventHandler<GlobalSystemMediaTransportControlsSessionManager, SessionsChangedEventArgs> _onSessionsChanged;
        private TypedEventHandler<GlobalSystemMediaTransportControlsSessionManager, CurrentSessionChangedEventArgs> _onCurrentChanged;
        private TypedEventHandler<GlobalSystemMediaTransportControlsSession, MediaPropertiesChangedEventArgs> _onMediaChanged;
        private TypedEventHandler<GlobalSystemMediaTransportControlsSession, PlaybackInfoChangedEventArgs> _onPlaybackChanged;
        private TypedEventHandler<GlobalSystemMediaTransportControlsSession, TimelinePropertiesChangedEventArgs> _onTimelineChanged;

        private static readonly Dictionary<string, int> AppCommandMap = new Dictionary<string, int>
        {
            ["play"] = 46,  // APPCOMMAND_MEDIA_PLAY
            ["pause"] = 47, // APPCOMMAND_MEDIA_PAUSE
            ["next"] = 11,  // APPCOMMAND_MEDIA_NEXTTRACK
            ["prev"] = 12,  // APPCOMMAND_MEDIA_PREVIOUSTRACK
        };

        private static readonly Dictionary<string, ushort> MediaKeyMap = new Dictionary<string, ushort>
        {
            ["play"] = 0xB3,  // VK_MEDIA_PLAY_PAUSE
            ["pause"] = 0xB3,
            ["next"] = 0xB0,  // VK_MEDIA_NEXT_TRACK
            ["prev"] = 0xB1,  // VK_MEDIA_PREV_TRACK
        };

        public SmtcService(PushEvent push)
        {
            _push = push;
        }

        public void Handle(string cmd, Dictionary<string, object> req, Dictionary<string, object> resp)
        {
            lock (_gate)
            {
                switch (cmd)
                {
                    case "query":
                        resp["data"] = QueryLocked();
                        break;
                    case "thumbnail":
                        resp["data"] = ThumbnailLocked();
                        break;
                    case "seek":
                    {
                        var session = GetTargetSessionLocked(_lastApp);
                        if (session == null)
                        {
                            resp["ok"] = false;
                            resp["error"] = "no session";
                            break;
                        }
                        // requestedPlaybackPosition 单位是 100ns tick
                        WinRt.Await(session.TryChangePlaybackPositionAsync(Json.Int(req, "positionMs", 0) * 10000));
                        break;
                    }
                    case "play":
                    case "pause":
                    case "next":
                    case "prev":
                        ControlLocked(cmd);
                        break;
                    case "volume":
                    {
                        var level = Math.Max(0, Math.Min(100, (int)Json.Int(req, "level", 50)));
                        CoreAudio.SetMasterVolumeScalar(level / 100f);
                        break;
                    }
                    case "watch":
                        StartWatchLocked();
                        break;
                    default:
                        resp["ok"] = false;
                        resp["error"] = "unknown command";
                        break;
                }
            }
        }

        /// <summary>独立校验：打印当前会话与主音量。</summary>
        public int Probe()
        {
            try { Console.OutputEncoding = Encoding.UTF8; } catch { }
            Dictionary<string, object> q;
            lock (_gate) q = QueryLocked();
            Console.WriteLine(q == null ? "no media session" : Json.Stringify(q));
            try { Console.WriteLine($"master volume: {CoreAudio.GetMasterVolumeScalar() * 100:F0}%"); }
            catch (Exception ex) { Console.WriteLine("volume read failed: " + ex.Message); }
            return 0;
        }

        // watch：订阅 manager 与全部会话的 WinRT 事件，防抖后推快照
        private void StartWatchLocked()
        {
            if (_watching)
            {
                KickSnapshot(); // 幂等：重复 watch 只再推一次当前快照
                return;
            }
            _watching = true;
            _onSessionsChanged = (m, e) => OnSessionsChanged();
            _onCurrentChanged = (m, e) => Bump();
            _onMediaChanged = (s, e) => Bump();
            _onPlaybackChanged = (s, e) => Bump();
            _onTimelineChanged = (s, e) => Bump();
            _debounce = new Timer(_ => SnapshotAndPush(), null, Timeout.Infinite, Timeout.Infinite);
            ResubscribeLocked(ManagerLocked());
            KickSnapshot();
        }

        private void KickSnapshot()
        {
            try { _debounce?.Change(0, Timeout.Infinite); } catch { }
        }

        /// <summary>150ms 防抖：切歌时 media/playback/timeline 事件成串到达，归并成一次快照。</summary>
        private void Bump()
        {
            try { _debounce?.Change(150, Timeout.Infinite); } catch { }
        }

        private void OnSessionsChanged()
        {
            // 会话增删：重挂全部会话的事件（新出现的会话此前没有订阅）
            lock (_gate)
            {
                if (_watching)
                {
                    try { ResubscribeLocked(ManagerLocked()); } catch { }
                }
            }
            Bump();
        }

        /// <summary>把事件挂到 manager 与其全部会话上；先摘旧再挂新，可重入。</summary>
        private void ResubscribeLocked(GlobalSystemMediaTransportControlsSessionManager mgr)
        {
            if (!_watching || mgr == null) return;
            if (_hookedManager != null)
            {
                try
                {
                    _hookedManager.SessionsChanged -= _onSessionsChanged;
                    _hookedManager.CurrentSessionChanged -= _onCurrentChanged;
                }
                catch { }
            }
            foreach (var s in _hookedSessions)
            {
                try
                {
                    s.MediaPropertiesChanged -= _onMediaChanged;
                    s.PlaybackInfoChanged -= _onPlaybackChanged;
                    s.TimelinePropertiesChanged -= _onTimelineChanged;
                }
                catch { }
            }
            _hookedSessions.Clear();

            mgr.SessionsChanged += _onSessionsChanged;
            mgr.CurrentSessionChanged += _onCurrentChanged;
            _hookedManager = mgr;
            foreach (var s in mgr.GetSessions())
            {
                try
                {
                    s.MediaPropertiesChanged += _onMediaChanged;
                    s.PlaybackInfoChanged += _onPlaybackChanged;
                    s.TimelinePropertiesChanged += _onTimelineChanged;
                    _hookedSessions.Add(s);
                }
                catch { }
            }
        }

        /// <summary>取快照并推送，与上次内容相同则静默（事件常成串重复）。仅防抖定时器线程执行。</summary>
        private void SnapshotAndPush()
        {
            try
            {
                Dictionary<string, object> snapshot;
                lock (_gate) snapshot = QueryLocked();
                var json = snapshot == null ? "null" : Json.Stringify(snapshot);
                if (json == _lastPushedJson) return;
                _lastPushedJson = json;
                _push?.Invoke("state", snapshot);
            }
            catch { }
        }

        // *Locked 方法：调用方须持 _gate
        /// <summary>
        /// 常驻 manager：部分应用（网易云）时间线在会话创建 1-2s 后才填充，每次
        /// 新建 manager 永远读不到；30s 定期重建，兜底陈旧句柄（重建后重挂事件）。
        /// </summary>
        private GlobalSystemMediaTransportControlsSessionManager ManagerLocked()
        {
            if (_manager == null || (DateTime.Now - _managerTime).TotalSeconds > 30)
            {
                _manager = WinRt.Await(GlobalSystemMediaTransportControlsSessionManager.RequestAsync());
                _managerTime = DateTime.Now;
                if (_watching) ResubscribeLocked(_manager);
            }
            return _manager;
        }

        /// <summary>
        /// 目标会话选择（标准 API 优先）：
        /// 1. preferApp —— 控制命令跟随上次 query 展示的会话
        /// 2. GetCurrentSession() —— 系统认定的当前媒体会话
        /// 3. 第一个正在播放的会话，再退到第一个会话（罕见兜底）
        /// </summary>
        private GlobalSystemMediaTransportControlsSession GetTargetSessionLocked(string preferApp)
        {
            var manager = ManagerLocked();
            if (manager == null) return null;

            if (!string.IsNullOrEmpty(preferApp))
            {
                foreach (var s in manager.GetSessions())
                {
                    try { if (s.SourceAppUserModelId == preferApp) return s; }
                    catch { }
                }
            }

            var current = manager.GetCurrentSession();
            if (current != null) return current;

            GlobalSystemMediaTransportControlsSession first = null;
            foreach (var s in manager.GetSessions())
            {
                try
                {
                    if (first == null) first = s;
                    if (s.GetPlaybackInfo().PlaybackStatus ==
                        GlobalSystemMediaTransportControlsSessionPlaybackStatus.Playing)
                        return s;
                }
                catch { }
            }
            return first;
        }

        /// <summary>查询当前会话；无会话/查询失败返回 null（协议层 data=null）。</summary>
        private Dictionary<string, object> QueryLocked()
        {
            try
            {
                var session = GetTargetSessionLocked("");
                if (session == null) return null;
                _lastApp = session.SourceAppUserModelId ?? "";

                var info = WinRt.Await(session.TryGetMediaPropertiesAsync());
                var playback = session.GetPlaybackInfo();
                var playing = playback.PlaybackStatus ==
                              GlobalSystemMediaTransportControlsSessionPlaybackStatus.Playing;

                long positionMs = 0;
                long durationMs = 0;
                try
                {
                    var timeline = session.GetTimelineProperties();
                    if (timeline.EndTime.TotalMilliseconds > 0)
                    {
                        positionMs = (long)timeline.Position.TotalMilliseconds;
                        durationMs = (long)timeline.EndTime.TotalMilliseconds;
                        // 多数播放器（QQ 音乐/Spotify/浏览器）只在暂停/seek/换曲时刷新
                        // Position，播放中它是冻结的。用 LastUpdatedTime 到现在的墙钟
                        // 差补偿，否则读到陈旧进度、歌词滞后。仅播放中补偿；封顶时长；
                        // LastUpdatedTime 未设置（1601 纪元）时差值巨大 -> 丢弃。
                        if (playing)
                        {
                            var elapsed = (DateTimeOffset.Now - timeline.LastUpdatedTime).TotalMilliseconds;
                            if (elapsed > 0 && elapsed < 30000)
                                positionMs = Math.Min(positionMs + (long)elapsed, durationMs);
                        }
                    }
                }
                catch { }

                var seekable = false;
                try { seekable = playback.Controls.IsPlaybackPositionEnabled; }
                catch { }

                return new Dictionary<string, object>
                {
                    ["title"] = info.Title ?? "",
                    ["artist"] = info.Artist ?? "",
                    ["app"] = session.SourceAppUserModelId ?? "",
                    ["playing"] = playing,
                    ["positionMs"] = positionMs,
                    ["durationMs"] = durationMs,
                    ["seekSupported"] = seekable,
                };
            }
            catch
            {
                return null;
            }
        }

        /// <summary>当前会话专辑封面 -> { hash, b64 }；无封面/失败返回 null。</summary>
        private Dictionary<string, object> ThumbnailLocked()
        {
            try
            {
                var session = GetTargetSessionLocked(_lastApp);
                if (session == null) return null;
                var info = WinRt.Await(session.TryGetMediaPropertiesAsync());
                var thumbRef = info.Thumbnail;
                if (thumbRef == null) return null;

                byte[] bytes;
                var ras = WinRt.Await(thumbRef.OpenReadAsync());
                if (ras == null) return null;
                using (var stream = ras.AsStreamForRead())
                using (var buffer = new MemoryStream())
                {
                    stream.CopyTo(buffer);
                    bytes = buffer.ToArray();
                }
                if (bytes.Length == 0) return null;

                string hash;
                using (var md5 = MD5.Create())
                    hash = BitConverter.ToString(md5.ComputeHash(bytes)).Replace("-", "").ToLowerInvariant();

                return new Dictionary<string, object>
                {
                    ["hash"] = hash,
                    ["b64"] = Convert.ToBase64String(bytes),
                };
            }
            catch
            {
                return null;
            }
        }

        /// <summary>
        /// 播放控制，逐级回退（与应用无关的通用链路）：
        /// 1. 会话级 TryXxxAsync —— 标准接口；返回 FALSE 表示应用拒绝。个别应用还会
        ///    说谎：网易云 TryPauseAsync 返回 TRUE 但不暂停（其自身 elog 验证过），
        ///    此时靠下级兜底。
        /// 2. 定向 WM_APPCOMMAND 打到播放器自己的窗口 —— 无路由歧义（全局媒体键会被
        ///    shell 路由给"最近的"媒体应用，可能被僵尸浏览器会话劫持）。
        /// 3. 全局媒体键 —— 最后手段（如没有 win32 窗口的 UWP 应用）。
        /// </summary>
        private void ControlLocked(string action)
        {
            var ok = false;
            var app = "";
            try
            {
                var session = GetTargetSessionLocked(_lastApp);
                if (session != null)
                {
                    app = session.SourceAppUserModelId ?? "";
                    switch (action)
                    {
                        case "play": ok = WinRt.Await(session.TryPlayAsync()); break;
                        case "pause": ok = WinRt.Await(session.TryPauseAsync()); break;
                        case "next": ok = WinRt.Await(session.TrySkipNextAsync()); break;
                        case "prev": ok = WinRt.Await(session.TrySkipPreviousAsync()); break;
                    }
                }
            }
            catch { }
            if (ok) return;

            if (AppCommandMap.TryGetValue(action, out var cmd) && app.Length > 0 && SendAppCommand(app, cmd))
                return;
            if (MediaKeyMap.TryGetValue(action, out var vk))
                NativeInput.PressMediaKey(vk);
        }

        /// <summary>
        /// 把 WM_APPCOMMAND 发给 win32 SourceAppUserModelId（形如 "cloudmusic.exe"）
        /// 对应进程的主窗口。送达返回 true。
        /// </summary>
        private static bool SendAppCommand(string appId, int cmd)
        {
            try
            {
                if (!appId.EndsWith(".exe", StringComparison.OrdinalIgnoreCase)) return false;
                var name = appId.Substring(0, appId.Length - 4);
                var procs = Process.GetProcessesByName(name);
                try
                {
                    foreach (var p in procs)
                        if (p.MainWindowHandle != IntPtr.Zero)
                            return NativeInput.SendAppCommand(p.MainWindowHandle, cmd);
                }
                finally
                {
                    foreach (var p in procs) p.Dispose();
                }
            }
            catch { }
            return false;
        }
    }
}
