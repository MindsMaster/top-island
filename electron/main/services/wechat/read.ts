import * as path from 'path';
import { app } from 'electron';
import { Worker } from 'worker_threads';
import { diagLog } from '../../diag';
import type { Contacts, RawMessage } from './read-shared';

export type { Contacts, ContactInfo, RawMessage } from './read-shared';

let worker: Worker | null = null;
let seq = 0;
const pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();

// 打包后 asarUnpack 把 dist/read-worker.js 与原生模块解到 app.asar.unpacked；
// dev 下就是项目根。worker 文件与 .node 都基于这个根定位。
function unpackedBase(): string {
  return app.isPackaged ? app.getAppPath() + '.unpacked' : app.getAppPath();
}
function workerFile(): string {
  return path.join(unpackedBase(), 'dist', 'read-worker.js');
}

function nativeBindingPath(): string {
  return path.join(
    unpackedBase(),
    'node_modules',
    'better-sqlite3-multiple-ciphers',
    'build',
    'Release',
    'better_sqlite3.node'
  );
}

function getWorker(): Worker {
  if (worker) return worker;
  const w = new Worker(workerFile(), { workerData: { nativeBinding: nativeBindingPath() } });
  w.on('message', (m: { id: number; ok: boolean; result?: unknown; error?: string }) => {
    const p = pending.get(m.id);
    if (!p) return;
    pending.delete(m.id);
    if (m.ok) p.resolve(m.result);
    else p.reject(new Error(m.error || 'worker-error'));
  });
  const fail = (e: unknown) => {
    const err = e instanceof Error ? e : new Error(String(e));
    diagLog(`[wechat] read-worker error: ${err.message}`);
    for (const p of pending.values()) p.reject(err);
    pending.clear();
    worker = null; // 下次调用重建
  };
  w.on('error', fail);
  w.on('exit', () => fail(new Error('worker-exited')));
  worker = w;
  return w;
}

function rpc<T>(job: Record<string, unknown>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const id = ++seq;
    pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
    try {
      getWorker().postMessage({ id, ...job });
    } catch (e) {
      pending.delete(id);
      reject(e instanceof Error ? e : new Error(String(e)));
    }
  });
}

export function loadContacts(key: Buffer, contactDb: string): Promise<Contacts> {
  return rpc<Contacts>({ kind: 'contacts', keyHex: key.toString('hex'), file: contactDb });
}

export function readNewMessages(
  key: Buffer,
  messageDb: string,
  sinceTime: number,
  myWxid: string
): Promise<RawMessage[]> {
  return rpc<RawMessage[]>({
    kind: 'messages',
    keyHex: key.toString('hex'),
    file: messageDb,
    sinceTime,
    myWxid,
  });
}
