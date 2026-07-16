using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Threading;
using Microsoft.Win32;
using TopIsland.WinBridge.Interop;
using Windows.Foundation;
using Windows.UI.Notifications;
using Windows.UI.Notifications.Management;

namespace TopIsland.WinBridge
{
    /// <summary>
    /// 消息托管。完整数据（launch 深链等）来自通知中心存储库 wpndatabase.db；"有没有新通知"
    /// 的探测走 UserNotificationListener（纯内存，其 UserNotification.Id 与库中 Notification.Id
    /// 同源同值），只有出现新 Id 才做拷库查询的重活。命令协议：
    ///   {"id":1,"cmd":"watch","sinceId":123}  -> { maxId }。sinceId&lt;0 取当前基线
    ///        （不弹历史）。之后新 toast 经事件行 {"event":"toasts","data":{items,maxId}}
    ///        推送：NotificationChanged 事件（若可用）触发 + 1s 轻探测兜底；
    ///        listener 无权限时退化为 1.5s 直接扫库。
    ///   {"id":2,"cmd":"unwatch"}              -> 停止监视（水位保留）
    ///   {"id":3,"cmd":"poll","sinceId":123}   -> { items:[...], maxId }（兜底/调试）
    ///   {"id":4,"cmd":"baseline"}             -> { maxId }（当前水位，不带条目）
    ///   {"id":5,"cmd":"activate","aumid":"..","launch":"..","atype":".."} -> { method }
    ///   {"id":6,"cmd":"bannerGet","aumid":".."} -> { value }（null=系统默认）
    ///   {"id":7,"cmd":"bannerSet","aumid":"..","value":0|-1} -> { ok }（-1=删除还原默认）
    ///   {"id":8,"cmd":"foreground"}           -> { exe }
    ///
    /// 不用 FileSystemWatcher 盯库文件：wpndatabase 被通知平台常开句柄持有、写入走系统缓存，
    /// 目录变更通知只在缓存刷盘时才产生（ReadDirectoryChangesW 的文档化行为），实测新 toast
    /// 落库后 8s 内毫无事件，对该库天然失聪。
    /// </summary>
    internal sealed class NotifyService
    {
        private static readonly string SrcDir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            @"Microsoft\Windows\Notifications");

        private static readonly string TmpDir = Path.Combine(Path.GetTempPath(), "topisland-wpn");

        /// <summary>按应用横幅开关（HKCU）。ShowBanner=0 关右下角弹窗，通知中心照常收（我们照常读）。可逆。</summary>
        private const string NotifSettings = @"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings";

        private readonly PushEvent _push;

        /// <summary>水位/监视器状态锁：主循环命令线程与事件/定时器线程共享</summary>
        private readonly object _gate = new object();

        /// <summary>已推送的最大行 Id（含被过滤的空壳 toast，避免反复重扫）；-1 = 未 watch</summary>
        private long _watermark = -1;

        private UserNotificationListener _listener;
        private TypedEventHandler<UserNotificationListener, UserNotificationChangedEventArgs> _onChanged;
        private Timer _debounce;
        private Timer _lightPoll;

        public NotifyService(PushEvent push)
        {
            _push = push;
        }

        public void Handle(string cmd, Dictionary<string, object> req, Dictionary<string, object> resp)
        {
            switch (cmd)
            {
                case "watch":
                {
                    long max;
                    lock (_gate)
                    {
                        var since = Json.Int(req, "sinceId", -1);
                        _watermark = since >= 0 ? since : GetMaxId();
                        StartWatcherLocked();
                        max = _watermark;
                    }
                    resp["data"] = new Dictionary<string, object> { ["maxId"] = max };
                    break;
                }
                case "unwatch":
                    lock (_gate) StopWatcherLocked();
                    break;
                case "baseline":
                    resp["data"] = new Dictionary<string, object> { ["maxId"] = GetMaxId() };
                    break;
                case "poll":
                {
                    var since = Json.Int(req, "sinceId", 0);
                    var (items, rawMax) = QuerySince(since);
                    resp["data"] = new Dictionary<string, object>
                    {
                        ["items"] = items,
                        ["maxId"] = Math.Max(since, rawMax),
                    };
                    break;
                }
                case "activate":
                    resp["data"] = new Dictionary<string, object>
                    {
                        ["method"] = ToastActivation.Run(
                            Json.Str(req, "aumid"), Json.Str(req, "launch"), Json.Str(req, "atype")),
                    };
                    break;
                case "bannerGet":
                    resp["data"] = new Dictionary<string, object>
                    {
                        ["value"] = GetBanner(Json.Str(req, "aumid")),
                    };
                    break;
                case "bannerSet":
                    resp["data"] = new Dictionary<string, object>
                    {
                        ["ok"] = SetBanner(Json.Str(req, "aumid"), (int)Json.Int(req, "value", -1)),
                    };
                    break;
                case "foreground":
                    resp["data"] = new Dictionary<string, object>
                    {
                        ["exe"] = NativeInput.ForegroundProcessName(),
                    };
                    break;
                default:
                    resp["ok"] = false;
                    resp["error"] = "unknown command";
                    break;
            }
        }

