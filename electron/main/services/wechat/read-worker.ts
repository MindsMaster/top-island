import * as fs from 'fs';
import { parentPort, workerData } from 'worker_threads';
import { CIPHER_PAGE_SIZE, getKeyMaterial } from './decrypt';
import {
  decodeContent,
  md5Hex,
  placeholderFor,
  stripSenderPrefix,
  type Contacts,
  type ContactInfo,
  type RawMessage,
} from './read-shared';

type NativeDatabase = {
  pragma(s: string): unknown;
  prepare(sql: string): { all(...a: unknown[]): Record<string, unknown>[] };
  close(): void;
};
type NativeCtor = new (
  file: string,
  opts?: { readonly?: boolean; fileMustExist?: boolean; nativeBinding?: string }
) => NativeDatabase;

// eslint-disable-next-line @typescript-eslint/no-require-imports
const NativeDb = require('better-sqlite3-multiple-ciphers') as NativeCtor;
// 原生 .node 绝对路径（主进程算好传入），直接 require 绕过 `bindings` 探测（打包必需）
const NATIVE_BINDING = (workerData as { nativeBinding?: string } | undefined)?.nativeBinding;

/** 原生打开微信 SQLCipher 库（只读）。失败抛错，由上层重试。 */
function openNative(keyHex: string, file: string): NativeDatabase {
  const fd = fs.openSync(file, 'r');
  const page1 = Buffer.allocUnsafe(CIPHER_PAGE_SIZE);
  try {
    fs.readSync(fd, page1, 0, CIPHER_PAGE_SIZE, 0);
  } finally {
    fs.closeSync(fd);
  }
  const km = getKeyMaterial(Buffer.from(keyHex, 'hex'), page1);
  if (!km) throw new Error('key material 解析失败');
  const db = new NativeDb(file, { readonly: true, fileMustExist: true, nativeBinding: NATIVE_BINDING });
  db.pragma("cipher='sqlcipher'");
  db.pragma('legacy=4'); // sqlite3mc 的 SQLCipher 4 兼容
  db.pragma(`key="x'${km.encKey.toString('hex')}'"`); // 解析出的真实 enc_key 作原始密钥
  db.prepare('SELECT name FROM sqlite_master LIMIT 1').all();
  return db;
}

function tablesOf(db: NativeDatabase): string[] {
  try {
    return db
      .prepare("SELECT name FROM sqlite_master WHERE type='table'")
      .all()
      .map((r) => String(r.name));
  } catch {
    return [];
  }
}

function loadContactsFrom(db: NativeDatabase): Contacts {
  const byUsername = new Map<string, ContactInfo>();
  const byHash = new Map<string, ContactInfo>();
  const t = tablesOf(db).find((n) => /^contact$/i.test(n)) || tablesOf(db).find((n) => /^contact/i.test(n));
  if (t) {
    for (const r of db.prepare(`SELECT * FROM "${t}"`).all()) applyContact(byUsername, byHash, r);
  }
  return { byUsername, byHash };
}

function readMessagesFrom(db: NativeDatabase, sinceTime: number, myWxid: string): RawMessage[] {
  const result: RawMessage[] = [];
  const id2name = new Map<number, string>();
  for (const r of db.prepare('SELECT rowid, user_name FROM Name2Id').all()) {
    id2name.set(Number(r.rowid), String(r.user_name ?? ''));
  }
  const since = Math.floor(sinceTime);
  for (const t of tablesOf(db).filter((n) => /^Msg_[0-9a-f]{32}$/i.test(n))) {
    let rows: Record<string, unknown>[];
    try {
      rows = db
        .prepare(
          `SELECT local_id, local_type, real_sender_id, create_time, message_content ` +
            `FROM "${t}" WHERE create_time > ? ORDER BY create_time ASC, local_id ASC LIMIT 50`
        )
        .all(since);
    } catch {
      continue;
    }
    for (const r of rows) pushMessage(result, t, r, id2name, myWxid);
  }
  return sortMsgs(result);
}

function applyContact(
  byUsername: Map<string, ContactInfo>,
  byHash: Map<string, ContactInfo>,
  r: Record<string, unknown>
): void {
  const username = String(r.username ?? r.user_name ?? '');
  if (!username) return;
  const name = String(r.remark ?? '') || String(r.nick_name ?? r.nickname ?? '') || username;
  const avatar = String(r.small_head_url ?? r.big_head_url ?? '');
  const isGroup = /@chatroom$/i.test(username);
  const muted = isGroup ? Number(r.chat_room_notify ?? 1) === 0 : (Number(r.flag ?? 0) & 0x200) !== 0;
  const info: ContactInfo = { username, name, avatar, isGroup, muted };
  byUsername.set(username, info);
  byHash.set(md5Hex(username), info);
}
function pushMessage(
  out: RawMessage[],
  table: string,
  r: Record<string, unknown>,
  id2name: Map<number, string>,
  myWxid: string
): void {
  const senderUsername = id2name.get(Number(r.real_sender_id)) || '';
  const isSend = senderUsername !== '' && senderUsername === myWxid;
  if (isSend) return;
  const localType = Number(r.local_type);
  const placeholder = placeholderFor(localType);
  const content = placeholder ?? stripSenderPrefix(decodeContent(r.message_content));
  out.push({
    table,
    localId: Number(r.local_id),
    createTime: Number(r.create_time),
    localType,
    senderUsername,
    content: content.slice(0, 200),
    isSend,
  });
}
function sortMsgs(a: RawMessage[]): RawMessage[] {
  a.sort((x, y) => x.createTime - y.createTime || x.localId - y.localId);
  return a;
}

interface Job {
  id: number;
  kind: 'contacts' | 'messages';
  keyHex: string;
  file: string;
  sinceTime?: number;
  myWxid?: string;
}

function handle(job: Job): Contacts | RawMessage[] {
  const db = openNative(job.keyHex, job.file);
  try {
    return job.kind === 'contacts'
      ? loadContactsFrom(db)
      : readMessagesFrom(db, job.sinceTime ?? 0, job.myWxid ?? '');
  } finally {
    try {
      db.close();
    } catch {
      /* ignore */
    }
  }
}

parentPort?.on('message', (job: Job) => {
  try {
    const result = handle(job);
    parentPort?.postMessage({ id: job.id, ok: true, result });
  } catch (e) {
    parentPort?.postMessage({ id: job.id, ok: false, error: e instanceof Error ? e.message : String(e) });
  }
});
