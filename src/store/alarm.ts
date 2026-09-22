import { reactive, watch } from 'vue';
import { alarmApi } from '@/platform/alarm';
import { storeApi } from '@/platform/store';
import { showAlert, dismissAlert } from './alert';
import { pauseIfPlaying, resumePlay } from './music';
import { useI18n } from '../i18n';
import type { AlarmSound } from '@/platform/types';

const { t } = useI18n();

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

/** 闹钟/倒计时全部窗口共享状态。组件模板直读字段（reactive 自动追踪），写走本文件 action */
export const alarmState = reactive({
  alarms: [] as AlarmItem[],
  /** 当前提示音（path 为空表示未选，回退默认列表第一个） */
  sound: null as AlarmSound | null,
  defaultSounds: [] as AlarmSound[],
  countdown: {
    running: false,
    paused: false,
    /** 结束时刻（ms epoch；暂停时无效） */
    endAt: 0,
    /** 暂停时冻结的剩余 ms */
    frozenRemainMs: 0,
    totalMs: 0,
    /** 未运行时面板上选择的时长（分钟） */
    pickMinutes: 10,
  },
  ringing: null as null | { kind: 'alarm' | 'countdown'; label: string },
  /** 每秒 tick 刷新；组件里的 remainMs/progress 等派生 computed 依赖它 */
  now: Date.now(),
});

let tickTimer: number | null = null;
/** 每个闹钟最近触发的日期+分钟戳，防止同一分钟内重复触发 */
const lastFired = new Map<string, string>();
let audioEl: HTMLAudioElement | null = null;
/** path -> dataURL 缓存 */
const soundCache = new Map<string, string>();
let ringTimeout: number | null = null;

/** 最近一次已持久化的 alarms+sound（JSON）。alarmState 每秒都因 now 变化触发 subscribe，
 *  只有这两字段内容变了才写盘 */
let lastSavedJson = '';

function save() {
  const json = JSON.stringify({ alarms: alarmState.alarms, sound: alarmState.sound });
  if (json === lastSavedJson) return;
  lastSavedJson = json;
  // 深拷贝去 Proxy：Proxy 过 contextBridge 会抛 "could not be cloned"，
  // 且此错误在回调里会炸掉调度器 flush，整个界面停更
  storeApi.set('alarmData', JSON.parse(json) as AlarmStore).catch(() => {});
}

export async function initAlarm() {
  const [saved, defs] = await Promise.all([storeApi.get<AlarmStore>('alarmData'), alarmApi.listSounds()]);
  alarmState.defaultSounds = defs;
  if (saved) {
    alarmState.alarms = saved.alarms || [];
    alarmState.sound = saved.sound || null;
  }
  if (!alarmState.sound && defs.length) alarmState.sound = defs[0];
  lastSavedJson = JSON.stringify({ alarms: alarmState.alarms, sound: alarmState.sound });
  watch(alarmState, save);
  if (tickTimer === null) tickTimer = window.setInterval(tick, 1000);
}

async function soundDataUrl(path: string): Promise<string | null> {
  const cached = soundCache.get(path);
  if (cached) return cached;
  const data = await alarmApi.soundData(path);
  if (data) soundCache.set(path, data);
  return data;
}

