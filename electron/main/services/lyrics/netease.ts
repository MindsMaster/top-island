import type { LyricsData } from '../../../../shared/ipc';
import type { LyricsProvider } from './types';
import { fetchJson, normEq, parseLrc, queryMatchesSong } from './shared';

async function search163(title: string, artist: string): Promise<{ id: number; durationMs: number } | null> {
  const query = artist ? `${title} ${artist}` : title;
  const url = `https://music.163.com/api/search/get/web?s=${encodeURIComponent(query)}&type=1&offset=0&total=true&limit=10`;
  const json = await fetchJson(url);
  const songs: any[] = json?.result?.songs;
  if (!Array.isArray(songs) || songs.length === 0) return null;

  if (artist) {
    for (const s of songs) {
      const names: string[] = (s.artists || []).map((a: any) => String(a.name || ''));
      if (names.some((n) => normEq(n, artist))) {
        return { id: s.id, durationMs: s.duration || 0 };
      }
    }
  }
  const first = songs[0];
  if (first.name && !queryMatchesSong(query, String(first.name))) return null;
  return { id: first.id, durationMs: first.duration || 0 };
}

async function fetch163Inner(title: string, artist: string): Promise<LyricsData | null> {
  const match = await search163(title, artist);
  if (!match) return null;
  const json = await fetchJson(`https://music.163.com/api/song/lyric?id=${match.id}&lv=1&kv=1&tv=-1`);
  const lrc: string = json?.lrc?.lyric || '';
  return { lines: parseLrc(lrc), durationMs: match.durationMs };
}

/**
 * 按 songId 直取歌词与时长 不经搜索 有确切 songId（来自 elog 位置源）时优先
 * 避免搜索匹配到 live/翻唱等错误版本导致歌词整体错位
 */
export async function fetchLyrics163ById(songId: string): Promise<LyricsData | null> {
  const [lyricJson, detailJson] = await Promise.all([
    fetchJson(`https://music.163.com/api/song/lyric?id=${songId}&lv=1&kv=1&tv=-1`).catch(() => null),
    fetchJson(`https://music.163.com/api/song/detail?id=${songId}&ids=%5B${songId}%5D`).catch(() => null),
  ]);
  const lrc: string = lyricJson?.lrc?.lyric || '';
  const durationMs: number = detailJson?.songs?.[0]?.duration || 0;
  if (!lrc && !durationMs) return null;
  return { lines: parseLrc(lrc), durationMs };
}

export const provider163: LyricsProvider = {
  name: '163',
  async fetch(title, artist) {
    const r = await fetch163Inner(title, artist);
    if (r && (r.lines.length > 0 || r.durationMs > 0)) return r;
    // 带歌手检索失败则退化为仅曲名检索
    return artist ? fetch163Inner(title, '') : null;
  },
};
