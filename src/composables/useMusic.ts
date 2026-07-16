import { computed, ref } from 'vue';
import { api } from '../api';
import { useSettings } from './useSettings';
import type { LyricLine, MusicAction, MusicState } from '../../shared/ipc';

/**
 * 不确定的偏差 个人测试补偿400延迟
 */
function builtinLyricOffset(sourceAppId: string): number {
  return sourceAppId.toLowerCase().includes('qqmusic') ? 400 : 0;
}

const isPlaying = ref(false);
const hasMusic = ref(false);
const currentTrack = ref('');
const currentArtist = ref('');
const currentSourceApp = ref('');
const artworkUrl = ref('');

// 进度（毫秒）。SMTC 上报稀疏，poll 间隙本地按时间外推
const positionMs = ref(0);
const durationMs = ref(0);
/** SMTC 无时间轴时来自歌词源的估算时长；进度为纯本地计时（从检测到切歌起算） */
const estimatedDurationMs = ref(0);
const lyricLines = ref<LyricLine[]>([]);
/** 会话是否支持外部 seek（网易云等为 false，进度条只显示不可拖） */
const seekSupported = ref(false);
/** 用户正在拖动进度条：暂停同步与外推 */
const isScrubbing = ref(false);

let lastPlayAction = 0;
let lastSeekAction = 0;
let missCount = 0;
let pollTimer: number | null = null;
let tickTimer: number | null = null;
let lastTickAt = 0;
let artworkHashLoaded = '';
let lyricsIdLoaded = '';

function clearState() {
  hasMusic.value = false;
  currentTrack.value = '';
  currentArtist.value = '';
  currentSourceApp.value = '';
  isPlaying.value = false;
  artworkUrl.value = '';
  artworkHashLoaded = '';
  positionMs.value = 0;
  durationMs.value = 0;
  estimatedDurationMs.value = 0;
  lyricLines.value = [];
  lyricsIdLoaded = '';
  seekSupported.value = false;
}

function loadLyrics(id: string) {
  lyricsIdLoaded = id;
  api
    .musicLyrics(id)
    .then((data) => {
      if (data && lyricsIdLoaded === id) lyricLines.value = data.lines;
    })
    .catch(() => {});
}

