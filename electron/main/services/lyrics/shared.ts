import type { LyricLine } from '../../../../shared/ipc';

const UA =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36';

export async function fetchJson(
  url: string,
  headers: Record<string, string> = {},
  timeoutMs = 8000
): Promise<any> {
  const res = await fetch(url, {
    headers: { 'User-Agent': UA, ...headers },
    signal: AbortSignal.timeout(timeoutMs),
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

/** 标题相关性校验：搜索词与歌名至少要有词面上的交集（防止无关命中） */
export function queryMatchesSong(query: string, songName: string): boolean {
  const norm = (s: string) => s.toLowerCase().replace(/[\s\-_()[\]（）【】'"',.，。!！?？]+/g, '');
  const q = norm(query);
  const n = norm(songName);
  if (!q || !n) return false;
  return q.includes(n) || n.includes(q);
}

export function parseLrc(lrc: string): LyricLine[] {
  const lines: LyricLine[] = [];
  const timeTag = /\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?]/g;
  for (const raw of lrc.split('\n')) {
    const text = raw.replace(timeTag, '').trim();
    if (!text) continue;
    timeTag.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = timeTag.exec(raw)) !== null) {
      const min = parseInt(m[1]);
      const sec = parseInt(m[2]);
      // 小数部分按位数解释：2 位是厘秒，3 位是毫秒
      const fracRaw = m[3] || '0';
      const frac = parseInt(fracRaw) * (fracRaw.length === 3 ? 1 : fracRaw.length === 2 ? 10 : 100);
      lines.push({ timeMs: (min * 60 + sec) * 1000 + frac, text });
    }
  }
  lines.sort((a, b) => a.timeMs - b.timeMs);
  return lines;
}

export function normEq(a: string, b: string): boolean {
  return a.toLowerCase() === b.toLowerCase();
}