        // watch：NotificationChanged 事件（打包应用才保证生效，能挂就挂）+ 1s 轻探测
        // （listener 内存枚举比对最大 Id）兜底，命中新 Id 才扫库
        private void StartWatcherLocked()
        {
            if (_debounce == null)
                _debounce = new Timer(_ => Scan(), null, Timeout.Infinite, Timeout.Infinite);
            if (_lightPoll != null) return;

            if (_listener == null) _listener = InitListener();
            if (_listener != null && _onChanged == null)
            {
                _onChanged = (s, e) => BumpScan();
                // 非打包进程通常不触发此事件（需包标识），挂上没坏处，真触发就是零延迟
                try { _listener.NotificationChanged += _onChanged; }
                catch { _onChanged = null; }
            }
            if (_listener == null)
                Console.Error.WriteLine("[notify] UserNotificationListener 不可用，退化为周期扫库");

            // 有 listener：1s 轻探测（纯内存）；没有：1.5s 直接扫库
            var interval = _listener != null ? 1000 : 1500;
            _lightPoll = new Timer(_ => LightPoll(), null, interval, interval);
        }

        private void StopWatcherLocked()
        {
            if (_listener != null && _onChanged != null)
            {
                try { _listener.NotificationChanged -= _onChanged; } catch { }
                _onChanged = null;
            }
            _debounce?.Dispose();
            _debounce = null;
            _lightPoll?.Dispose();
            _lightPoll = null;
        }

        private static UserNotificationListener InitListener()
        {
            try
            {
                var listener = UserNotificationListener.Current;
                var status = WinRt.Await(listener.RequestAccessAsync());
                return status == UserNotificationListenerAccessStatus.Allowed ? listener : null;
            }
            catch
            {
                return null;
            }
        }

        /// <summary>300ms 防抖：事件成串到达时归并成一次扫库。</summary>
        private void BumpScan()
        {
            try { _debounce?.Change(300, Timeout.Infinite); } catch { }
        }

        /// <summary>轻探测：listener 内存枚举当前 toast 最大 Id，超过水位才做拷库查询的重活。</summary>
        private void LightPoll()
        {
            try
            {
                long watermark;
                lock (_gate)
                {
                    if (_watermark < 0 || _lightPoll == null) return;
                    watermark = _watermark;
                }
                if (_listener != null)
                {
                    var notifs = WinRt.Await(_listener.GetNotificationsAsync(NotificationKinds.Toast));
                    long max = 0;
                    foreach (var n in notifs)
                        if (n.Id > max) max = n.Id;
                    if (max <= watermark) return;
                }
                Scan();
            }
            catch { }
        }

        /// <summary>扫描水位之上的新 toast 并推送；水位含空壳行，空转不推。</summary>
        private void Scan()
        {
            try
            {
                List<Dictionary<string, object>> items;
                long max;
                lock (_gate)
                {
                    if (_watermark < 0 || _debounce == null) return; // 未 watch / 已 unwatch
                    var (list, rawMax) = QuerySince(_watermark);
                    if (rawMax > _watermark) _watermark = rawMax;
                    if (list.Count == 0) return;
                    items = list;
                    max = _watermark;
                }
                _push?.Invoke("toasts", new Dictionary<string, object> { ["items"] = items, ["maxId"] = max });
            }
            catch { }
        }

        // 先拷到临时目录再打开：绝不锁真库，且连 -wal/-shm 一起拷，
        // 才能看到已提交但未合并的 WAL 帧。
        private static string CopyDb()
        {
            try { Directory.CreateDirectory(TmpDir); } catch { return null; }
            var ok = false;
            foreach (var suffix in new[] { "", "-wal", "-shm" })
            {
                var src = Path.Combine(SrcDir, "wpndatabase.db" + suffix);
                var dst = Path.Combine(TmpDir, "wpndatabase.db" + suffix);
                try
                {
                    if (File.Exists(src))
                    {
                        File.Copy(src, dst, overwrite: true);
                        if (suffix.Length == 0) ok = true;
                    }
                    else if (File.Exists(dst))
                    {
                        File.Delete(dst);
                    }
                }
                catch
                {
                    // 拷贝撞上系统写入属正常，等下一次事件/兜底扫描
                    if (suffix.Length == 0) ok = false;
                }
            }
            return ok ? Path.Combine(TmpDir, "wpndatabase.db") : null;
        }

