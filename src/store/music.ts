import { reactive } from 'vue';
import { musicApi } from '@/platform/music';
import { settings } from './settings';
import type { LyricLine, MusicAction, MusicState } from '@/platform/types';
import {
  builtinLyricOffset,
  extrapolate as extrapolateAnchor,
  formatTimeMs,
  lyricIndexAt,
  trackIdentity,
} from './musicMath';

export { builtinLyricOffset, formatTimeMs };

/** positionMs 由 tick 从锚点外推；组件拖动进度条时直接写 isScrubbing/positionMs */
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

export type MusicStateView = Readonly<typeof musicState>;

let lastPlayAction = 0;
let lastSeekAction = 0;
let missCount = 0;
let pollTimer: number | null = null;
let tickTimer: number | null = null;
let watchdogTimer: number | null = null;
let artworkHashLoaded = '';
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
  artworkHashLoaded = '';
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

function loadArtwork(hash: string) {
  artworkHashLoaded = hash;
  musicApi
    .artwork(hash)
    .then((art) => {
      if (art && artworkHashLoaded === hash) musicState.artworkUrl = art.dataUrl;
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

  // 刚点过播放/暂停的 2s 内以本地为准，免得轮询回跳；切了歌本地那次点击就作废
  const holdPlayState = !trackChanged && Date.now() - lastPlayAction < 2000;
  if (!holdPlayState) musicState.isPlaying = data.isPlaying;

  musicState.durationMs = data.durationMs || 0;
  musicState.seekSupported = !!data.seekSupported;

  if (data.artworkUrl) {
    if (data.artworkUrl !== musicState.artworkUrl) musicState.artworkUrl = data.artworkUrl;
    artworkHashLoaded = '';
  } else if (data.artworkHash && data.artworkHash !== artworkHashLoaded) {
    loadArtwork(data.artworkHash);
  }

  if (trackChanged) {
    musicState.artworkUrl = data.artworkUrl || '';
    artworkHashLoaded = '';
    musicState.lyricLines = [];
    lyricsIdLoaded = '';
  }

  // 拖动中或刚 seek 过时保留本地锚点，别被服务端还没追上的旧位置拽回去
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

// webview 被后台节流后 setInterval 可能长时间不触发，心跳停了就重建
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
  // 100ms：歌词切行的量化延迟上限。250ms 时平均多 ~125ms 滞后，肉眼可感
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
  // 本地立即改速率，进度不跳变：以当前外推位置重设锚点
  setAnchor(extrapolate(Date.now()), Date.now(), next ? rate || 1 : 0);
}

export function pauseIfPlaying(): boolean {
  if (!musicState.isPlaying) return false;
  control('pause');
  musicState.isPlaying = false;
  lastPlayAction = Date.now();
  setAnchor(extrapolate(Date.now()), Date.now(), 0);
  return true;
}

export function resumePlay() {
  if (musicState.isPlaying) return;
  control('play');
  musicState.isPlaying = true;
  lastPlayAction = Date.now();
  setAnchor(extrapolate(Date.now()), Date.now(), rate || 1);
}

export function skipTrack(dir: number) {
  control(dir > 0 ? 'next' : 'prev');
  setTimeout(poll, 400);
  setTimeout(poll, 1200);
}

/** 拖动进度条改变实际播放位置（仅在源支持 seek 时可用） */
export function seek(ms: number) {
  const target = Math.max(0, Math.min(ms, musicState.durationMs || Number.MAX_SAFE_INTEGER));
  lastSeekAction = Date.now();
  setAnchor(target, Date.now(), musicState.isPlaying ? rate || 1 : 0);
  musicApi.seek(target).catch(() => {});
}

/* ---- 响应式派生：组件里包 computed(fn(state)) 用 ---- */

export function playIcon(state: MusicStateView): string {
  return state.isPlaying ? 'fa-pause' : 'fa-play';
}

export function marqueeText(state: MusicStateView): string {
  return state.currentTrack + (state.currentArtist ? '  •  ' + state.currentArtist : '');
}

/** 当前源是否上报时间轴（无时间轴源 -> 进度条禁用） */
export function timelineAvailable(state: MusicStateView): boolean {
  return state.durationMs > 0;
}
/** 进度条是否可拖动：会话声明支持 seek 且有真实时间轴 */
export function seekable(state: MusicStateView): boolean {
  return state.seekSupported && timelineAvailable(state);
}
/** 有效时长（ms）：权威时间轴 */
export function effectiveDurationMs(state: MusicStateView): number {
  return state.durationMs;
}
export function progressAvailable(state: MusicStateView): boolean {
  return effectiveDurationMs(state) > 0;
}

/** 当前歌词行索引；无歌词/未到首句为 -1 */
export function currentLyricIndex(state: MusicStateView): number {
  return lyricIndexAt(state.lyricLines, state.positionMs + builtinLyricOffset(state.currentSourceApp));
}

export function currentLyric(state: MusicStateView): string {
  return state.lyricLines[currentLyricIndex(state)]?.text ?? '';
}
