import { spawn } from 'child_process';
import { app } from 'electron';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { resourcePath } from '../paths';
import { store } from '../store';
import { diagLog } from '../diag';
import { loadContacts, readNewMessages, type Contacts } from './wechat/read';
import type { NotificationItem } from '../../../shared/ipc';

interface WeChatAccount {
  wxid: string; // 纯 wxid（去掉目录后缀）
  dataDir: string; // db_storage 目录
}

interface StoredKey {
  keyHex: string;
  wxid: string;
  dataDir: string;
  recoveredAt: number;
}

// 事件驱动为主（fs.watch 消息目录），轮询作兜底（fs.watch 在个别场景漏事件）
// debounce 只需合并单条消息触发的多文件写（main/-wal/-shm，数毫秒内），取小值降延迟；
// pollOnce 幂等（按 watermark），多触发无害。兜底轮询取 1.5s 防 watch 失效时成批。
const POLL_INTERVAL = 1500;
const WATCH_DEBOUNCE = 100;

let running = false;
let pollTimer: NodeJS.Timeout | null = null;
let watcher: fs.FSWatcher | null = null;
let watchDebounce: NodeJS.Timeout | null = null;
let onItems: ((items: NotificationItem[]) => void) | null = null;
let watermark = 0; // 已上报的最大 create_time（unix 秒）
let lastMtime = 0;
let fileMtimes = new Map<string, number>(); // 每个 message 库上次读取时的 mtime
let contacts: Contacts = { byUsername: new Map(), byHash: new Map() };
let contactLoadedFor = '';
let contactDbMtime = 0; // 上次加载联系人时 contact.db(+wal) 的 mtime；变了则重载（免打扰等实时同步）
let lastReadErrLog = 0;

/** 读库失败节流打日志（原生开库/解密失败时能看到真因） */
function logReadErr(where: string, e: unknown): void {
  const now = Date.now();
  if (now - lastReadErrLog < 5000) return;
  lastReadErrLog = now;
  diagLog(`[wechat] ${where} failed: ${e instanceof Error ? e.message : String(e)}`);
}

// 扫描各盘直接子目录时跳过的系统/无关目录（提速、避噪）
const SCAN_SKIP_NAMES = new Set([
  'windows',
  'program files',
  'program files (x86)',
  'programdata',
  '$recycle.bin',
  'system volume information',
  'recovery',
  'perflogs',
  'msocache',
  'onedrivetemp',
  'appdata',
  'node_modules',
]);

function safeSubdirs(dir: string): string[] {
  try {
    return fs
      .readdirSync(dir, { withFileTypes: true })
      .filter((e) => e.isDirectory())
      .map((e) => path.join(dir, e.name));
  } catch {
    return [];
  }
}

/** 账号目录里 db_storage/message/message_*.db 的最新 mtime（不是有效账号则返回 0） */
function accountNewestMtime(accountDir: string): number {
  const msgDir = path.join(accountDir, 'db_storage', 'message');
  let dbs: string[];
  try {
    dbs = fs.readdirSync(msgDir).filter((f) => /^message_\d+\.db$/i.test(f));
  } catch {
    return 0;
  }
  let m = 0;
  for (const f of dbs) {
    try {
      m = Math.max(m, fs.statSync(path.join(msgDir, f)).mtimeMs);
    } catch {
      /* ignore */
    }
  }
  return m;
}

/**
 * 可能“是”或“包含” xwechat_files 的候选路径（浅层，不做全盘深扫）。
 * 覆盖：Documents\xwechat_files、各盘根\xwechat_files、各盘直接子目录\xwechat_files
 * （如 D:\wechatMSG\xwechat_files 这类自定义数据目录）、其它用户的 Documents。
 */
function xwechatDirCandidates(): string[] {
  const out = new Set<string>();
  const consider = (dir: string): void => {
    if (!dir) return;
    if (path.basename(dir).toLowerCase() === 'xwechat_files') out.add(path.normalize(dir));
    out.add(path.normalize(path.join(dir, 'xwechat_files')));
  };

  let home = '';
  try {
    home = app.getPath('home');
  } catch {
    home = os.homedir();
  }
  let docs = '';
  try {
    docs = app.getPath('documents');
  } catch {
    docs = path.join(home, 'Documents');
  }
  for (const d of [docs, home, path.join(home, 'Documents')]) consider(d);
  const up = (process.env.USERPROFILE || '').trim();
  if (up) for (const d of [up, path.join(up, 'Documents')]) consider(d);

  for (const drive of ['C:', 'D:', 'E:', 'F:', 'G:']) {
    const root = drive + path.sep;
    if (!fs.existsSync(root)) continue;
    consider(root);
    for (const child of safeSubdirs(root)) {
      if (SCAN_SKIP_NAMES.has(path.basename(child).toLowerCase())) continue;
      consider(child);
    }
    for (const user of safeSubdirs(path.join(root, 'Users'))) {
      consider(path.join(user, 'Documents'));
    }
  }
  return [...out];
}

