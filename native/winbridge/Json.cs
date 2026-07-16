using System;
using System.Collections.Generic;
using System.Globalization;
using System.Web.Script.Serialization;

namespace TopIsland.WinBridge
{
    /// <summary>
    /// JSON 行协议序列化。用系统自带的 JavaScriptSerializer（System.Web.Extensions）免第三方
    /// 依赖；MaxJsonLength 默认 2MB，封面 base64 会超，放开到上限。实例非线程安全而推送来自
    /// WinRT/定时器线程，故进出统一加锁（吞吐极低，无争用代价）。
    /// </summary>
    internal static class Json
    {
        private static readonly object Gate = new object();
        private static readonly JavaScriptSerializer Serializer =
            new JavaScriptSerializer { MaxJsonLength = int.MaxValue };

        /// <summary>解析一行 JSON 对象；非对象/解析失败返回 null。</summary>
        public static Dictionary<string, object> ParseObject(string text)
        {
            lock (Gate) return Serializer.DeserializeObject(text) as Dictionary<string, object>;
        }

        public static string Stringify(object value)
        {
            lock (Gate) return Serializer.Serialize(value);
        }

        public static string Str(Dictionary<string, object> obj, string key)
        {
            return obj.TryGetValue(key, out var v) && v != null
                ? Convert.ToString(v, CultureInfo.InvariantCulture)
                : "";
        }

        public static long Int(Dictionary<string, object> obj, string key, long fallback)
        {
            if (!obj.TryGetValue(key, out var v) || v == null) return fallback;
            try { return Convert.ToInt64(v, CultureInfo.InvariantCulture); }
            catch { return fallback; }
        }
    }
}
