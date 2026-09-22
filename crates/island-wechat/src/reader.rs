use std::collections::HashMap;
use std::path::Path;
use std::ptr::NonNull;

use md5::Digest as _;
use rusqlite::serialize::OwnedData;
use rusqlite::{Connection, DatabaseName};

use crate::decrypt::{self, KEY_SIZE};
use crate::error::{Result, WeChatError};

const BATCH_SIZE: i64 = 50;
const CONTENT_MAX_CHARS: usize = 200;

#[derive(Debug, Clone, Default)]
pub struct ContactInfo {
    pub username: String,
    pub name: String,
    /// small_head_url 渲染层经 notifyImage 抓取
    pub avatar: String,
    pub is_group: bool,
    pub muted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Contacts {
    pub by_username: HashMap<String, ContactInfo>,
    /// md5(username) 反解 Msg_<md5> 表名
    pub by_hash: HashMap<String, ContactInfo>,
}

#[derive(Debug, Clone)]
pub struct RawMessage {
    pub table: String,
    pub local_id: i64,
    pub create_time: i64,
    pub local_type: i64,
    pub sender_username: String,
    pub content: String,
}

pub fn md5_hex(s: &str) -> String {
    hex::encode(md5::Md5::digest(s.as_bytes()))
}

/// 群聊正文前缀 wxid_xxx:\n
pub fn strip_sender_prefix(content: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"^(wxid_[0-9a-zA-Z_-]+|gh_[0-9a-zA-Z_-]+|[0-9]+@chatroom):\n?")
            .expect("发送人前缀正则是常量")
    });
    re.replace(content, "").into_owned()
}

/// local_type 子类型<<32|基类型 49 按子类型分载体
pub fn placeholder_for(local_type: i64) -> Option<&'static str> {
    match local_type {
        1 => None,                          // 文本 用真实内容
        3 => Some("[图片]"),                // 基类型 3 图片
        34 => Some("[语音]"),               // 基类型 34 语音
        43 => Some("[视频]"),               // 基类型 43 视频
        47 => Some("[动画表情]"),           // 基类型 47 emoji 贴图
        42 => Some("[名片]"),               // 基类型 42 名片
        48 => Some("[位置]"),               // 基类型 48 位置共享
        49 => Some("[链接/文件]"),          // 基类型 49 无子类型 泛链接文件
        50 => Some("[通话]"),               // 基类型 50 音视频通话
        10000 => Some("[系统消息]"),        // 基类型 10000 撤回入群等提示
        244813135921 => Some("[引用消息]"), // 57<<32|49 引用回复
        17179869233 => Some("[链接]"),      // 4<<32|49 网页链接
        21474836529 => Some("[文章]"),      // 5<<32|49 公众号文章
        154618822705 => Some("[小程序]"),   // 36<<32|49 小程序卡片
        12884901937 => Some("[音乐]"),      // 3<<32|49 音乐分享
        8594229559345 => Some("[红包]"),    // 2001<<32|49 红包
        81604378673 => Some("[聊天记录]"),  // 19<<32|49 合并转发聊天记录
        266287972401 => Some("[拍一拍]"),   // 62<<32|49 拍一拍
        8589934592049 => Some("[转账]"),    // 2000<<32|49 转账
        270582939697 => Some("[直播]"),     // 63<<32|49 直播卡片
        25769803825 => Some("[文件]"),      // 6<<32|49 文件
        _ => None,
    }
}

pub fn body_for(local_type: i64, decoded_content: &str) -> String {
    if let Some(p) = placeholder_for(local_type) {
        return p.to_string();
    }
    let content = strip_sender_prefix(decoded_content);
    if local_type == 1 {
        return content;
    }
    if content.is_empty() {
        "[消息]".to_string()
    } else {
        content
    }
}