/**
 * 发现最新的微信 4.x 账号。判据是账号目录下确有 db_storage/message/message_*.db
 * （名字不必是 wxid_ 开头——微信也允许自定义号）。先看标准 Documents\xwechat_files，
 * 再浅层多盘扫描，兼容用户把数据目录改到别的盘/自定义文件夹的情况。
 */
export function discoverAccount(): WeChatAccount | null {
  let best: WeChatAccount | null = null;
  let bestTime = 0;
  const seen = new Set<string>();
  for (const xw of xwechatDirCandidates()) {
    const key = xw.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    for (const accountDir of safeSubdirs(xw)) {
      const name = path.basename(accountDir);
      if (/^all_users$/i.test(name)) continue; // 非账号目录
      const mtime = accountNewestMtime(accountDir);
      if (mtime > bestTime) {
        bestTime = mtime;
        best = {
          wxid: name.replace(/_[0-9a-f]{4}$/i, ''),
          dataDir: path.join(accountDir, 'db_storage'),
        };
      }
    }
  }
  return best;
}

/** 个人聊天 message 库文件（不含 biz 官号） */
function messageDbFiles(dataDir: string): string[] {
  const dir = path.join(dataDir, 'message');
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir)
    .filter((f) => /^message_\d+\.db$/i.test(f))
    .map((f) => path.join(dir, f));
}

function statMtime(f: string): number {
  try {
    return fs.statSync(f).mtimeMs;
  } catch {
    return 0;
  }
}

// 新消息先写进 -wal（主库要等 checkpoint），故取库与其 -wal 的较新 mtime
function fileMtime(f: string): number {
  return Math.max(statMtime(f), statMtime(f + '-wal'));
}

function newestMessageMtime(dataDir: string): number {
  let m = 0;
  for (const f of messageDbFiles(dataDir)) m = Math.max(m, fileMtime(f));
  return m;
}

/** 运行一次性密钥提取 exe；返回 64 位 hex 或 null（需要 Weixin.exe 在运行） */
export function recoverKeyViaExe(dataDir: string, timeoutMs = 60000): Promise<string | null> {
  return new Promise((resolve) => {
    let proc;
    try {
      proc = spawn(resourcePath('ti-wxkey.exe'), [`--db=${dataDir}`], { windowsHide: true });
    } catch {
      resolve(null);
      return;
    }
    let out = '';
    const timer = setTimeout(() => {
      try {
        proc.kill();
      } catch {
        /* ignore */
      }
      resolve(null);
    }, timeoutMs);
    proc.stdout.setEncoding('utf-8');
    proc.stdout.on('data', (d: string) => (out += d));
    proc.on('exit', () => {
      clearTimeout(timer);
      // 取最后一行 JSON
      const line = out.trim().split(/\r?\n/).filter(Boolean).pop() || '';
      try {
        const j = JSON.parse(line);
        resolve(j.ok && j.key ? String(j.key) : null);
      } catch {
        resolve(null);
      }
    });
    proc.on('error', () => {
      clearTimeout(timer);
      resolve(null);
    });
  });
}

function getStoredKey(): StoredKey | null {
  return (store.get('wechatKey') as StoredKey) || null;
}

/** 主动获取并缓存密钥（设置里的「获取密钥」按钮触发） */
export async function acquireKey(): Promise<{ ok: boolean; wxid?: string; error?: string }> {
  const acct = discoverAccount();
  if (!acct) return { ok: false, error: 'no-account' };
  const keyHex = await recoverKeyViaExe(acct.dataDir);
  if (!keyHex) return { ok: false, error: 'recover-failed' };
  const stored: StoredKey = {
    keyHex,
    wxid: acct.wxid,
    dataDir: acct.dataDir,
    recoveredAt: Date.now(),
  };
  store.set('wechatKey', stored);
  return { ok: true, wxid: acct.wxid };
}

export function hasKey(): boolean {
  const k = getStoredKey();
  return !!(k && k.keyHex && k.dataDir);
}

