import { computed, reactive, watch } from 'vue';
import { alarmApi } from '@/platform/alarm';
import { storeApi } from '@/platform/store';
import { showAlert, dismissAlert } from '@/ui/alert';
import { emit as emitAppEvent } from '@/core/bus';
import { useI18n } from '@/core/i18n';
import type { AlarmSound } from '@/platform/types';

const { t } = useI18n();

export interface AlarmItem {
  id: string;
  /** 'HH:MM' */
  time: string;
  enabled: boolean;
  label?: string;
  /** 0=周日 6=周六 空为每天 */
  days?: number[];
}

interface AlarmStore {
  alarms: AlarmItem[];
  sound: AlarmSound | null;
}

export const alarmState = reactive({
  alarms: [] as AlarmItem[],
  sound: null as AlarmSound | null,
  defaultSounds: [] as AlarmSound[],
  countdown: {
    running: false,
    paused: false,
    endAt: 0,
    frozenRemainMs: 0,
    totalMs: 0,
    pickMinutes: 10,
  },
  ringing: null as null | { kind: 'alarm' | 'countdown'; label: string },
  /** 每秒 tick 驱动下面的派生 computed */
  now: Date.now(),
});

let tickTimer: number | null = null;
const lastFired = new Map<string, string>();
let audioEl: HTMLAudioElement | null = null;
const soundCache = new Map<string, string>();
let ringTimeout: number | null = null;

/** now 每秒变会触发 watch 只有 alarms/sound 变了才写盘 */
let lastSavedJson = '';

function save() {
  const json = JSON.stringify({ alarms: alarmState.alarms, sound: alarmState.sound });
  if (json === lastSavedJson) return;
  lastSavedJson = json;
  // reactive 代理不可结构化克隆 须先落成普通对象
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

const SNOOZE_MS = 10 * 60000;
let snoozeTimer: number | null = null;

function fire(kind: 'alarm' | 'countdown', label: string) {
  alarmState.ringing = { kind, label };
  emitAppEvent('alarm:ringing', true);
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
  // 无人处理 60s 自动停
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
  emitAppEvent('alarm:ringing', false);
}

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
  // 同一分钟只触发一次
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

const remainMs = computed(() => {
  const cd = alarmState.countdown;
  if (!cd.running) return cd.pickMinutes * 60000;
  if (cd.paused) return cd.frozenRemainMs;
  return Math.max(0, cd.endAt - alarmState.now);
});

export const progress = computed(() => {
  const cd = alarmState.countdown;
  if (!cd.running || cd.totalMs <= 0) return 0;
  return 1 - remainMs.value / cd.totalMs;
});

export const displayRemain = computed(() => {
  const total = Math.ceil(remainMs.value / 1000);
  const hh = Math.floor(total / 3600);
  const mm = Math.floor((total % 3600) / 60);
  const ss = total % 60;
  const p = (n: number) => String(n).padStart(2, '0');
  return hh > 0 ? `${hh}:${p(mm)}:${p(ss)}` : `${p(mm)}:${p(ss)}`;
});

/** 与 Panel.vue 进度环的 r=45 一致 */
export const ringDash = computed(() => {
  const circumference = 2 * Math.PI * 45;
  return { circumference, offset: circumference * (1 - progress.value) };
});

export const keepInteractive = computed(() => alarmState.countdown.running || alarmState.ringing !== null);