/// message_content BLOB 或为 zstd 魔数 28 B5 2F FD
pub fn decode_blob(bytes: &[u8]) -> Result<String> {
    const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
    if bytes.len() >= 4 && bytes[..4] == ZSTD_MAGIC {
        let raw = zstd::decode_all(bytes).map_err(|e| WeChatError::api("zstd 解压消息正文", e))?;
        return Ok(String::from_utf8_lossy(&raw).into_owned());
    }
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

/// OwnedData 无 from_vec 须自管 sqlite3_malloc 明文不落盘
fn connection_from_bytes(data: &mut [u8]) -> Result<Connection> {
    // memdb 建不起 wal-index 读 WAL 镜像必报 CANTOPEN 文件版本字节 18/19 改 2→1
    if data.len() >= 20 && data[18] == 2 && data[19] == 2 {
        data[18] = 1;
        data[19] = 1;
    }
    let mut conn =
        Connection::open_in_memory().map_err(|e| WeChatError::sqlite("打开内存库", e))?;
    let raw = unsafe { rusqlite::ffi::sqlite3_malloc(data.len().try_into().unwrap_or(i32::MAX)) };
    let raw = NonNull::new(raw.cast::<u8>()).ok_or_else(|| {
        WeChatError::api(
            "sqlite3_malloc",
            format_args!("分配 {} 字节失败", data.len()),
        )
    })?;
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr(), raw.as_ptr(), data.len());
        let owned = OwnedData::from_raw_nonnull(raw, data.len());
        conn.deserialize(DatabaseName::Main, owned, true)
            .map_err(|e| WeChatError::sqlite("挂载解密库", e))?;
    }
    Ok(conn)
}

pub fn open_decrypted(key: &[u8; KEY_SIZE], db_path: &Path) -> Result<Connection> {
    let file = std::fs::read(db_path).map_err(|e| WeChatError::io("读取消息库", e))?;
    let wal_path = wal_path_of(db_path);
    let wal = std::fs::read(&wal_path).ok();
    if wal.is_none() && wal_path.exists() {
        eprintln!("[wechat] 读取 WAL 失败: {}", wal_path.display());
    }
    let mut plain = decrypt::decrypt_database_with_wal(key, &file, wal.as_deref())
        .ok_or(WeChatError::InvalidKey)?;
    connection_from_bytes(&mut plain)
}

fn wal_path_of(db_path: &Path) -> std::path::PathBuf {
    let mut s = db_path.as_os_str().to_os_string();
    s.push("-wal");
    std::path::PathBuf::from(s)
}

fn tables_of(conn: &Connection) -> Vec<String> {
    let mut stmt = match conn.prepare("SELECT name FROM sqlite_master WHERE type='table'") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[wechat] 读 sqlite_master 失败: {e}");
            return vec![];
        }
    };
    let rows = stmt.query_map([], |r| r.get::<_, String>(0));
    match rows {
        Ok(it) => {
            let mut out = vec![];
            for r in it {
                match r {
                    Ok(name) => out.push(name),
                    Err(e) => {
                        eprintln!("[wechat] 读取表名行失败: {e}");
                        break;
                    }
                }
            }
            out
        }
        Err(e) => {
            eprintln!("[wechat] 枚举表名失败: {e}");
            vec![]
        }
    }
}

fn is_msg_table(name: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?i)^Msg_[0-9a-f]{32}$").expect("消息表名正则是常量"))
        .is_match(name)
}

fn apply_contact(
    by_username: &mut HashMap<String, ContactInfo>,
    by_hash: &mut HashMap<String, ContactInfo>,
    cols: &rusqlite::Row,
) {
    let get = |name: &str| -> String {
        cols.get_ref(name)
            .ok()
            .and_then(|v| v.as_str().ok().map(|s| s.to_string()))
            .unwrap_or_default()
    };
    let get_num = |name: &str| -> Option<i64> {
        cols.get_ref(name).ok().and_then(|v| match v {
            rusqlite::types::ValueRef::Integer(i) => Some(i),
            rusqlite::types::ValueRef::Real(f) => Some(f as i64),
            rusqlite::types::ValueRef::Text(t) => std::str::from_utf8(t).ok()?.trim().parse().ok(),
            _ => None,
        })
    };
    let username = {
        let u = get("username");
        if u.is_empty() {
            get("user_name")
        } else {
            u
        }
    };
    if username.is_empty() {
        return;
    }
    let name = {
        let remark = get("remark");
        let nick = {
            let n = get("nick_name");
            if n.is_empty() {
                get("nickname")
            } else {
                n
            }
        };
        if !remark.is_empty() {
            remark
        } else if !nick.is_empty() {
            nick
        } else {
            username.clone()
        }
    };
    let avatar = {
        let small = get("small_head_url");
        if small.is_empty() {
            get("big_head_url")
        } else {
            small
        }
    };
    let is_group = username.to_lowercase().ends_with("@chatroom");
    // 群 chat_room_notify=0 免打扰 NULL 不算 个人 flag 0x200 位
    let muted = if is_group {
        get_num("chat_room_notify").unwrap_or(1) == 0
    } else {
        get_num("flag").unwrap_or(0) & 0x200 != 0
    };
    let info = ContactInfo {
        username: username.clone(),
        name,
        avatar,
        is_group,
        muted,
    };
    by_username.insert(username.clone(), info.clone());
    by_hash.insert(md5_hex(&username), info);
}

