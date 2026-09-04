import { reactive } from 'vue';
import { api } from '../api';
import { settings } from './settings';
import type { LyricLine, MusicAction, MusicState } from '../../shared/ipc';

/**
 * QQ 音乐的 SMTC 时间轴上报比实际播放落后约 400ms，协议侧无法修正。匹配当前歌词行时把进度往后补 400ms，否则歌词始终慢半拍。
 */
export function builtinLyricOffset(sourceAppId: string): number {
  return sourceAppId.toLowerCase().includes('qqmusic') ? 400 : 0;
}

/** 音乐播放状态。组件模板直读字段（reactive 自动追踪）；
 *  组件侧写 isScrubbing/positionMs（拖动进度条）直接改 musicState 的字段 */
export const musicState = reactive({
  isPlaying: false,
  hasMusic: false,
  currentTrack: '',
  currentArtist: '',
  currentSourceApp: '',
  artworkUrl: '',
  // 进度（毫秒）。SMTC 上报稀疏，poll 间隙本地按时间外推
  positionMs: 0,
  durationMs: 0,
  /** SMTC 无时间轴时来自歌词源的估算时长；进度为纯本地计时（从检测到切歌起算） */
  estimatedDurationMs: 0,
  lyricLines: [] as LyricLine[],
  /** 会话是否支持外部 seek（网易云等为 false，进度条只显示不可拖） */
  seekSupported: false,
  /** 用户正在拖动进度条：暂停同步与外推 */
  isScrubbing: false,
});

/** 派生函数的入参：直接传 musicState（Readonly 只是提醒别在派生里写） */
export type MusicStateView = Readonly<typeof musicState>;

// 纯内部簿记，不参与渲染，不进 proxy
let lastPlayAction = 0;
let lastSeekAction = 0;
let missCount = 0;
let pollTimer: number | null = null;
let tickTimer: number | null = null;
let lastTickAt = 0;
let artworkHashLoaded = '';
let lyricsIdLoaded = '';

function clearState() {
  musicState.hasMusic = false;
  musicState.currentTrack = '';
  musicState.currentArtist = '';
  musicState.currentSourceApp = '';
  musicState.isPlaying = false;
  musicState.artworkUrl = '';
  artworkHashLoaded = '';
  musicState.positionMs = 0;
  musicState.durationMs = 0;
  musicState.estimatedDurationMs = 0;
  musicState.lyricLines = [];
  lyricsIdLoaded = '';
  musicState.seekSupported = false;
}

function loadLyrics(id: string) {
  lyricsIdLoaded = id;
  api
    .musicLyrics(id)
    .then((data) => {
      if (data && lyricsIdLoaded === id) musicState.lyricLines = data.lines;
    })
    .catch(() => {});
}

function loadArtwork(hash: string) {
  artworkHashLoaded = hash;
  api
    .musicArtwork(hash)
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
  const trackChanged =
    musicState.hasMusic && (t !== musicState.currentTrack || a !== musicState.currentArtist);
  musicState.currentTrack = t;
  musicState.currentArtist = a;
  musicState.currentSourceApp = data.sourceAppId || '';
  musicState.hasMusic = true;
  // 刚手动操作过播放/暂停时，短时间内以本地状态为准，避免轮询回跳
  if (Date.now() - lastPlayAction > 2000) {
    musicState.isPlaying = data.isPlaying;
  }

  musicState.durationMs = data.durationMs || 0;
  musicState.estimatedDurationMs = data.estimatedDurationMs || 0;
  musicState.seekSupported = !!data.seekSupported;
  const smtcPos = data.positionMs || 0;
  if (trackChanged) {
    musicState.positionMs = smtcPos;
    musicState.artworkUrl = '';
    artworkHashLoaded = '';
    musicState.lyricLines = [];
    lyricsIdLoaded = '';
  } else if (data.durationMs && !musicState.isScrubbing && Date.now() - lastSeekAction > 3000) {
    // 仅在真实时间轴存在时回同步（估算模式上报位置恒为 0，会把本地计时拽回去）。
    // 分级收敛：暂停/大偏差直接对齐；中等偏差每次收一半（几个 poll 内归零）；
    // <150ms 视为噪声不动。但不能留“永不纠正”的死区，否则瞬时误差会固化成整首歌的歌词滞后。
    const diff = smtcPos - musicState.positionMs;
    if (!data.isPlaying || Math.abs(diff) > 600) {
      musicState.positionMs = smtcPos;
    } else if (Math.abs(diff) > 150) {
      musicState.positionMs += diff / 2;
    }
  }

  if (data.artworkHash && data.artworkHash !== artworkHashLoaded) {
    loadArtwork(data.artworkHash);
  }
  if (data.lyricsId && data.lyricsId !== lyricsIdLoaded) {
    loadLyrics(data.lyricsId);
  }
}