async function playRingSound() {
  const s = alarmState.sound ?? alarmState.defaultSounds[0];
  if (!s) return;
  const data = await soundDataUrl(s.path);
  if (!data || !alarmState.ringing) return;
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
export async function preview() {
  if (alarmState.ringing) return;
  stopSound();
  const s = alarmState.sound ?? alarmState.defaultSounds[0];
  if (!s) return;
  const data = await soundDataUrl(s.path);
  if (!data) return;
  audioEl = new Audio(data);
  audioEl.play().catch(() => {});
}

export async function pickCustomSound() {
  const picked = await alarmApi.pickSound();
  if (picked) alarmState.sound = picked;
}

/** 响铃时暂停音乐，铃停后恢复（闹钟优先级高于媒体） */
let musicPausedByRing = false;
/** 延后重响（Snooze）：静铃后 10 分钟原样重响，可反复延后 */
const SNOOZE_MS = 10 * 60000;
let snoozeTimer: number | null = null;

function fire(kind: 'alarm' | 'countdown', label: string) {
  alarmState.ringing = { kind, label };
  musicPausedByRing = pauseIfPlaying();
  void playRingSound();
  showAlert({
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
  const cur = alarmState.ringing;
  stopRinging();
  if (!cur) return;
  if (snoozeTimer) clearTimeout(snoozeTimer);
  snoozeTimer = window.setTimeout(() => {
    snoozeTimer = null;
    fire(cur.kind, cur.label);
  }, SNOOZE_MS);
}

export function stopRinging() {
  stopSound();
  alarmState.ringing = null;
  if (ringTimeout) {
    clearTimeout(ringTimeout);
    ringTimeout = null;
  }
  dismissAlert();
  if (musicPausedByRing) {
    musicPausedByRing = false;
    resumePlay();
  }
}

/** 新增或更新闹钟（id 为空则新增），按时间排序 */
export function upsertAlarm(data: { id?: string | null; time: string; label: string; days: number[] }) {
  const next = data.id
    ? alarmState.alarms.map((a) =>
        a.id === data.id ? { ...a, time: data.time, label: data.label, days: [...data.days] } : a
      )
    : [
        ...alarmState.alarms,
        {
          id: Date.now().toString(36) + Math.random().toString(36).slice(2, 5),
          time: data.time,
          enabled: true,
          label: data.label,
          days: [...data.days],
        },
      ];
  alarmState.alarms = next.sort((a, b) => a.time.localeCompare(b.time));
}

export function removeAlarm(id: string) {
  alarmState.alarms = alarmState.alarms.filter((a) => a.id !== id);
}

export function toggleAlarm(id: string) {
  alarmState.alarms = alarmState.alarms.map((a) => (a.id === id ? { ...a, enabled: !a.enabled } : a));
}

export function setPickMinutes(minutes: number) {
  alarmState.countdown.pickMinutes = minutes;
}

export function startCountdown(minutes: number) {
  alarmState.countdown.totalMs = minutes * 60000;
  alarmState.countdown.endAt = Date.now() + alarmState.countdown.totalMs;
  alarmState.countdown.running = true;
  alarmState.countdown.paused = false;
}

export function pauseCountdown() {
  const cd = alarmState.countdown;
  if (!cd.running) return;
  if (cd.paused) {
    cd.endAt = Date.now() + cd.frozenRemainMs;
    cd.paused = false;
  } else {
    cd.frozenRemainMs = Math.max(0, cd.endAt - Date.now());
    cd.paused = true;
  }
}

export function cancelCountdown() {
  alarmState.countdown.running = false;
  alarmState.countdown.paused = false;
}

function tick() {
  alarmState.now = Date.now();
  const cd = alarmState.countdown;
  if (cd.running && !cd.paused && cd.endAt <= alarmState.now) {
    cd.running = false;
    fire('countdown', t('alarmCountdownDone'));
  }
  // 分钟精度；同一分钟只触发一次
  const d = new Date(alarmState.now);
  const hm = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  const stamp = `${d.toDateString()} ${hm}`;
  const dow = d.getDay();
  for (const a of alarmState.alarms) {
    if (!a.enabled || a.time !== hm || lastFired.get(a.id) === stamp) continue;
    if (a.days && a.days.length > 0 && !a.days.includes(dow)) continue;
    lastFired.set(a.id, stamp);
    fire('alarm', a.label ? `${a.label} · ${a.time}` : `${t('alarmRing')} ${a.time}`);
  }
}

/* ---------- 纯派生函数：组件里 computed(() => xxxOf(snap)) 使用 ---------- */

type AlarmView = Readonly<typeof alarmState>;

export function remainMsOf(s: AlarmView): number {
  if (!s.countdown.running) return s.countdown.pickMinutes * 60000;
  if (s.countdown.paused) return s.countdown.frozenRemainMs;
  return Math.max(0, s.countdown.endAt - s.now);
}

export function progressOf(s: AlarmView): number {
  if (!s.countdown.running || s.countdown.totalMs <= 0) return 0;
  return 1 - remainMsOf(s) / s.countdown.totalMs;
}

export function displayRemainOf(s: AlarmView): string {
  const total = Math.ceil(remainMsOf(s) / 1000);
  const hh = Math.floor(total / 3600);
  const mm = Math.floor((total % 3600) / 60);
  const ss = total % 60;
  const p = (n: number) => String(n).padStart(2, '0');
  return hh > 0 ? `${hh}:${p(mm)}:${p(ss)}` : `${p(mm)}:${p(ss)}`;
}

export function ringDashOf(s: AlarmView): { circumference: number; offset: number } {
  const r = 45;
  const circ = 2 * Math.PI * r;
  return { circumference: circ, offset: circ * (1 - progressOf(s)) };
}

/** 岛需要保持可交互（quick 倒计时显示 / 响铃处理） */
export function keepInteractiveOf(s: AlarmView): boolean {
  return s.countdown.running || s.ringing !== null;
}