        /// <summary>
        /// Id 大于 sinceId 的 toast。RawMax 是行级最大 Id（含被过滤的空壳
        /// 进度条/更新器 toast）——水位按它推进，否则空壳行每轮重复扫描。
        /// </summary>
        private static (List<Dictionary<string, object>> Items, long RawMax) QuerySince(long sinceId)
        {
            var items = new List<Dictionary<string, object>>();
            long rawMax = sinceId;
            var db = CopyDb();
            if (db == null) return (items, rawMax);
            foreach (var row in WpnDatabase.QueryToasts(db, sinceId))
            {
                if (row.Id > rawMax) rawMax = row.Id;
                var payload = ToastPayload.Parse(row.Payload);
                // 跳过无正文的 toast（进度条/更新器空壳）
                if (payload.Title.Length == 0 && payload.Body.Length == 0) continue;
                items.Add(new Dictionary<string, object>
                {
                    ["id"] = row.Id,
                    ["aumid"] = row.Aumid,
                    ["app"] = row.DisplayName.Length > 0 ? row.DisplayName : row.Aumid,
                    ["icon"] = row.IconUri,
                    ["image"] = payload.Image,
                    ["title"] = payload.Title,
                    ["body"] = payload.Body,
                    ["launch"] = payload.Launch,
                    ["atype"] = payload.AType,
                    ["arrival"] = row.ArrivalMs,
                });
            }
            return (items, rawMax);
        }

        private static long GetMaxId()
        {
            var db = CopyDb();
            return db == null ? 0 : WpnDatabase.QueryMaxId(db);
        }

        /// <summary>当前 ShowBanner 值；键/值不存在（= 系统默认）返回 null。</summary>
        private static object GetBanner(string aumid)
        {
            if (string.IsNullOrEmpty(aumid)) return null;
            try
            {
                using (var key = Registry.CurrentUser.OpenSubKey(NotifSettings + "\\" + aumid))
                {
                    return key?.GetValue("ShowBanner") is int v ? (object)v : null;
                }
            }
            catch
            {
                return null;
            }
        }

        /// <summary>value = -1 删除 ShowBanner（还原系统默认）；否则写 DWORD。</summary>
        private static bool SetBanner(string aumid, int value)
        {
            if (string.IsNullOrEmpty(aumid)) return false;
            try
            {
                if (value < 0)
                {
                    using (var key = Registry.CurrentUser.OpenSubKey(NotifSettings + "\\" + aumid, writable: true))
                        key?.DeleteValue("ShowBanner", throwOnMissingValue: false);
                    return true;
                }
                using (var key = Registry.CurrentUser.CreateSubKey(NotifSettings + "\\" + aumid))
                {
                    if (key == null) return false;
                    key.SetValue("ShowBanner", value, RegistryValueKind.DWord);
                }
                return true;
            }
            catch
            {
                return false;
            }
        }

        /// <summary>打印通知中心现存的全部 toast。</summary>
        public int Probe()
        {
            try { Console.OutputEncoding = Encoding.UTF8; } catch { }
            var (items, _) = QuerySince(0);
            if (items.Count == 0)
            {
                Console.WriteLine(
                    "No toasts currently in the Action Center. Trigger a notification " +
                    "(send yourself a WeChat/QQ message, etc.), leave it in the Action Center, then run --probe again.");
                return 0;
            }
            foreach (var i in items)
            {
                Console.WriteLine($"[{i["id"]}] {i["app"]}");
                Console.WriteLine($"   title : {i["title"]}");
                Console.WriteLine($"   body  : {i["body"]}");
                Console.WriteLine($"   aumid : {i["aumid"]}");
                Console.WriteLine($"   launch: {i["launch"]}  (activationType={i["atype"]})");
                Console.WriteLine($"   icon  : {i["icon"]}");
                Console.WriteLine($"   image : {i["image"]}");
                Console.WriteLine();
            }
            return 0;
        }

        /// <summary>复现一次点击激活：--activate &lt;AUMID&gt; [launch] [atype]。</summary>
        public int ActivateCli(string[] rest)
        {
            try { Console.OutputEncoding = Encoding.UTF8; } catch { }
            var aumid = rest.Length >= 1 ? rest[0] : "";
            if (aumid.Length == 0)
            {
                Console.Error.WriteLine("usage: topisland-winbridge notify --activate <AUMID> [launchArgs] [activationType]");
                return 2;
            }
            var launch = rest.Length >= 2 ? rest[1] : "";
            var atype = rest.Length >= 3 ? rest[2] : "";
            Console.WriteLine("activate method: " + ToastActivation.Run(aumid, launch, atype));
            return 0;
        }
    }
}
