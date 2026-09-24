import { computed, reactive } from 'vue';
import { musicApi } from '@/platform/music';
import { mediaUrl } from '@/platform/media';
import { settings } from '@/core/settings';
import { on as onAppEvent } from '@/core/bus';
import type { LyricLine, MusicAction, MusicState } from '@/platform/types';
import {
  builtinLyricOffset,
  extrapolate as extrapolateAnchor,
  formatTimeMs,
  lyricIndexAt,
  trackIdentity,
} from './math';

export { formatTimeMs };

/** 拖动进度时组件直写 isScrubbing positionMs */
export const musicState = reactive({
  provider: '',
  isPlaying: false,
  hasMusic: false,
  currentTrack: '',
  currentArtist: '',
  currentAlbum: '',
  currentSourceApp: '',
  songId: '',
  artworkUrl: '',
  positionMs: 0,
  durationMs: 0,
  lyricLines: [] as LyricLine[],
  seekSupported: false,
  isScrubbing: false,
});

let lastPlayAction = 0;
let lastSeekAction = 0;
let missCount = 0;
let pollTimer: number | null = null;
let tickTimer: number | null = null;
let watchdogTimer: number | null = null;
let lyricsIdLoaded = '';
let lastTickAt = 0;

let anchorPositionMs = 0;
let anchorEpochMs = 0;
let rate = 0;

function extrapolate(now: number): number {
  return extrapolateAnchor({ positionMs: anchorPositionMs, anchorEpochMs, rate }, now, musicState.durationMs);
}

function setAnchor(positionMs: number, epochMs: number, r: number) {
  anchorPositionMs = positionMs;
  anchorEpochMs = epochMs || Date.now();
  rate = r;
  musicState.positionMs = extrapolate(Date.now());
}

function clearState() {
  musicState.provider = '';
  musicState.hasMusic = false;
  musicState.currentTrack = '';
  musicState.currentArtist = '';
  musicState.currentAlbum = '';
  musicState.currentSourceApp = '';
  musicState.songId = '';
  musicState.isPlaying = false;
  musicState.artworkUrl = '';
  musicState.durationMs = 0;
  musicState.lyricLines = [];
  lyricsIdLoaded = '';
  musicState.seekSupported = false;
  setAnchor(0, Date.now(), 0);
}

function loadLyrics(id: string) {
  lyricsIdLoaded = id;
  musicApi
    .lyrics(id)
    .then((data) => {
      if (data && lyricsIdLoaded === id) musicState.lyricLines = data.lines;
    })
    .catch(() => {});
}

function handleState(data: MusicState) {
  if (!data.track) {
    missCount++;
    if (missCount >= 3) {
      clearState();
      missCount = 0;
    }
    return;
  }

  missCount = 0;
  const t = data.track.replace(/^["\s]+|["\s]+$/g, '');
  const a = (data.artist || '').replace(/^["\s]+|["\s]+$/g, '');
  const idNow = trackIdentity(data.songId, t, a);
  const idPrev = trackIdentity(musicState.songId, musicState.currentTrack, musicState.currentArtist);
  const trackChanged = musicState.hasMusic && idNow !== idPrev;

  musicState.provider = data.provider;
  musicState.currentTrack = t;
  musicState.currentArtist = a;
  musicState.currentAlbum = data.album || '';
  musicState.currentSourceApp = data.sourceAppId || '';
  musicState.songId = data.songId || '';
  musicState.hasMusic = true;

  // 点击后暂以本地为准 防轮询回跳
  const holdPlayState = !trackChanged && Date.now() - lastPlayAction < 2000;
  if (!holdPlayState) musicState.isPlaying = data.isPlaying;

  musicState.durationMs = data.durationMs || 0;
  musicState.seekSupported = !!data.seekSupported;

  const artworkUrl = data.artworkUrl || (data.artworkHash ? mediaUrl('artwork', data.artworkHash) : '');
  if (artworkUrl || trackChanged) musicState.artworkUrl = artworkUrl;

  if (trackChanged) {
    musicState.lyricLines = [];
    lyricsIdLoaded = '';
  }

  // 防服务端旧位置拽回锚点
  const effectiveRate = musicState.isPlaying ? data.rate || 1 : 0;
  if (!musicState.isScrubbing && Date.now() - lastSeekAction > 3000) {
    setAnchor(data.positionMs, data.anchorEpochMs, effectiveRate);
  } else {
    rate = effectiveRate;
  }

  if (data.lyricsId && data.lyricsId !== lyricsIdLoaded) {
    loadLyrics(data.lyricsId);
  }
}

