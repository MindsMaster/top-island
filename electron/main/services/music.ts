import type { LyricsData, MusicAction, MusicArtwork, MusicState } from '../../../shared/ipc';
import {
  queryWindows,
  controlWindows,
  disposeWindowsHelper,
  onSmtcChange,
  seekWindows,
  thumbnailWindows,
} from './music/windows';
import { fetchLyrics, fetchLyrics163ById } from './lyrics';
import { lyricsSupported } from './media-sources';
import { pollExternalPosition } from './position';

interface ArtworkCache {
  trackKey: string;
  hash: string;
  dataUrl: string;
}

let artworkCache: ArtworkCache | null = null;
let fetchingKey: string | null = null;
let lastTrackKey = '';

interface LyricsCache {
  trackKey: string;
  data: LyricsData;
  /** 是否按平台歌曲 ID 精确获取——只有此时时长才可作权威时间轴 */
  byId: boolean;
}

/** 歌词获取结果需自带 byId 标记：按 ID 失败回退搜索时，时长降级为估算 */
type LyricsFetch = () => Promise<{ data: LyricsData; byId: boolean } | null>;

let lyricsCache: LyricsCache | null = null;
let lyricsFetchingKey: string | null = null;
/** 歌词缓存键与 SMTC 曲目键解耦：网易云用 elog songId（精确），其余用曲目 key（搜索） */
let lastLyricsKey = '';

async function fetchLyricsFor(lyricsKey: string, fetch: LyricsFetch) {
  if (lyricsFetchingKey === lyricsKey) return;
  lyricsFetchingKey = lyricsKey;
  try {
    const r = await fetch();
    if (r && lastLyricsKey === lyricsKey) {
      lyricsCache = { trackKey: lyricsKey, data: r.data, byId: r.byId };
    }
  } finally {
    if (lyricsFetchingKey === lyricsKey) lyricsFetchingKey = null;
  }
}

function trackKeyOf(s: { title?: string; artist?: string; app?: string }): string {
  return `${s.title ?? ''}|${s.artist ?? ''}|${s.app ?? ''}`;
}