fn load_contacts_from(conn: &Connection) -> Contacts {
    let mut by_username = HashMap::new();
    let mut by_hash = HashMap::new();
    let tables = tables_of(conn);
    let table = tables
        .iter()
        .find(|n| n.eq_ignore_ascii_case("contact"))
        .or_else(|| {
            tables
                .iter()
                .find(|n| n.to_lowercase().starts_with("contact"))
        });
    let Some(table) = table else {
        return Contacts::default();
    };
    let sql = format!("SELECT * FROM \"{table}\"");
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[wechat] 打开联系人表失败: {e}");
            return Contacts::default();
        }
    };
    let rows = match stmt.query([]) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[wechat] 读联系人表失败: {e}");
            return Contacts::default();
        }
    };
    let mut rows = rows;
    loop {
        match rows.next() {
            Ok(Some(row)) => apply_contact(&mut by_username, &mut by_hash, row),
            Ok(None) => break,
            Err(e) => {
                eprintln!("[wechat] 读联系人行失败: {e}");
                break;
            }
        }
    }
    Contacts {
        by_username,
        by_hash,
    }
}

/// real_sender_id 反解 username 的表 可缺
fn load_name2id(conn: &Connection) -> HashMap<i64, String> {
    let mut map = HashMap::new();
    let mut stmt = match conn.prepare("SELECT rowid, user_name FROM Name2Id") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[wechat] Name2Id 不可用: {e}");
            return map;
        }
    };
    let rows = match stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[wechat] 读 Name2Id 失败: {e}");
            return map;
        }
    };
    for r in rows {
        match r {
            Ok((id, name)) => {
                map.insert(id, name);
            }
            Err(e) => {
                eprintln!("[wechat] 读取 Name2Id 行失败: {e}");
                break;
            }
        }
    }
    map
}

fn push_message(
    out: &mut Vec<RawMessage>,
    table: &str,
    local_id: i64,
    local_type: i64,
    create_time: i64,
    real_sender_id: i64,
    content_value: &rusqlite::types::Value,
    id2name: &HashMap<i64, String>,
    my_wxid: &str,
) {
    let sender_username = id2name.get(&real_sender_id).cloned().unwrap_or_default();
    if !sender_username.is_empty() && sender_username == my_wxid {
        return;
    }
    let decoded = match content_value {
        rusqlite::types::Value::Null => String::new(),
        rusqlite::types::Value::Text(t) => t.clone(),
        rusqlite::types::Value::Blob(b) => match decode_blob(b) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[wechat] 正文解码失败(local_id={local_id}): {e}");
                String::new()
            }
        },
        _ => String::new(),
    };
    let body = body_for(local_type, &decoded);
    let content: String = body.chars().take(CONTENT_MAX_CHARS).collect();
    out.push(RawMessage {
        table: table.to_string(),
        local_id,
        create_time,
        local_type,
        sender_username,
        content,
    });
}

