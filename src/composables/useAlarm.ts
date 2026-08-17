import { computed, reactive, ref, watch } from 'vue';
import { api } from '../api';
import { useAlert } from './useAlert';
import { useMusic } from './useMusic';
import { useI18n } from '../i18n';
import type { AlarmSound } from '../../shared/ipc';

const { t } = useI18n();
const alert = useAlert();
const music = useMusic();

export interface AlarmItem {
  id: string;
  /** 'HH:MM' */
  time: string;
  enabled: boolean;
  label?: string;
  /** 重复星期（0=周日…6=周六）；空/缺省 = 每天 */
  days?: number[];
}

interface AlarmStore {
  alarms: AlarmItem[];
  /** 当前提示音（path 为空表示未选，回退默认列表第一个） */
  sound: AlarmSound | null;
}

const alarms = ref<AlarmItem[]>([]);
const sound = ref<AlarmSound | null>(null);
const defaultSounds = ref<AlarmSound[]>([]);

const countdown = reactive({
  running: false,
  paused: false,
  /** 结束时刻（ms epoch；暂停时无效） */
  endAt: 0,
  /** 暂停时冻结的剩余 ms */
  frozenRemainMs: 0,
  totalMs: 0,
  /** 未运行时面板上选择的时长（分钟） */
  pickMinutes: 10,
});

const ringing = ref<null | { kind: 'alarm' | 'countdown'; label: string }>(null);

let tickTimer: number | null = null;
/** 每个闹钟最近触发的日期+分钟戳，防止同一分钟内重复触发 */
const lastFired = new Map<string, string>();
let audioEl: HTMLAudioElement | null = null;
/** path -> dataURL 缓存 */
const soundCache = new Map<string, string>();
let ringTimeout: number | null = null;

function save() {
  // 深拷贝去 Proxy：Proxy 过 contextBridge 会抛 "could not be cloned"，
  // 且此错误在 watcher 回调里会炸掉 Vue 调度器 flush，整个界面停更
  const data: AlarmStore = JSON.parse(JSON.stringify({ alarms: alarms.value, sound: sound.value }));
  api.storeSet('alarmData', data).catch(() => {});
}

async function initAlarm() {
  // alarmSoundList 防御：preload 版本落后时不至于中断整个初始化链
  const [saved, defs] = await Promise.all([
    api.storeGet<AlarmStore>('alarmData'),
    typeof api.alarmSoundList === 'function' ? api.alarmSoundList() : Promise.resolve([]),
  ]);
  defaultSounds.value = defs;
  if (saved) {
    alarms.value = saved.alarms || [];
    sound.value = saved.sound || null;
  }
  if (!sound.value && defs.length) sound.value = defs[0];
  watch([alarms, sound], save, { deep: true });
  if (tickTimer === null) tickTimer = window.setInterval(tick, 1000);
}

async function soundDataUrl(path: string): Promise<string | null> {
  const cached = soundCache.get(path);
  if (cached) return cached;
  const data = await api.alarmSoundData(path);
  if (data) soundCache.set(path, data);
  return data;
}

async function playRingSound() {
  const s = sound.value ?? defaultSounds.value[0];
  if (!s) return;
  const data = await soundDataUrl(s.path);
  if (!data || !ringing.value) return;
  audioEl = new Audio(data);
  audioEl.loop = true;
  audioEl.play().catch(() => {});
}

function stopSound() {
  if (audioEl) {
    audioEl.pause();
    audioEl = null;
  }
}

/** 试听（响铃中不可用；再次调用停止上一次试听） */
async function preview() {
  if (ringing.value) return;
  stopSound();
  const s = sound.value ?? defaultSounds.value[0];
  if (!s) return;
  const data = await soundDataUrl(s.path);
  if (!data) return;
  audioEl = new Audio(data);
  audioEl.play().catch(() => {});
}

async function pickCustomSound() {
  const picked = await api.alarmSoundPick();
  if (picked) sound.value = picked;
}

/** 响铃时暂停音乐，铃停后恢复（闹钟优先级高于媒体） */
let musicPausedByRing = false;
/** 延后重响（Snooze）：静铃后 10 分钟原样重响，可反复延后 */
const SNOOZE_MS = 10 * 60000;
let snoozeTimer: number | null = null;

function fire(kind: 'alarm' | 'countdown', label: string) {
  ringing.value = { kind, label };
  musicPausedByRing = music.pauseIfPlaying();
  void playRingSound();
  alert.show({
    icon: 'fa-bell',
    text: label,
    dismissible: false,
    duration: 0,
    secondLabel: t('alarmSnooze'),
    secondHandler: snooze,
    actionLabel: t('alarmStop'),
    actionHandler: stopRinging,
  });
  // 无人处理时 60s 自动停止
  if (ringTimeout) clearTimeout(ringTimeout);
  ringTimeout = window.setTimeout(stopRinging, 60000);
}