async function poll() {
  if (!settings.diagnostics.musicPoll) return;
  try {
    handleState(await api.musicPoll());
  } catch {}
}

function tick() {
  const now = Date.now();
  const elapsed = now - lastTickAt;
  lastTickAt = now;
  if (!musicState.isPlaying || musicState.isScrubbing) return;
  const cap = musicState.durationMs || musicState.estimatedDurationMs;
  musicState.positionMs =
    cap > 0 ? Math.min(musicState.positionMs + elapsed, cap) : musicState.positionMs + elapsed;
}

export function startMusicPoll() {
  stopMusicPoll();
  // 事件通道：SMTC 变化（播放/暂停/切歌）由主进程即时推送，不等下个轮询周期；
  // 2s 轮询保留作兜底并驱动非事件源（网易云 elog 进度等）
  api.onMusicState((state) => {
    if (settings.diagnostics.musicPoll) handleState(state);
  });
  poll();
  pollTimer = window.setInterval(poll, 2000);
  lastTickAt = Date.now();
  // 100ms：歌词切行的量化延迟上限。250ms 时平均多 ~125ms 滞后，肉眼可感
  tickTimer = window.setInterval(tick, 100);
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
}

function control(action: MusicAction, level?: number) {
  api.musicControl(action, level).catch(() => {});
}

export function togglePlay() {
  control(musicState.isPlaying ? 'pause' : 'play');
  musicState.isPlaying = !musicState.isPlaying;
  lastPlayAction = Date.now();
}

export function pauseIfPlaying(): boolean {
  if (!musicState.isPlaying) return false;
  control('pause');
  musicState.isPlaying = false;
  lastPlayAction = Date.now();
  return true;
}

export function resumePlay() {
  if (musicState.isPlaying) return;
  control('play');
  musicState.isPlaying = true;
  lastPlayAction = Date.now();
}

export function skipTrack(dir: number) {
  control(dir > 0 ? 'next' : 'prev');
  setTimeout(poll, 400);
  setTimeout(poll, 1200);
}

/** 拖动进度条改变实际播放位置（仅在源支持 seek 时可用） */
export function seek(ms: number) {
  const target = Math.max(0, Math.min(ms, musicState.durationMs || musicState.estimatedDurationMs));
  musicState.positionMs = target;
  lastSeekAction = Date.now();
  api.musicSeek(target).catch(() => {});
}

/* ---- 响应式派生：组件里包 computed(fn(state)) 用 ---- */

export function playIcon(state: MusicStateView): string {
  return state.isPlaying ? 'fa-pause' : 'fa-play';
}

export function marqueeText(state: MusicStateView): string {
  return state.currentTrack + (state.currentArtist ? '  •  ' + state.currentArtist : '');
}

/** 当前源是否上报时间轴（旧版网易云等不上报 -> 进度条禁用） */
export function timelineAvailable(state: MusicStateView): boolean {
  return state.durationMs > 0;
}
/** 进度条是否可拖动：会话声明支持 seek 且有真实时间轴 */
export function seekable(state: MusicStateView): boolean {
  return state.seekSupported && timelineAvailable(state);
}
/** 有效时长：真实时间轴优先，其次歌词源估算 */
export function effectiveDurationMs(state: MusicStateView): number {
  return state.durationMs || state.estimatedDurationMs;
}
export function progressAvailable(state: MusicStateView): boolean {
  return effectiveDurationMs(state) > 0;
}

/** 当前歌词行索引（按位置取最后一条已到时间的）；无歌词/未到首句为 -1 */
export function currentLyricIndex(state: MusicStateView): number {
  const lines = state.lyricLines;
  if (!lines.length) return -1;
  const pos = state.positionMs + builtinLyricOffset(state.currentSourceApp);
  let idx = -1;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].timeMs > pos) break;
    idx = i;
  }
  return idx;
}

export function currentLyric(state: MusicStateView): string {
  return state.lyricLines[currentLyricIndex(state)]?.text ?? '';
}

export function formatTimeMs(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return m + ':' + String(s).padStart(2, '0');
}
