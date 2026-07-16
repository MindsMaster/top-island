import type { LyricsData } from '../../../../shared/ipc';
import type { LyricsProvider } from './types';
import { fetchJson, normEq, parseLrc, queryMatchesSong } from './shared';

const QQ_HEADERS = { Referer: 'https://y.qq.com/' };

async function searchQQ(
  title: string,
  artist: string
): Promise<{ songmid: string; durationMs: number } | null> {
  const query = artist ? `${title} ${artist}` : title;
  const url = `https://c.y.qq.com/soso/fcgi-bin/client_search_cp?w=${encodeURIComponent(query)}&format=json&n=10&p=1&cr=1&t=0`;
  const json = await fetchJson(url, QQ_HEADERS);
  const songs: any[] = json?.data?.song?.list;
  if (!Array.isArray(songs) || songs.length === 0) return null;

  if (artist) {
    for (const s of songs) {
      const names: string[] = (s.singer || []).map((a: any) => String(a.name || ''));
      if (names.some((n) => normEq(n, artist))) {
        return { songmid: s.songmid, durationMs: (s.interval || 0) * 1000 };
      }
    }
  }
  const first = songs[0];
  if (first.songname && !queryMatchesSong(query, String(first.songname))) return null;
  return { songmid: first.songmid, durationMs: (first.interval || 0) * 1000 };
}

async function fetchQQInner(title: string, artist: string): Promise<LyricsData | null> {
  const match = await searchQQ(title, artist);
  if (!match) return null;
  const json = await fetchJson(
    `https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg?songmid=${match.songmid}&format=json&nobase64=0&g_tk=5381`,
    QQ_HEADERS
  );
  const lrcB64: string = json?.lyric || '';
  if (!lrcB64) return { lines: [], durationMs: match.durationMs };
  const lrc = Buffer.from(lrcB64, 'base64').toString('utf8');
  return { lines: parseLrc(lrc), durationMs: match.durationMs };
}

export const providerQQ: LyricsProvider = {
  name: 'qq',
  async fetch(title, artist) {
    const r = await fetchQQInner(title, artist);
    if (r && (r.lines.length > 0 || r.durationMs > 0)) return r;
    return artist ? fetchQQInner(title, '') : null;
  },
};
