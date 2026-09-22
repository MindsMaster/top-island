use rusqlite::{Connection, OpenFlags};

use crate::error::{Result, WinError};

/// Payload 为 UTF-16LE 或 UTF-8 判别沿用 WpnDatabase.cs
fn decode_payload(blob: &[u8]) -> String {
    if blob.len() >= 2 && blob[0] == 0xFF && blob[1] == 0xFE {
        return utf16(&blob[2..]);
    }
    if blob.len() >= 2 && blob[1] == 0x00 {
        return utf16(blob);
    }
    let skip = if blob.len() >= 3 && blob[..3] == [0xEF, 0xBB, 0xBF] {
        3
    } else {
        0
    };
    String::from_utf8_lossy(&blob[skip..]).into_owned()
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// 刻意 READWRITE 恢复 -wal 帧需写权限
fn copy_db() -> Result<std::path::PathBuf> {
    let local =
        std::env::var("LOCALAPPDATA").map_err(|e| WinError::api("LOCALAPPDATA 环境变量", e))?;
    let src = std::path::Path::new(&local).join(r"Microsoft\Windows\Notifications\wpndatabase.db");
    let dir = std::env::temp_dir().join("top-island-wpn");
    std::fs::create_dir_all(&dir).map_err(|e| WinError::io("创建通知库临时目录", e))?;
    for suffix in ["", "-wal", "-shm"] {
        let from = src.with_file_name(format!("wpndatabase.db{suffix}"));
        if from.exists() {
            std::fs::copy(&from, dir.join(format!("wpndatabase.db{suffix}")))
                .map_err(|e| WinError::io("拷贝通知库", e))?;
        }
    }
    Ok(dir.join("wpndatabase.db"))
}

/// 轮询先比指纹 没变跳过拷贝
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbFingerprint {
    db: Option<(std::time::SystemTime, u64)>,
    wal: Option<(std::time::SystemTime, u64)>,
}

pub fn source_fingerprint() -> DbFingerprint {
    let stamp = |suffix: &str| -> Option<(std::time::SystemTime, u64)> {
        let local = std::env::var("LOCALAPPDATA").ok()?;
        let path = std::path::Path::new(&local)
            .join(r"Microsoft\Windows\Notifications")
            .join(format!("wpndatabase.db{suffix}"));
        let meta = std::fs::metadata(path).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    };
    DbFingerprint {
        db: stamp(""),
        wal: stamp("-wal"),
    }
}

fn filetime_to_unix_ms(filetime: i64) -> i64 {
    // FILETIME 1601 起 100ns <=0 按 0 同 WpnDatabase.cs
    if filetime <= 0 {
        return 0;
    }
    (filetime - 116_444_736_000_000_000) / 10_000
}

#[derive(Debug, Clone)]
pub struct WpnToast {
    pub id: i64,
    pub arrival_ms: i64,
    pub aumid: String,
    pub display_name: String,
    pub icon_uri: String,
    pub payload: String,
}

/// 水位必须按 raw_max 推进 其含上层过滤掉的空壳
pub fn toasts_since(since_id: i64) -> Result<(Vec<WpnToast>, i64)> {
    let path = copy_db()?;
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|e| WinError::sqlite("打开通知库", e))?;
    let mut stmt = conn
        .prepare(
            "SELECT n.Id, n.ArrivalTime, h.PrimaryId, \
             (SELECT AssetValue FROM HandlerAssets WHERE HandlerId=n.HandlerId AND AssetKey='DisplayName'), \
             (SELECT AssetValue FROM HandlerAssets WHERE HandlerId=n.HandlerId AND AssetKey='IconUri'), \
             n.Payload \
             FROM Notification n JOIN NotificationHandler h ON n.HandlerId=h.RecordId \
             WHERE n.Type='toast' AND n.Id>?1 ORDER BY n.Id ASC",
        )
        .map_err(|e| WinError::sqlite("查询通知", e))?;
    let rows = stmt
        .query_map([since_id], |row| {
            let payload: Vec<u8> = row.get::<_, Option<Vec<u8>>>(5)?.unwrap_or_default();
            Ok(WpnToast {
                id: row.get(0)?,
                arrival_ms: filetime_to_unix_ms(row.get::<_, i64>(1)?),
                aumid: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                display_name: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                icon_uri: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                payload: decode_payload(&payload),
            })
        })
        .map_err(|e| WinError::sqlite("查询通知", e))?;
    let mut items = Vec::new();
    let mut raw_max = since_id;
    for row in rows {
        let row = row.map_err(|e| WinError::sqlite("读取通知", e))?;
        if row.id > raw_max {
            raw_max = row.id;
        }
        items.push(row);
    }
    Ok((items, raw_max))
}

/// 水位基线 历史通知不弹
pub fn query_max_id() -> Result<i64> {
    let path = copy_db()?;
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|e| WinError::sqlite("打开通知库", e))?;
    conn.query_row(
        "SELECT IFNULL(MAX(Id),0) FROM Notification WHERE Type='toast'",
        [],
        |row| row.get(0),
    )
    .map_err(|e| WinError::sqlite("查询通知水位", e))
}
