import { app } from 'electron';
import * as fs from 'fs';
import * as path from 'path';

export function diagPath(): string {
  return path.join(app.getPath('userData'), 'diag.log');
}

/** 单次会话写入上限（防持续卡顿刷爆日志）；超过后静默丢弃 */
const SESSION_LINE_CAP = 1000;
let sessionLines = 0;

export function diagLog(msg: string) {
  if (sessionLines >= SESSION_LINE_CAP) return;
  sessionLines++;
  const line =
    `${new Date().toISOString()} ${msg}\n` +
    (sessionLines === SESSION_LINE_CAP ? '(session line cap reached, further entries suppressed)\n' : '');
  fs.appendFile(diagPath(), line, () => {});
}

/** 超过 512KB 轮转到 diag.old.log（只保留一代），日志总量恒定有界 ~1MB */
function rotateIfNeeded() {
  try {
    const p = diagPath();
    if (fs.existsSync(p) && fs.statSync(p).size > 512 * 1024) {
      fs.renameSync(p, path.join(app.getPath('userData'), 'diag.old.log'));
    }
  } catch {}
}

/** 包一层计时：耗时超过阈值的操作写入诊断日志 */
export function timed<T>(label: string, fn: () => T, thresholdMs = 50): T {
  const t0 = Date.now();
  try {
    return fn();
  } finally {
    const dt = Date.now() - t0;
    if (dt > thresholdMs) diagLog(`slow-op ${label} took ${dt}ms`);
  }
}

/** 事件循环卡顿检测：500ms 心跳，漂移超 100ms 即主进程被同步阻塞过 */
export function startJankMonitor() {
  rotateIfNeeded();
  diagLog(`=== session start (v${app.getVersion()}) ===`);
  let last = Date.now();
  setInterval(() => {
    const now = Date.now();
    const drift = now - last - 500;
    last = now;
    if (drift > 100) diagLog(`main-loop blocked ~${drift}ms`);
  }, 500);
}
