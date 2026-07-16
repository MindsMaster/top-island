using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

namespace TopIsland.WinBridge.Interop
{
    internal sealed class NotifRow
    {
        public long Id;
        public long ArrivalMs;
        public string Aumid = "";
        public string DisplayName = "";
        public string IconUri = "";
        public string Payload = "";
    }

    /// <summary>
    /// 读 wpndatabase.db（通知中心存储库），走系统自带 winsqlite3.dll，零依赖。
    /// 只打开自己的临时拷贝（见 NotifyService.CopyDb），刻意用 READWRITE：拷贝出的 -wal 帧
    /// 需写权限才能恢复合并，READONLY 会打不开或看不到新数据。
    /// </summary>
    internal static class WpnDatabase
    {
        private const int SQLITE_OK = 0;
        private const int SQLITE_ROW = 100;
        private const int SQLITE_OPEN_READWRITE = 0x00000002;

        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_open_v2(byte[] filename, out IntPtr db, int flags, IntPtr vfs);

        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_prepare_v2(IntPtr db, byte[] sql, int nByte, out IntPtr stmt, out IntPtr tail);

        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_bind_int64(IntPtr stmt, int idx, long val);

        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_step(IntPtr stmt);

        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern long sqlite3_column_int64(IntPtr stmt, int col);

        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr sqlite3_column_text16(IntPtr stmt, int col);

        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_column_bytes16(IntPtr stmt, int col);

        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr sqlite3_column_blob(IntPtr stmt, int col);

        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_column_bytes(IntPtr stmt, int col);

        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_finalize(IntPtr stmt);

        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_close(IntPtr db);

        private const string ToastSql =
            "SELECT n.Id, n.ArrivalTime, h.PrimaryId, " +
            "(SELECT AssetValue FROM HandlerAssets WHERE HandlerId=n.HandlerId AND AssetKey='DisplayName'), " +
            "(SELECT AssetValue FROM HandlerAssets WHERE HandlerId=n.HandlerId AND AssetKey='IconUri'), " +
            "n.Payload " +
            "FROM Notification n JOIN NotificationHandler h ON n.HandlerId=h.RecordId " +
            "WHERE n.Type='toast' AND n.Id>?1 ORDER BY n.Id ASC";

        private const string MaxIdSql =
            "SELECT IFNULL(MAX(Id),0) FROM Notification WHERE Type='toast'";

        /// <summary>Id 大于 sinceId 的全部 toast，按 Id 升序。打不开/查不动一律返回空表。</summary>
        public static List<NotifRow> QueryToasts(string dbPath, long sinceId)
        {
            var list = new List<NotifRow>();
            if (sqlite3_open_v2(Utf8z(dbPath), out var db, SQLITE_OPEN_READWRITE, IntPtr.Zero) != SQLITE_OK)
            {
                if (db != IntPtr.Zero) sqlite3_close(db);
                return list;
            }
            try
            {
                if (sqlite3_prepare_v2(db, Utf8z(ToastSql), -1, out var stmt, out _) != SQLITE_OK) return list;
                try
                {
                    sqlite3_bind_int64(stmt, 1, sinceId);
                    while (sqlite3_step(stmt) == SQLITE_ROW)
                    {
                        list.Add(new NotifRow
                        {
                            Id = sqlite3_column_int64(stmt, 0),
                            ArrivalMs = FileTimeToUnixMs(sqlite3_column_int64(stmt, 1)),
                            Aumid = ColText(stmt, 2),
                            DisplayName = ColText(stmt, 3),
                            IconUri = ColText(stmt, 4),
                            Payload = ColPayload(stmt, 5),
                        });
                    }
                }
                finally { sqlite3_finalize(stmt); }
            }
            finally { sqlite3_close(db); }
            return list;
        }

        /// <summary>当前最大 toast Id（水位基线）。异常路径返回 0。</summary>
        public static long QueryMaxId(string dbPath)
        {
            if (sqlite3_open_v2(Utf8z(dbPath), out var db, SQLITE_OPEN_READWRITE, IntPtr.Zero) != SQLITE_OK)
            {
                if (db != IntPtr.Zero) sqlite3_close(db);
                return 0;
            }
            try
            {
                if (sqlite3_prepare_v2(db, Utf8z(MaxIdSql), -1, out var stmt, out _) != SQLITE_OK) return 0;
                try
                {
                    return sqlite3_step(stmt) == SQLITE_ROW ? sqlite3_column_int64(stmt, 0) : 0;
                }
                finally { sqlite3_finalize(stmt); }
            }
            finally { sqlite3_close(db); }
        }

        /// <summary>NUL 结尾的 UTF-8 字节串（sqlite3 的字符串参数约定）。</summary>
        private static byte[] Utf8z(string s)
        {
            var bytes = Encoding.UTF8.GetBytes(s);
            var z = new byte[bytes.Length + 1];
            Array.Copy(bytes, z, bytes.Length);
            return z;
        }

        /// <summary>文本列走 text16（SQLite 内部转码），避免手工解码。bytes16 须在 text16 之后调。</summary>
        private static string ColText(IntPtr stmt, int col)
        {
            var p = sqlite3_column_text16(stmt, col);
            if (p == IntPtr.Zero) return "";
            return Marshal.PtrToStringUni(p, sqlite3_column_bytes16(stmt, col) / 2) ?? "";
        }

        /// <summary>Payload 是 BLOB：可能 UTF-8 或 UTF-16LE（BOM 或次字节为 NUL 的启发式）。</summary>
        private static string ColPayload(IntPtr stmt, int col)
        {
            int n = sqlite3_column_bytes(stmt, col);
            if (n <= 0) return "";
            var p = sqlite3_column_blob(stmt, col);
            if (p == IntPtr.Zero) return "";
            var buf = new byte[n];
            Marshal.Copy(p, buf, 0, n);
            if (n >= 2 && buf[0] == 0xFF && buf[1] == 0xFE) return Encoding.Unicode.GetString(buf, 2, n - 2);
            if (n >= 2 && buf[1] == 0x00) return Encoding.Unicode.GetString(buf);
            int offset = n >= 3 && buf[0] == 0xEF && buf[1] == 0xBB && buf[2] == 0xBF ? 3 : 0;
            return Encoding.UTF8.GetString(buf, offset, n - offset);
        }

        /// <summary>FILETIME（1601 起 100ns）→ Unix 毫秒；非法值返回 0。</summary>
        private static long FileTimeToUnixMs(long fileTime)
        {
            if (fileTime <= 0) return 0;
            try { return DateTimeOffset.FromFileTime(fileTime).ToUnixTimeMilliseconds(); }
            catch { return 0; }
        }
    }
}