function sniffMime(b64: string): string {
  const head = Buffer.from(b64.slice(0, 12), 'base64');
  if (head[0] === 0x89 && head[1] === 0x50) return 'image/png';
  if (head[0] === 0xff && head[1] === 0xd8) return 'image/jpeg';
  if (head[0] === 0x47 && head[1] === 0x49) return 'image/gif';
  if (head[0] === 0x42 && head[1] === 0x4d) return 'image/bmp';
  return 'image/jpeg';
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** 后台抓封面，带重试；曲目已切换则放弃 */
async function fetchArtwork(trackKey: string) {
  if (fetchingKey === trackKey) return;
  fetchingKey = trackKey;
  try {
    await sleep(800);
    for (let attempt = 0; attempt < 6; attempt++) {
      if (lastTrackKey !== trackKey) return; // 已切歌
      const thumb = await thumbnailWindows();
      if (thumb && lastTrackKey === trackKey) {
        artworkCache = {
          trackKey,
          hash: thumb.hash,
          dataUrl: `data:${sniffMime(thumb.b64)};base64,${thumb.b64}`,
        };
        return;
      }
      await sleep(attempt < 3 ? 500 : 1000);
    }
  } finally {
    if (fetchingKey === trackKey) fetchingKey = null;
  }
}

export async function pollMusicState(): Promise<MusicState> {
  const result: MusicState = { isPlaying: false };

  const smtc = await queryWindows();
  if (smtc) {
    const title = smtc.title || '';
    const artist = smtc.artist || '';
    const source = smtc.app || '';
    if (title || source) {
      result.isPlaying = smtc.playing;
      result.track = title || `SMTC: ${source.includes('.') ? source.split('.').pop() : source.slice(-30)}`;
      if (artist) result.artist = artist;
      result.sourceAppId = source;
      result.positionMs = smtc.positionMs || 0;
      result.durationMs = smtc.durationMs || 0;
      result.seekSupported = smtc.seekSupported;

      const key = trackKeyOf(smtc);
      if (key !== lastTrackKey) {
        lastTrackKey = key;
        void fetchArtwork(key);
      }
      if (artworkCache && artworkCache.trackKey === key) {
        result.artworkHash = artworkCache.hash;
      }

      // SMTC 无时间轴的源（网易云等）：外部位置源补真实进度；
      // 源能给出 songId 时歌词/时长按 ID 精确获取
      let lyricsKey = key;
      let byIdFetcher: LyricsFetch | null = null;
      if (!result.durationMs) {
        const ext = await pollExternalPosition(source);
        if (ext) {
          result.positionMs = ext.positionMs;
          // 网易云 SMTC PlaybackStatus 冻结/滞后不可信，播放态以 elog 为准
          // （否则 UI 播放/暂停按钮显示反相，点按发出错误动词成为无操作）
          result.isPlaying = ext.playing;
          if (ext.durationMs > 0) result.durationMs = ext.durationMs;
          if (ext.songId) {
            const songId = ext.songId;
            lyricsKey = `163:${songId}`;
            byIdFetcher = async () => {
              const byId = await fetchLyrics163ById(songId);
              if (byId) return { data: byId, byId: true };
              const searched = await fetchLyrics(title, artist);
              return searched ? { data: searched, byId: false } : null;
            };
          }
        }
      }

      const searchFetcher: LyricsFetch = async () => {
        const data = await fetchLyrics(title, artist);
        return data ? { data, byId: false } : null;
      };
      const fetcher = byIdFetcher ?? (lyricsSupported(source) ? searchFetcher : null);

      if (lyricsKey !== lastLyricsKey) {
        lastLyricsKey = lyricsKey;
        if (fetcher) void fetchLyricsFor(lyricsKey, fetcher);
      }
      if (lyricsCache && lyricsCache.trackKey === lyricsKey) {
        if (lyricsCache.data.lines.length > 0) result.lyricsId = lyricsKey;
        if (!result.durationMs && lyricsCache.data.durationMs > 0) {
          // 按 ID 取得的时长是权威值，与外部位置源构成完整时间轴（渲染层可回同步）；
          // 搜索得到的时长可能是错误版本，仅作估算展示
          if (lyricsCache.byId) result.durationMs = lyricsCache.data.durationMs;
          else result.estimatedDurationMs = lyricsCache.data.durationMs;
        }
      }
      return result;
    }
  }

  lastTrackKey = '';
  lastLyricsKey = '';
  return result;
}

// helper watch 到 SMTC 变化（播放/暂停/切歌）→ 立刻重算完整状态推给岛。
// 渲染层 2s 轮询保留：兜底 + 驱动网易云 elog 进度等非事件源

let musicPushCb: ((state: MusicState) => void) | null = null;
let pushInFlight = false;

async function pushMusicState() {
  if (pushInFlight) return;
  pushInFlight = true;
  try {
    const state = await pollMusicState();
    musicPushCb?.(state);
  } finally {
    pushInFlight = false;
  }
}

export function startMusicEvents(cb: (state: MusicState) => void) {
  musicPushCb = cb;
  onSmtcChange(() => void pushMusicState());
}

export function getArtwork(hash: string): MusicArtwork | null {
  if (artworkCache && artworkCache.hash === hash) {
    return { hash: artworkCache.hash, dataUrl: artworkCache.dataUrl };
  }
  return null;
}

export function getLyrics(id: string): LyricsData | null {
  if (lyricsCache && lyricsCache.trackKey === id) return lyricsCache.data;
  return null;
}

export async function musicSeek(positionMs: number): Promise<boolean> {
  return seekWindows(positionMs);
}

export async function musicControl(action: MusicAction, level?: number): Promise<string> {
  return controlWindows(action, level);
}

export function disposeMusic() {
  disposeWindowsHelper();
}
