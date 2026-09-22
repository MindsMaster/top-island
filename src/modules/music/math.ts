import type { LyricLine } from '@/platform/types';

export interface Anchor {
  positionMs: number;
  anchorEpochMs: number;
  rate: number;
}

export function extrapolate(anchor: Anchor, nowMs: number, durationMs: number): number {
  const raw = anchor.positionMs + (nowMs - anchor.anchorEpochMs) * anchor.rate;
  const capped = durationMs > 0 ? Math.min(raw, durationMs) : raw;
  return Math.max(0, capped);
}

/** QQ 音乐 SMTC 时间轴滞后 */
export function builtinLyricOffset(sourceAppId: string): number {
  return sourceAppId.toLowerCase().includes('qqmusic') ? 400 : 0;
}

export function lyricIndexAt(lines: readonly LyricLine[], positionMs: number): number {
  if (!lines.length) return -1;
  let lo = 0;
  let hi = lines.length - 1;
  let idx = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (lines[mid].timeMs <= positionMs) {
      idx = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return idx;
}

export function formatTimeMs(ms: number): string {
  const total = Math.max(0, Math.floor((Number.isFinite(ms) ? ms : 0) / 1000));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return m + ':' + String(s).padStart(2, '0');
}

export function trackIdentity(songId: string | undefined, title: string, artist: string): string {
  return songId || `${title}|${artist}`;
}
