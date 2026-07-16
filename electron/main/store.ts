import { app } from 'electron';
import * as fs from 'fs';
import * as path from 'path';

class JsonStore {
  private data: Record<string, unknown> | null = null;
  private saveTimer: NodeJS.Timeout | null = null;

  private get file(): string {
    return path.join(app.getPath('userData'), 'store.json');
  }

  private tryRead(f: string): Record<string, unknown> | null {
    try {
      return JSON.parse(fs.readFileSync(f, 'utf-8'));
    } catch {
      return null;
    }
  }

  private load(): Record<string, unknown> {
    if (this.data) return this.data;
    this.data = this.tryRead(this.file) ?? this.tryRead(this.file + '.bak') ?? {};
    return this.data;
  }

  get<T>(key: string): T | null {
    const v = this.load()[key];
    return v === undefined ? null : (v as T);
  }

  set(key: string, value: unknown) {
    this.load()[key] = value;
    if (this.saveTimer) clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => this.flush(), 300);
  }

  /** 清空全部数据（设置里的「重置」用）：内存置空并立即落盘 */
  clear() {
    this.data = {};
    this.flush();
  }

  /** 原子落盘：写 .tmp → 备份旧正本到 .bak → rename .tmp 覆盖正本 */
  flush() {
    if (this.saveTimer) {
      clearTimeout(this.saveTimer);
      this.saveTimer = null;
    }
    if (!this.data) return;
    const file = this.file;
    try {
      fs.writeFileSync(file + '.tmp', JSON.stringify(this.data));
      try {
        fs.copyFileSync(file, file + '.bak'); // 首次正本尚不存在时忽略
      } catch {}
      fs.renameSync(file + '.tmp', file);
    } catch (e) {
      console.error('[Store] save failed:', e);
    }
  }
}

export const store = new JsonStore();