function snooze() {
  const cur = ringing.value;
  stopRinging();
  if (!cur) return;
  if (snoozeTimer) clearTimeout(snoozeTimer);
  snoozeTimer = window.setTimeout(() => {
    snoozeTimer = null;
    fire(cur.kind, cur.label);
  }, SNOOZE_MS);
}

function stopRinging() {
  stopSound();
  ringing.value = null;
  if (ringTimeout) {
    clearTimeout(ringTimeout);
    ringTimeout = null;
  }
  alert.dismiss();
  if (musicPausedByRing) {
    musicPausedByRing = false;
    music.resumePlay();
  }
}

/** 新增或更新闹钟（id 为空则新增），按时间排序 */
function upsertAlarm(data: { id?: string | null; time: string; label: string; days: number[] }) {
  const next = data.id
    ? alarms.value.map((a) =>
        a.id === data.id ? { ...a, time: data.time, label: data.label, days: [...data.days] } : a
      )
    : [
        ...alarms.value,
        {
          id: Date.now().toString(36) + Math.random().toString(36).slice(2, 5),
          time: data.time,
          enabled: true,
          label: data.label,
          days: [...data.days],
        },
      ];
  alarms.value = next.sort((a, b) => a.time.localeCompare(b.time));
}

function removeAlarm(id: string) {
  alarms.value = alarms.value.filter((a) => a.id !== id);
}

function toggleAlarm(id: string) {
  alarms.value = alarms.value.map((a) => (a.id === id ? { ...a, enabled: !a.enabled } : a));
}

function startCountdown(minutes: number) {
  countdown.totalMs = minutes * 60000;
  countdown.endAt = Date.now() + countdown.totalMs;
  countdown.running = true;
  countdown.paused = false;
}

function pauseCountdown() {
  if (!countdown.running) return;
  if (countdown.paused) {
    countdown.endAt = Date.now() + countdown.frozenRemainMs;
    countdown.paused = false;
  } else {
    countdown.frozenRemainMs = Math.max(0, countdown.endAt - Date.now());
    countdown.paused = true;
  }
}

function cancelCountdown() {
  countdown.running = false;
  countdown.paused = false;
}

const now = ref(Date.now());

function tick() {
  now.value = Date.now();
  if (countdown.running && !countdown.paused && countdown.endAt <= now.value) {
    countdown.running = false;
    fire('countdown', t('alarmCountdownDone'));
  }
  // 分钟精度；同一分钟只触发一次
  const d = new Date(now.value);
  const hm = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  const stamp = `${d.toDateString()} ${hm}`;
  const dow = d.getDay();
  for (const a of alarms.value) {
    if (!a.enabled || a.time !== hm || lastFired.get(a.id) === stamp) continue;
    if (a.days && a.days.length > 0 && !a.days.includes(dow)) continue;
    lastFired.set(a.id, stamp);
    fire('alarm', a.label ? `${a.label} · ${a.time}` : `${t('alarmRing')} ${a.time}`);
  }
}

const remainMs = computed(() => {
  if (!countdown.running) return countdown.pickMinutes * 60000;
  if (countdown.paused) return countdown.frozenRemainMs;
  return Math.max(0, countdown.endAt - now.value);
});

const progress = computed(() => {
  if (!countdown.running || countdown.totalMs <= 0) return 0;
  return 1 - remainMs.value / countdown.totalMs;
});

const displayRemain = computed(() => {
  const total = Math.ceil(remainMs.value / 1000);
  const hh = Math.floor(total / 3600);
  const mm = Math.floor((total % 3600) / 60);
  const ss = total % 60;
  const p = (n: number) => String(n).padStart(2, '0');
  return hh > 0 ? `${hh}:${p(mm)}:${p(ss)}` : `${p(mm)}:${p(ss)}`;
});

const ringDash = computed(() => {
  const r = 45;
  const circ = 2 * Math.PI * r;
  return { circumference: circ, offset: circ * (1 - progress.value) };
});

/** 岛需要保持可交互（quick 倒计时显示 / 响铃处理） */
const keepInteractive = computed(() => countdown.running || ringing.value !== null);

const soundName = computed(() => (sound.value ?? defaultSounds.value[0])?.name ?? '--');

export function useAlarm() {
  return {
    alarms,
    sound,
    soundName,
    defaultSounds,
    countdown,
    ringing,
    remainMs,
    progress,
    displayRemain,
    ringDash,
    keepInteractive,
    initAlarm,
    upsertAlarm,
    removeAlarm,
    toggleAlarm,
    startCountdown,
    pauseCountdown,
    cancelCountdown,
    stopRinging,
    preview,
    pickCustomSound,
  };
}
