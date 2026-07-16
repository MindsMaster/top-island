import type { LyricsData } from '../../../shared/ipc';
import type { LyricsProvider } from './lyrics/types';
import { provider163, fetchLyrics163ById } from './lyrics/netease';
import { providerQQ } from './lyrics/qq';

export type { LyricsProvider } from './lyrics/types';
export { fetchLyrics163ById };

const PROVIDERS: LyricsProvider[] = [provider163, providerQQ];

export async function fetchLyrics(title: string, artist: string): Promise<LyricsData | null> {
  if (!title) return null;
  let best: LyricsData | null = null;
  for (const provider of PROVIDERS) {
    try {
      const r = await provider.fetch(title, artist);
      if (!r) continue;
      // 有歌词行即最优；只有时长则记为兜底继续尝试下一个源
      if (r.lines.length > 0) return r;
      if (!best && r.durationMs > 0) best = r;
    } catch (e) {
      console.error(`[Lyrics] provider ${provider.name} failed:`, e);
    }
  }
  return best;
}