async function pollOnce() {
  if (!running) return;
  const stored = getStoredKey();
  if (!stored) return;
  const { dataDir, wxid } = stored;
  const key = Buffer.from(stored.keyHex, 'hex');

  const mtime = newestMessageMtime(dataDir);
  if (mtime <= lastMtime && watermark > 0) return; // 无新写入
  lastMtime = mtime;

  const files = messageDbFiles(dataDir);

  // 首轮建立基线（不回灌历史，也不解密任何库；只记录各文件 mtime）
  if (watermark === 0) {
    watermark = Math.floor(Date.now() / 1000);
    for (const f of files) fileMtimes.set(f, fileMtime(f));
    diagLog(`[wechat] baseline watermark=${watermark} dbFiles=${files.length}`);
    return;
  }

  // 联系人/群信息：按账号缓存；contact.db 变化（如运行时改了某人/群的免打扰）时重载，
  // 保证免打扰状态实时生效。mute 只在有新消息时才用得到，故随消息轮询按需重载即可。
  const contactDb = path.join(dataDir, 'contact', 'contact.db');
  const cMtime = fileMtime(contactDb);
  if (contactLoadedFor !== wxid || cMtime > contactDbMtime) {
    contacts = await loadContacts(key, contactDb).catch((e) => {
      logReadErr('loadContacts', e);
      return { byUsername: new Map(), byHash: new Map() };
    });
    contactLoadedFor = wxid;
    contactDbMtime = cMtime;
  }

  const all: NotificationItem[] = [];
  let maxTime = watermark;
  for (const dbFile of files) {
    // 只解密自身有新写入的库（跳过庞大的旧库，避免每轮重复解密卡顿）
    const mt = fileMtime(dbFile);
    if (mt <= (fileMtimes.get(dbFile) ?? 0)) continue;
    fileMtimes.set(dbFile, mt);
    const msgs = await readNewMessages(key, dbFile, watermark, wxid).catch((e) => {
      logReadErr('readNewMessages', e);
      return [];
    });
    for (const m of msgs) {
      if (m.createTime > maxTime) maxTime = m.createTime;
      // 会话（对方/群）：Msg_<md5(会话username)> -> 反查群名/头像/免打扰
      const conv = contacts.byHash.get(m.table.slice(4).toLowerCase());
      if (conv?.muted) continue; // 免打扰会话跳过
      const sender = contacts.byUsername.get(m.senderUsername)?.name || m.senderUsername || '微信';
      const isGroup = conv?.isGroup ?? false;
      const title = isGroup ? conv?.name || '群聊' : conv?.name || sender;
      const body = isGroup ? `${sender}: ${m.content}` : m.content;
      all.push({
        id: m.localId,
        aumid: 'wechat',
        app: '微信',
        icon: '',
        image: conv?.avatar || '',
        title,
        body,
        launch: '',
        atype: '',
        arrival: m.createTime * 1000,
      });
    }
  }
  if (all.length) {
    const times = all.map((a) => Math.floor(a.arrival / 1000)).join(',');
    diagLog(`[wechat] poll@${Math.floor(Date.now() / 1000)}: ${all.length} msg(s) createTimes=[${times}]`);
  }
  watermark = maxTime;
  if (all.length && onItems) onItems(all);
}

export function startWeChat(cb: (items: NotificationItem[]) => void) {
  onItems = cb;
  if (running) return;
  if (!hasKey()) {
    diagLog('[wechat] start skipped: no key');
    return; // 未取密钥则不启动（用户需先在设置里获取）
  }
  running = true;
  watermark = 0;
  lastMtime = 0;
  fileMtimes = new Map();
  // 切账号时旧账号的联系人/头像不能残留：强制按新账号重载
  contacts = { byUsername: new Map(), byHash: new Map() };
  contactLoadedFor = '';
  contactDbMtime = 0;
  const acct = discoverAccount();
  diagLog(
    `[wechat] started; account=${acct?.wxid ?? 'none'} dbFiles=${acct ? messageDbFiles(acct.dataDir).length : 0}`
  );
  void pollOnce();
  pollTimer = setInterval(() => void pollOnce(), POLL_INTERVAL);

  // 消息库一有写入即（防抖后）轮询
  if (acct) {
    try {
      let lastWatchLog = 0;
      watcher = fs.watch(path.join(acct.dataDir, 'message'), () => {
        const now = Date.now();
        if (now - lastWatchLog > 2000) {
          lastWatchLog = now;
          diagLog(`[wechat] fs.watch fired @${Math.floor(now / 1000)}`);
        }
        if (watchDebounce) clearTimeout(watchDebounce);
        watchDebounce = setTimeout(() => void pollOnce(), WATCH_DEBOUNCE);
      });
      diagLog('[wechat] fs.watch armed');
    } catch {
      diagLog('[wechat] fs.watch failed; polling only');
    }
  }
}

export function stopWeChat() {
  running = false;
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
  if (watcher) {
    try {
      watcher.close();
    } catch {
      /* ignore */
    }
    watcher = null;
  }
  if (watchDebounce) {
    clearTimeout(watchDebounce);
    watchDebounce = null;
  }
}

export function disposeWeChat() {
  stopWeChat();
}