function loadArtwork(hash: string) {
  artworkHashLoaded = hash;
  api
    .musicArtwork(hash)
    .then((art) => {
      if (art && artworkHashLoaded === hash) artworkUrl.value = art.dataUrl;
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
  const trackChanged = hasMusic.value && (t !== currentTrack.value || a !== currentArtist.value);
  currentTrack.value = t;
  currentArtist.value = a;
  currentSourceApp.value = data.sourceAppId || '';
  hasMusic.value = true;
  // 刚手动操作过播放/暂停时，短时间内以本地状态为准，避免轮询回跳
  if (Date.now() - lastPlayAction > 2000) {
    isPlaying.value = data.isPlaying;
  }

  durationMs.value = data.durationMs || 0;
  estimatedDurationMs.value = data.estimatedDurationMs || 0;
  seekSupported.value = !!data.seekSupported;
  const smtcPos = data.positionMs || 0;
  if (trackChanged) {
    positionMs.value = smtcPos;
    artworkUrl.value = '';
    artworkHashLoaded = '';
    lyricLines.value = [];
    lyricsIdLoaded = '';
  } else if (data.durationMs && !isScrubbing.value && Date.now() - lastSeekAction > 3000) {
    // 仅在真实时间轴存在时回同步（估算模式上报位置恒为 0，会把本地计时拽回去）。
    // 分级收敛：暂停/大偏差直接对齐；中等偏差每次收一半（几个 poll 内归零）；
    // <150ms 视为噪声不动。但不能留“永不纠正”的死区，否则瞬时误差会固化成整首歌的歌词滞后。
    const diff = smtcPos - positionMs.value;
    if (!data.isPlaying || Math.abs(diff) > 600) {
      positionMs.value = smtcPos;
    } else if (Math.abs(diff) > 150) {
      positionMs.value += diff / 2;
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
  const { diagnostics } = useSettings();
  if (!diagnostics.value.musicPoll) return;
  try {
    handleState(await api.musicPoll());
  } catch {}
}

function tick() {
  const now = Date.now();
  const elapsed = now - lastTickAt;
  lastTickAt = now;
  if (!isPlaying.value || isScrubbing.value) return;
  const cap = durationMs.value || estimatedDurationMs.value;
  positionMs.value = cap > 0 ? Math.min(positionMs.value + elapsed, cap) : positionMs.value + elapsed;
}

function startMusicPoll() {
  stopMusicPoll();
  // 事件通道：SMTC 变化（播放/暂停/切歌）由主进程即时推送，不等下个轮询周期；
  // 2s 轮询保留作兜底并驱动非事件源（网易云 elog 进度等）
  api.onMusicState((state) => {
    const { diagnostics } = useSettings();
    if (diagnostics.value.musicPoll) handleState(state);
  });
  poll();
  pollTimer = window.setInterval(poll, 2000);
  lastTickAt = Date.now();
  // 100ms：歌词切行的量化延迟上限。250ms 时平均多 ~125ms 滞后，肉眼可感
  tickTimer = window.setInterval(tick, 100);
}

function stopMusicPoll() {
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

function togglePlay() {
  control(isPlaying.value ? 'pause' : 'play');
  isPlaying.value = !isPlaying.value;
  lastPlayAction = Date.now();
}

function skipTrack(dir: number) {
  control(dir > 0 ? 'next' : 'prev');
  setTimeout(poll, 400);
  setTimeout(poll, 1200);
}

/** 拖动进度条改变实际播放位置（仅在源支持 seek 时可用） */
function seek(ms: number) {
  const target = Math.max(0, Math.min(ms, durationMs.value || estimatedDurationMs.value));
  positionMs.value = target;
  lastSeekAction = Date.now();
  api.musicSeek(target).catch(() => {});
}

const playIcon = computed(() => (isPlaying.value ? 'fa-pause' : 'fa-play'));

const marqueeText = computed(
  () => currentTrack.value + (currentArtist.value ? '  •  ' + currentArtist.value : '')
);

/** 当前源是否上报时间轴（旧版网易云等不上报 -> 进度条禁用） */
const timelineAvailable = computed(() => durationMs.value > 0);
/** 进度条是否可拖动：会话声明支持 seek 且有真实时间轴 */
const seekable = computed(() => seekSupported.value && timelineAvailable.value);
/** 有效时长：真实时间轴优先，其次歌词源估算 */
const effectiveDurationMs = computed(() => durationMs.value || estimatedDurationMs.value);
const progressAvailable = computed(() => effectiveDurationMs.value > 0);

/** 当前歌词行索引（按位置取最后一条已到时间的）；无歌词/未到首句为 -1 */
const currentLyricIndex = computed(() => {
  const lines = lyricLines.value;
  if (!lines.length) return -1;
  const pos = positionMs.value + builtinLyricOffset(currentSourceApp.value);
  let idx = -1;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].timeMs > pos) break;
    idx = i;
  }
  return idx;
});

const currentLyric = computed(() => lyricLines.value[currentLyricIndex.value]?.text ?? '');

export function formatTimeMs(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return m + ':' + String(s).padStart(2, '0');
}

export function useMusic() {
  return {
    isPlaying,
    hasMusic,
    currentTrack,
    currentArtist,
    currentSourceApp,
    artworkUrl,
    positionMs,
    durationMs,
    isScrubbing,
    timelineAvailable,
    seekable,
    effectiveDurationMs,
    progressAvailable,
    lyricLines,
    currentLyric,
    currentLyricIndex,
    playIcon,
    marqueeText,
    startMusicPoll,
    stopMusicPoll,
    togglePlay,
    skipTrack,
    seek,
  };
}