/// 首批严格 create_time>水位 轮内同秒靠 local_id 接力
fn read_table_messages(
    conn: &Connection,
    table: &str,
    since_time: i64,
    id2name: &HashMap<i64, String>,
    my_wxid: &str,
    out: &mut Vec<RawMessage>,
) {
    let sql_first = format!(
        "SELECT local_id, local_type, real_sender_id, create_time, message_content \
         FROM \"{table}\" WHERE create_time > ?1 \
         ORDER BY create_time ASC, local_id ASC LIMIT {BATCH_SIZE}"
    );
    let sql_next = format!(
        "SELECT local_id, local_type, real_sender_id, create_time, message_content \
         FROM \"{table}\" \
         WHERE create_time > ?1 OR (create_time = ?1 AND local_id > ?2) \
         ORDER BY create_time ASC, local_id ASC LIMIT {BATCH_SIZE}"
    );
    let mut cursor_time = since_time;
    let mut cursor_id: Option<i64> = None;
    loop {
        let (sql, params): (&str, Vec<i64>) = match cursor_id {
            None => (&sql_first, vec![cursor_time]),
            Some(id) => (&sql_next, vec![cursor_time, id]),
        };
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[wechat] 准备消息查询失败({table}): {e}");
                return;
            }
        };
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                rusqlite::types::Value::from(r.get_ref(4)?),
            ))
        });
        let rows = match rows {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[wechat] 读消息失败({table}): {e}");
                return;
            }
        };
        let mut batch = 0i64;
        for row in rows {
            let (local_id, local_type, real_sender_id, create_time, content) = match row {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("[wechat] 读消息行失败({table}): {e}");
                    break;
                }
            };
            push_message(
                out,
                table,
                local_id,
                local_type,
                create_time,
                real_sender_id,
                &content,
                id2name,
                my_wxid,
            );
            cursor_time = create_time;
            cursor_id = Some(local_id);
            batch += 1;
        }
        if batch < BATCH_SIZE {
            return;
        }
    }
}

fn read_messages_from(conn: &Connection, since_time: i64, my_wxid: &str) -> Vec<RawMessage> {
    let id2name = load_name2id(conn);
    let mut result = vec![];
    for table in tables_of(conn).into_iter().filter(|n| is_msg_table(n)) {
        read_table_messages(conn, &table, since_time, &id2name, my_wxid, &mut result);
    }
    result.sort_by(|a, b| {
        a.create_time
            .cmp(&b.create_time)
            .then(a.local_id.cmp(&b.local_id))
    });
    result
}

pub fn load_contacts(key: &[u8; KEY_SIZE], contact_db: &Path) -> Result<Contacts> {
    let conn = open_decrypted(key, contact_db)?;
    Ok(load_contacts_from(&conn))
}

