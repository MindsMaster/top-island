import { spawn, ChildProcessWithoutNullStreams } from 'child_process';
import { resourcePath } from '../paths';

interface Pending {
  resolve: (v: any) => void;
  timer: NodeJS.Timeout;
}

export interface BridgeOptions {
  /** helper 子命令（smtc | notify | clip） */
  mode: string;
  /** 日志前缀 */
  tag: string;
  /** helper 主动推送的事件行 */
  onEvent?: (event: string, data: any) => void;
  /** 进程拉起（含崩溃重启）后回调：重发 watch 等订阅性命令 */
  onSpawn?: () => void;
}

export interface Bridge {
  request(cmd: string, extra?: Record<string, any>, timeoutMs?: number): Promise<any>;
  dispose(): void;
}

export function createBridge(opts: BridgeOptions): Bridge {
  let proc: ChildProcessWithoutNullStreams | null = null;
  let seq = 0;
  let stdoutBuf = '';
  let disposed = false;
  let restartDelay = 1000;
  const pending = new Map<number, Pending>();

  function ensureProc() {
    if (proc || disposed) return;
    proc = spawn(resourcePath('topisland-winbridge.exe'), [opts.mode], {
      windowsHide: true,
      stdio: 'pipe',
    });

    proc.stdout.setEncoding('utf-8');
    proc.stdout.on('data', (chunk: string) => {
      stdoutBuf += chunk;
      let idx;
      while ((idx = stdoutBuf.indexOf('\n')) >= 0) {
        const line = stdoutBuf.slice(0, idx).trim();
        stdoutBuf = stdoutBuf.slice(idx + 1);
        if (!line) continue;
        let msg: any;
        try {
          msg = JSON.parse(line);
        } catch {
          continue;
        }
        if (typeof msg.event === 'string') {
          try {
            opts.onEvent?.(msg.event, msg.data);
          } catch (e) {
            console.error(`[${opts.tag}] event handler failed:`, e);
          }
          continue;
        }
        const p = pending.get(msg.id);
        if (p) {
          pending.delete(msg.id);
          clearTimeout(p.timer);
          p.resolve(msg);
        }
      }
    });

    proc.stderr.on('data', (d) => console.error(`[${opts.tag}]`, String(d).trim()));

    proc.on('exit', (code) => {
      console.warn(`[${opts.tag}] exited with code ${code}`);
      proc = null;
      stdoutBuf = '';
      for (const [, p] of pending) {
        clearTimeout(p.timer);
        p.resolve(null);
      }
      pending.clear();
      if (!disposed) {
        setTimeout(ensureProc, restartDelay);
        restartDelay = Math.min(restartDelay * 2, 30000);
      }
    });

    proc.on('spawn', () => {
      restartDelay = 1000;
      try {
        opts.onSpawn?.();
      } catch (e) {
        console.error(`[${opts.tag}] onSpawn failed:`, e);
      }
    });
  }

  function request(cmd: string, extra: Record<string, any> = {}, timeoutMs = 8000): Promise<any> {
    ensureProc();
    if (!proc) return Promise.resolve(null);
    const id = ++seq;
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        pending.delete(id);
        resolve(null);
      }, timeoutMs);
      pending.set(id, { resolve, timer });
      try {
        proc!.stdin.write(JSON.stringify({ id, cmd, ...extra }) + '\n');
      } catch {
        pending.delete(id);
        clearTimeout(timer);
        resolve(null);
      }
    });
  }

  return {
    request,
    dispose() {
      disposed = true;
      if (proc) {
        try {
          proc.stdin.end();
          proc.kill();
        } catch {}
        proc = null;
      }
    },
  };
}
