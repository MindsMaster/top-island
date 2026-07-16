import * as crypto from 'crypto';

const PAGE_SIZE = 4096;
const SALT_SIZE = 16;
const IV_SIZE = 16;
const HMAC_SIZE = 64;
const RESERVE = IV_SIZE + HMAC_SIZE; // 80
const KEY_SIZE = 32;
const KDF_ROUNDS = 256000;
const SQLITE_HEADER = Buffer.concat([Buffer.from('SQLite format 3', 'latin1'), Buffer.from([0])]); // 16 bytes

function pbkdf2(pass: Buffer, salt: Buffer, rounds: number, dk: number): Buffer {
  return crypto.pbkdf2Sync(pass, salt, rounds, dk, 'sha512');
}

function macSaltOf(salt: Buffer): Buffer {
  const m = Buffer.alloc(SALT_SIZE);
  for (let i = 0; i < SALT_SIZE; i++) m[i] = salt[i] ^ 0x3a;
  return m;
}

function pageHmac(macKey: Buffer, page: Buffer, pageNum: number): Buffer {
  const h = crypto.createHmac('sha512', macKey);
  const start = pageNum === 1 ? SALT_SIZE : 0;
  h.update(page.subarray(start, PAGE_SIZE - RESERVE + IV_SIZE));
  const pn = Buffer.alloc(4);
  pn.writeUInt32LE(pageNum, 0);
  h.update(pn);
  return h.digest();
}

export interface KeyMaterial {
  encKey: Buffer;
  macKey: Buffer;
  mode: 'raw_enc_key' | 'sqlcipher_passphrase';
}

/**
 * 判断传入的 key 是 raw enc_key 还是 SQLCipher passphrase（按 page1 HMAC 验证）
 * 返回 null 表示这把 key 对该库无效
 */
export function resolveKeyMaterial(key: Buffer, page1: Buffer): KeyMaterial | null {
  if (page1.length < PAGE_SIZE) return null;
  const salt = page1.subarray(0, SALT_SIZE);
  const macSalt = macSaltOf(salt);
  const stored = page1.subarray(PAGE_SIZE - HMAC_SIZE, PAGE_SIZE);

  // 1) key 直接作为 enc_key
  {
    const encKey = key;
    const macKey = pbkdf2(encKey, macSalt, 2, KEY_SIZE);
    if (crypto.timingSafeEqual(stored, pageHmac(macKey, page1, 1)))
      return { encKey, macKey, mode: 'raw_enc_key' };
  }
  // 2) key 作为 passphrase 派生 enc_key
  {
    const encKey = pbkdf2(key, salt, KDF_ROUNDS, KEY_SIZE);
    const macKey = pbkdf2(encKey, macSalt, 2, KEY_SIZE);
    if (crypto.timingSafeEqual(stored, pageHmac(macKey, page1, 1)))
      return { encKey, macKey, mode: 'sqlcipher_passphrase' };
  }
  return null;
}

// 派生密钥缓存
const kmCache = new Map<string, KeyMaterial>();

export function getKeyMaterial(key: Buffer, page1: Buffer): KeyMaterial | null {
  const saltHex = page1.subarray(0, SALT_SIZE).toString('hex');
  const cached = kmCache.get(saltHex);
  if (cached) return cached;
  const km = resolveKeyMaterial(key, page1);
  if (km) kmCache.set(saltHex, km);
  return km;
}

/** SQLCipher 页大小（读 page1 判定密钥模式用） */
export const CIPHER_PAGE_SIZE = PAGE_SIZE;

function decryptPage(encKey: Buffer, page: Buffer, pageNum: number): Buffer {
  const iv = page.subarray(PAGE_SIZE - RESERVE, PAGE_SIZE - RESERVE + IV_SIZE);
  const offset = pageNum === 1 ? SALT_SIZE : 0;
  const body = page.subarray(offset, PAGE_SIZE - RESERVE);
  const decipher = crypto.createDecipheriv('aes-256-cbc', encKey, iv);
  decipher.setAutoPadding(false);
  const dec = Buffer.concat([decipher.update(body), decipher.final()]);
  const reserve = Buffer.alloc(RESERVE);
  if (pageNum === 1) return Buffer.concat([SQLITE_HEADER, dec, reserve]);
  return Buffer.concat([dec, reserve]);
}

function isAllZero(page: Buffer): boolean {
  for (let i = 0; i < PAGE_SIZE; i += 512) if (page[i] !== 0) return false;
  return true;
}

const WAL_HEADER_SIZE = 32;
const WAL_FRAME_HEADER_SIZE = 24;