async function poll() {
  if (!settings.diagnostics.musicPoll) return;
  try {
    handleState(await musicApi.poll());
  } catch {}
}

function tick() {
  lastTickAt = Date.now();
  if (!musicState.isPlaying || musicState.isScrubbing) return;
  musicState.positionMs = extrapolate(Date.now());
}

/** webview 后台节流停跳则重建 */
function watchdog() {
  if (tickTimer !== null && lastTickAt > 0 && Date.now() - lastTickAt > 2000) {
    clearInterval(tickTimer);
    tickTimer = window.setInterval(tick, 100);
  }
}

let stateListenerRegistered = false;

export function startMusicPoll() {
  stopMusicPoll();
  if (!stateListenerRegistered) {
    stateListenerRegistered = true;
    musicApi.onState((state) => {
      if (settings.diagnostics.musicPoll) handleState(state);
    });
  }
  poll();
  pollTimer = window.setInterval(poll, 2000);
  // 歌词切行延迟上限
  lastTickAt = Date.now();
  tickTimer = window.setInterval(tick, 100);
  watchdogTimer = window.setInterval(watchdog, 3000);
}

export function stopMusicPoll() {
  if (pollTimer !== null) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
  if (tickTimer !== null) {
    clearInterval(tickTimer);
    tickTimer = null;
  }
  if (watchdogTimer !== null) {
    clearInterval(watchdogTimer);
    watchdogTimer = null;
  }
}

function control(action: MusicAction, level?: number) {
  musicApi.control(action, level).catch(() => {});
}

export function togglePlay() {
  const next = !musicState.isPlaying;
  control(next ? 'play' : 'pause');
  musicState.isPlaying = next;
  lastPlayAction = Date.now();
  // 重设锚点防进度跳变
  setAnchor(extrapolate(Date.now()), Date.now(), next ? rate || 1 : 0);
}

/** 只恢复自己暂停的那次 */
let pausedForRinging = false;

onAppEvent('alarm:ringing', (ringing) => {
  if (ringing) {
    if (!musicState.isPlaying) return;
    control('pause');
    musicState.isPlaying = false;
    lastPlayAction = Date.now();
    setAnchor(extrapolate(Date.now()), Date.now(), 0);
    pausedForRinging = true;
    return;
  }
  if (!pausedForRinging) return;
  pausedForRinging = false;
  if (musicState.isPlaying) return;
  control('play');
  musicState.isPlaying = true;
  lastPlayAction = Date.now();
  setAnchor(extrapolate(Date.now()), Date.now(), rate || 1);
});

export function skipTrack(dir: number) {
  control(dir > 0 ? 'next' : 'prev');
  setTimeout(poll, 400);
  setTimeout(poll, 1200);
}

export function seek(ms: number) {
  const target = Math.max(0, Math.min(ms, musicState.durationMs || Number.MAX_SAFE_INTEGER));
  lastSeekAction = Date.now();
  setAnchor(target, Date.now(), musicState.isPlaying ? rate || 1 : 0);
  musicApi.seek(target).catch(() => {});
}

export const playIcon = computed(() => (musicState.isPlaying ? 'fa-pause' : 'fa-play'));

export const marqueeText = computed(
  () => musicState.currentTrack + (musicState.currentArtist ? '  •  ' + musicState.currentArtist : '')
);

export const hasTimeline = computed(() => musicState.durationMs > 0);

export const seekable = computed(() => musicState.seekSupported && hasTimeline.value);

/** 未到首句为 -1 */
export const lyricIndex = computed(() =>
  lyricIndexAt(musicState.lyricLines, musicState.positionMs + builtinLyricOffset(musicState.currentSourceApp))
);

export const currentLyric = computed(() => musicState.lyricLines[lyricIndex.value]?.text ?? '');