pub fn read_new_messages(
    key: &[u8; KEY_SIZE],
    message_db: &Path,
    since_time: i64,
    my_wxid: &str,
) -> Result<Vec<RawMessage>> {
    let conn = open_decrypted(key, message_db)?;
    Ok(read_messages_from(&conn, since_time, my_wxid))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_conn() -> Connection {
        Connection::open_in_memory().expect("内存库必须能打开")
    }

    fn make_msg_table(conn: &Connection, table: &str) {
        conn.execute(
            &format!(
                "CREATE TABLE \"{table}\" (\
                 local_id INTEGER PRIMARY KEY, local_type INTEGER, real_sender_id INTEGER, \
                 create_time INTEGER, message_content)"
            ),
            [],
        )
        .expect("测试消息表必须能创建");
    }

    fn insert_msg(
        conn: &Connection,
        table: &str,
        local_id: i64,
        create_time: i64,
        sender_id: i64,
        content: &str,
    ) {
        conn.execute(
            &format!(
                "INSERT INTO \"{table}\" (local_id, local_type, real_sender_id, create_time, message_content) \
                 VALUES (?1, 1, ?2, ?3, ?4)"
            ),
            rusqlite::params![local_id, sender_id, create_time, content],
        )
        .expect("测试消息必须能插入");
    }

    fn insert_name2id(conn: &Connection, id: i64, username: &str) {
        conn.execute(
            "INSERT INTO Name2Id (rowid, user_name) VALUES (?1, ?2)",
            rusqlite::params![id, username],
        )
        .expect("Name2Id 必须能插入");
    }

    #[test]
    fn burst_of_more_than_one_batch_is_fully_drained() {
        let conn = memory_conn();
        conn.execute("CREATE TABLE Name2Id (user_name TEXT)", [])
            .unwrap();
        insert_name2id(&conn, 7, "wxid_friend");
        let table = format!("Msg_{}", md5_hex("wxid_friend"));
        make_msg_table(&conn, &table);
        for i in 1..=120i64 {
            insert_msg(&conn, &table, i, 1000 + i, 7, &format!("msg {i}"));
        }

        let msgs = read_messages_from(&conn, 1000, "wxid_me");
        assert_eq!(msgs.len(), 120);
        assert_eq!(msgs[0].content, "msg 1");
        assert_eq!(msgs[119].content, "msg 120");
    }

    #[test]
    fn same_second_messages_paginate_by_local_id_cursor() {
        let conn = memory_conn();
        conn.execute("CREATE TABLE Name2Id (user_name TEXT)", [])
            .unwrap();
        insert_name2id(&conn, 7, "wxid_friend");
        let table = format!("Msg_{}", md5_hex("wxid_friend"));
        make_msg_table(&conn, &table);
        for i in 1..=70i64 {
            insert_msg(&conn, &table, i, 5000, 7, &format!("m{i}"));
        }

        let msgs = read_messages_from(&conn, 4999, "wxid_me");
        assert_eq!(msgs.len(), 70);
        let ids: Vec<i64> = msgs.iter().map(|m| m.local_id).collect();
        let mut dedup = ids.clone();
        dedup.dedup();
        assert_eq!(ids, dedup);
    }

    #[test]
    fn since_time_excludes_already_reported_messages() {
        let conn = memory_conn();
        conn.execute("CREATE TABLE Name2Id (user_name TEXT)", [])
            .unwrap();
        insert_name2id(&conn, 7, "wxid_friend");
        let table = format!("Msg_{}", md5_hex("wxid_friend"));
        make_msg_table(&conn, &table);
        insert_msg(&conn, &table, 1, 1000, 7, "old");
        insert_msg(&conn, &table, 2, 2000, 7, "new");

        let msgs = read_messages_from(&conn, 1000, "wxid_me");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "new");
    }

    #[test]
    fn own_messages_are_skipped() {
        let conn = memory_conn();
        conn.execute("CREATE TABLE Name2Id (user_name TEXT)", [])
            .unwrap();
        insert_name2id(&conn, 7, "wxid_me");
        insert_name2id(&conn, 8, "wxid_friend");
        let table = format!("Msg_{}", md5_hex("wxid_friend"));
        make_msg_table(&conn, &table);
        insert_msg(&conn, &table, 1, 1000, 7, "我自己发的");
        insert_msg(&conn, &table, 2, 1001, 8, "对方发的");

        let msgs = read_messages_from(&conn, 0, "wxid_me");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sender_username, "wxid_friend");
    }

    #[test]
    fn body_for_unknown_type_keeps_real_content() {
        let got = body_for(987654321, "真正的重要内容");
        assert_eq!(got, "真正的重要内容");
    }

    #[test]
    fn body_for_unknown_type_without_content_falls_back_to_placeholder() {
        assert_eq!(body_for(987654321, ""), "[消息]");
    }

    #[test]
    fn body_for_known_types_uses_placeholder() {
        assert_eq!(body_for(3, "binary image data"), "[图片]");
        assert_eq!(body_for(244813135921, "ignored"), "[引用消息]");
    }

    #[test]
    fn body_for_text_strips_group_sender_prefix() {
        assert_eq!(body_for(1, "wxid_sender:\n实际内容"), "实际内容");
        assert_eq!(body_for(1, "12345@chatroom:内容"), "内容");
        assert_eq!(body_for(1, "普通消息"), "普通消息");
    }

    #[test]
    fn placeholder_table_covers_all_documented_magic_numbers() {
        let cases = [
            (3, "[图片]"),
            (34, "[语音]"),
            (43, "[视频]"),
            (47, "[动画表情]"),
            (42, "[名片]"),
            (48, "[位置]"),
            (49, "[链接/文件]"),
            (50, "[通话]"),
            (10000, "[系统消息]"),
            (244813135921, "[引用消息]"),
            (17179869233, "[链接]"),
            (21474836529, "[文章]"),
            (154618822705, "[小程序]"),
            (12884901937, "[音乐]"),
            (8594229559345, "[红包]"),
            (81604378673, "[聊天记录]"),
            (266287972401, "[拍一拍]"),
            (8589934592049, "[转账]"),
            (270582939697, "[直播]"),
            (25769803825, "[文件]"),
        ];
        for (t, expect) in cases {
            assert_eq!(placeholder_for(t), Some(expect));
        }
        assert_eq!(placeholder_for(1), None);
    }

    #[test]
    fn decode_blob_handles_plain_zstd_and_garbage() {
        assert_eq!(
            decode_blob("明文".as_bytes()).expect("明文 BLOB 必须能解"),
            "明文"
        );

        let compressed = zstd::encode_all("压缩内容".as_bytes(), 3).expect("测试压缩必须成功");
        assert_eq!(
            decode_blob(&compressed).expect("合法 zstd BLOB 必须能解"),
            "压缩内容"
        );

        let mut bad = zstd::encode_all("x".as_bytes(), 3).expect("测试压缩必须成功");
        bad.truncate(6); // 保留魔数 截断主体
        assert!(decode_blob(&bad).is_err());
    }

    #[test]
    fn apply_contact_reads_mute_flags() {
        let conn = memory_conn();
        conn.execute(
            "CREATE TABLE contact (username TEXT, remark TEXT, nick_name TEXT, small_head_url TEXT, \
             chat_room_notify INTEGER, flag INTEGER)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO contact VALUES ('wxid_friend', '', '小明', 'http://a/b.png', 1, 512)", // 0x200
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO contact VALUES ('room@chatroom', '摸鱼群', '', '', 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO contact VALUES ('room2@chatroom', '', '热闹群', '', 1, 0)",
            [],
        )
        .unwrap();

        let contacts = load_contacts_from(&conn);
        let friend = &contacts.by_username["wxid_friend"];
        assert!(friend.muted);
        assert_eq!(friend.name, "小明");
        assert!(contacts.by_username["room@chatroom"].muted);
        assert!(!contacts.by_username["room2@chatroom"].muted);
        assert!(contacts.by_hash.contains_key(&md5_hex("wxid_friend")));
    }

    #[test]
    fn group_with_null_notify_column_is_not_muted() {
        let conn = memory_conn();
        conn.execute(
            "CREATE TABLE contact (username TEXT, remark TEXT, nick_name TEXT, small_head_url TEXT, \
             chat_room_notify INTEGER, flag INTEGER)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO contact VALUES ('room@chatroom', '', '群', '', NULL, 0)",
            [],
        )
        .unwrap();

        let contacts = load_contacts_from(&conn);
        assert!(!contacts.by_username["room@chatroom"].muted);
    }

    #[test]
    fn contact_name_prefers_remark_over_nickname() {
        let conn = memory_conn();
        conn.execute(
            "CREATE TABLE contact (username TEXT, remark TEXT, nick_name TEXT, small_head_url TEXT, \
             chat_room_notify INTEGER, flag INTEGER)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO contact VALUES ('wxid_x', '备注名', '昵称', '', 1, 0)",
            [],
        )
        .unwrap();
        let contacts = load_contacts_from(&conn);
        assert_eq!(contacts.by_username["wxid_x"].name, "备注名");
    }

    #[test]
    fn strip_prefix_also_handles_gh_accounts() {
        assert_eq!(strip_sender_prefix("gh_ab12cd:\n通知"), "通知");
    }

    #[test]
    fn wal_mode_image_is_readable_after_deserialize() {
        // 微信库文件格式为 WAL
        let dir = std::env::temp_dir().join(format!("ti-wal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("m.db");
        {
            let c = Connection::open(&path).expect("临时库必须能建");
            c.execute_batch(
                "PRAGMA journal_mode=WAL; \
                 CREATE TABLE Name2Id (user_name TEXT); \
                 INSERT INTO Name2Id VALUES ('wxid_x');",
            )
            .expect("WAL 库必须能写");
        }
        let mut bytes = std::fs::read(&path).expect("读临时库字节");
        assert_eq!(bytes[18], 2);
        let _ = std::fs::remove_dir_all(&dir);

        let conn = connection_from_bytes(&mut bytes).expect("deserialize 必须成功");
        let tables = tables_of(&conn);
        assert!(tables.iter().any(|t| t == "Name2Id"));
        assert_eq!(
            load_name2id(&conn).get(&1).map(|s| s.as_str()),
            Some("wxid_x")
        );
    }
}
