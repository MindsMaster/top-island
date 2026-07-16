using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;

namespace TopIsland.WinBridge
{
    /// <summary>向 stdout 推一条事件行 {"event":..,"data":..}（无 id）。线程安全，可跨线程调用。</summary>
    internal delegate void PushEvent(string evt, object data);

    /// <summary>处理一条命令。resp 预填 { id, ok:true }，自行放 data 或改写 ok/error；抛异常统一转 error。</summary>
    internal delegate void CommandHandler(string cmd, Dictionary<string, object> req, Dictionary<string, object> resp);

    /// <summary>
    /// Top Island Windows 桥（C# / .NET Framework 4.8，系统自带运行时，零依赖单 exe）。
    /// 常驻进程：stdin 每行一条 JSON 命令 -> stdout 一条带 id 的 JSON 响应。发送 watch 后，
    /// 状态变化经 stdout 推事件行 {"event":"state"|"toasts","data":{...}}（无 id），轮询命令仅兜底。
    ///
    ///   topisland-winbridge.exe smtc     — SMTC 媒体会话查询/控制（协议见 SmtcService）
    ///   topisland-winbridge.exe notify   — 通知托管（协议见 NotifyService）
    ///   topisland-winbridge.exe clip     — 剪贴板变化监听（协议见 ClipService）
    ///
    /// 独立校验模式（不进 stdin 循环）：
    ///   topisland-winbridge.exe smtc --probe                                当前会话 + 主音量
    ///   topisland-winbridge.exe notify --probe                              通知中心现存 toast
    ///   topisland-winbridge.exe notify --activate &lt;AUMID&gt; [launch] [atype]  复现一次点击
    /// </summary>
    internal static class Program
    {
        [MTAThread]
        private static int Main(string[] args)
        {
            var mode = args.Length > 0 ? args[0].ToLowerInvariant() : "";
            var probe = args.Contains("--probe");
            var activateAt = Array.IndexOf(args, "--activate");

            switch (mode)
            {
                case "smtc":
                {
                    if (probe) return new SmtcService(null).Probe();
                    return RunLoop(push => new SmtcService(push).Handle);
                }
                case "notify":
                {
                    if (probe) return new NotifyService(null).Probe();
                    if (activateAt >= 0)
                        return new NotifyService(null).ActivateCli(args.Skip(activateAt + 1).ToArray());
                    return RunLoop(push => new NotifyService(push).Handle);
                }
                case "clip":
                {
                    return RunLoop(push => new ClipService(push).Handle);
                }
                default:
                    Console.Error.WriteLine(
                        "usage: topisland-winbridge <smtc|notify|clip> [--probe] [--activate <AUMID> [launch] [atype]]");
                    return 2;
            }
        }

        /// <summary>
        /// stdin/stdout JSON 行循环。坏行只跳过不退出，stdin 关闭即退出。命令响应来自主循环线程、
        /// 事件行来自 WinRT/定时器线程，同一条 stdout 必须串行化（行内不能交错），写出走同一把锁。
        /// </summary>
        private static int RunLoop(Func<PushEvent, CommandHandler> createService)
        {
            // 显式 UTF-8（无 BOM）：对端是 Node 管道，按 '\n' 分行 JSON.parse
            var input = new StreamReader(Console.OpenStandardInput(), new UTF8Encoding(false));
            var output = new StreamWriter(Console.OpenStandardOutput(), new UTF8Encoding(false))
            {
                AutoFlush = true,
                NewLine = "\n",
            };
            var writeLock = new object();

            void WriteLine(object payload)
            {
                var text = Json.Stringify(payload);
                lock (writeLock)
                {
                    try { output.WriteLine(text); }
                    catch { /* 管道已断，下一次 ReadLine 会结束循环 */ }
                }
            }

            var handle = createService((evt, data) =>
                WriteLine(new Dictionary<string, object> { ["event"] = evt, ["data"] = data }));

            string line;
            while ((line = input.ReadLine()) != null)
            {
                line = line.Trim();
                if (line.Length == 0) continue;

                Dictionary<string, object> req;
                try { req = Json.ParseObject(line); }
                catch { continue; }
                if (req == null) continue;

                var resp = new Dictionary<string, object>
                {
                    ["id"] = req.TryGetValue("id", out var id) ? id : null,
                    ["ok"] = true,
                };
                try
                {
                    handle(Json.Str(req, "cmd"), req, resp);
                }
                catch (Exception ex)
                {
                    resp["ok"] = false;
                    resp["error"] = ex.Message;
                }
                WriteLine(resp);
            }
            return 0;
        }
    }
}