/** 解析 WAL，收集每个 page 号的最新加密帧（仅当前 salt 有效的帧） */
function collectWalFrames(wal: Buffer | null): Map<number, Buffer> {
  const frames = new Map<number, Buffer>();
  if (!wal || wal.length < WAL_HEADER_SIZE + WAL_FRAME_HEADER_SIZE + PAGE_SIZE) return frames;
  if (wal.readUInt32BE(8) !== PAGE_SIZE) return frames; // WAL 页大小需与库一致
  const hdrSalt1 = wal.readUInt32BE(16);
  const hdrSalt2 = wal.readUInt32BE(20);
  let o = WAL_HEADER_SIZE;
  while (o + WAL_FRAME_HEADER_SIZE + PAGE_SIZE <= wal.length) {
    // checkpoint 后 WAL 重置会改 salt；salt 不匹配的是残留旧帧，到此为止
    if (wal.readUInt32BE(o + 8) !== hdrSalt1 || wal.readUInt32BE(o + 12) !== hdrSalt2) break;
    const pageNum = wal.readUInt32BE(o);
    if (pageNum >= 1) {
      const start = o + WAL_FRAME_HEADER_SIZE;
      frames.set(pageNum, Buffer.from(wal.subarray(start, start + PAGE_SIZE))); // 后帧覆盖前帧
    }
    o += WAL_FRAME_HEADER_SIZE + PAGE_SIZE;
  }
  return frames;
}

/**
 * 解密 SQLCipher 库为明文 SQLite，并叠加 -wal 里尚未 checkpoint 的最新页，
 * 使新消息写入 WAL 即可读到，不必等主库 checkpoint。key 无效返回 null。
 */
export function decryptDatabaseWithWal(key: Buffer, file: Buffer, wal: Buffer | null): Buffer | null {
  if (file.length < PAGE_SIZE) return null;
  const page1 = file.subarray(0, PAGE_SIZE);
  const km = getKeyMaterial(key, page1);
  if (!km) return null;

  const mainPages = Math.floor(file.length / PAGE_SIZE);
  const frames = collectWalFrames(wal);
  let maxPage = mainPages;
  for (const pn of frames.keys()) if (pn > maxPage) maxPage = pn;

  const out = Buffer.alloc(maxPage * PAGE_SIZE);
  // 主库页（被 WAL 覆盖的页跳过，改用 WAL 帧）
  for (let p = 0; p < mainPages; p++) {
    if (frames.has(p + 1)) continue;
    const page = file.subarray(p * PAGE_SIZE, (p + 1) * PAGE_SIZE);
    if (isAllZero(page)) continue;
    try {
      decryptPage(km.encKey, page, p + 1).copy(out, p * PAGE_SIZE);
    } catch {
      /* 失败页留零 */
    }
  }
  // WAL 最新页
  for (const [pageNum, encPage] of frames) {
    try {
      decryptPage(km.encKey, encPage, pageNum).copy(out, (pageNum - 1) * PAGE_SIZE);
    } catch {
      /* 失败页留零 */
    }
  }
  return out;
}

/** 解密整个 SQLCipher 库文件为明文 SQLite 字节（不含 WAL）。key 无效返回 null。 */
export function decryptDatabase(key: Buffer, file: Buffer): Buffer | null {
  return decryptDatabaseWithWal(key, file, null);
}

/**
 * 同 decryptDatabaseWithWal，但每 256 页让出一次事件循环（setImmediate），
 * 避免大库逐页 AES 解密长时间阻塞主进程。
 */
export async function decryptDatabaseWithWalAsync(
  key: Buffer,
  file: Buffer,
  wal: Buffer | null
): Promise<Buffer | null> {
  if (file.length < PAGE_SIZE) return null;
  const page1 = file.subarray(0, PAGE_SIZE);
  const km = getKeyMaterial(key, page1);
  if (!km) return null;

  const mainPages = Math.floor(file.length / PAGE_SIZE);
  const frames = collectWalFrames(wal);
  let maxPage = mainPages;
  for (const pn of frames.keys()) if (pn > maxPage) maxPage = pn;

  const out = Buffer.alloc(maxPage * PAGE_SIZE);
  for (let p = 0; p < mainPages; p++) {
    if (frames.has(p + 1)) continue;
    const page = file.subarray(p * PAGE_SIZE, (p + 1) * PAGE_SIZE);
    if (isAllZero(page)) continue;
    try {
      decryptPage(km.encKey, page, p + 1).copy(out, p * PAGE_SIZE);
    } catch {
      /* 失败页留零 */
    }
    if ((p & 0xff) === 0xff) await new Promise((r) => setImmediate(r)); // 每 256 页喘一下
  }
  for (const [pageNum, encPage] of frames) {
    try {
      decryptPage(km.encKey, encPage, pageNum).copy(out, (pageNum - 1) * PAGE_SIZE);
    } catch {
      /* 失败页留零 */
    }
  }
  return out;
}
