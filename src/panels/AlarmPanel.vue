<script setup lang="ts">
import { computed, ref } from 'vue';
import { useI18n } from '../i18n';
import {
  alarmState,
  displayRemainOf,
  ringDashOf,
  upsertAlarm,
  removeAlarm,
  toggleAlarm,
  setPickMinutes,
  startCountdown,
  pauseCountdown,
  cancelCountdown,
  preview,
  pickCustomSound,
} from '../store/alarm';
import type { AlarmItem } from '../store/alarm';
import SettingSelect from '../components/SettingSelect.vue';
import TimeDial from '../components/TimeDial.vue';

const { t, weekdays } = useI18n();
const snap = alarmState;

const ringDash = computed(() => ringDashOf(snap));
const displayRemain = computed(() => displayRemainOf(snap));
/** snap.alarms 是深只读快照，这里转回可变类型供 openEdit/daysSummary 使用 */
const alarmList = computed(() => snap.alarms as AlarmItem[]);

const mode = ref<'countdown' | 'clock'>('countdown');

const PRESETS = [5, 10, 15, 30, 60];
/** 直输上限（分钟）；滑杆仍为 1-120 的快捷区间 */
const COUNTDOWN_MAX = 720;

function onPickMinutes(e: Event) {
  setPickMinutes(parseInt((e.target as HTMLInputElement).value));
}

function onMinutesTyped(e: Event) {
  const el = e.target as HTMLInputElement;
  const v = parseInt(el.value.replace(/\D/g, ''));
  setPickMinutes(Number.isFinite(v) ? Math.max(1, Math.min(COUNTDOWN_MAX, v)) : 1);
  el.value = String(alarmState.countdown.pickMinutes);
}

interface EditorState {
  id: string | null;
  h: number;
  m: number;
  days: number[];
  label: string;
}

const editor = ref<EditorState | null>(null);
const selecting = ref<'hour' | 'minute'>('hour');

function openAdd() {
  editor.value = { id: null, h: 8, m: 0, days: [], label: '' };
  selecting.value = 'hour';
}

function openEdit(a: AlarmItem) {
  const [h, m] = a.time.split(':').map(Number);
  editor.value = { id: a.id, h, m, days: [...(a.days ?? [])], label: a.label ?? '' };
  selecting.value = 'hour';
}

function toggleDay(d: number) {
  if (!editor.value) return;
  const days = editor.value.days;
  editor.value.days = days.includes(d) ? days.filter((x) => x !== d) : [...days, d].sort();
}

function saveEditor() {
  if (!editor.value) return;
  const { id, h, m, days, label } = editor.value;
  upsertAlarm({
    id,
    time: `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`,
    label: label.trim(),
    days,
  });
  editor.value = null;
}

function deleteFromEditor() {
  if (editor.value?.id) removeAlarm(editor.value.id);
  editor.value = null;
}

const dialValue = computed({
  get: () => (selecting.value === 'hour' ? (editor.value?.h ?? 0) : (editor.value?.m ?? 0)),
  set: (v: number) => {
    if (!editor.value) return;
    if (selecting.value === 'hour') editor.value.h = v;
    else editor.value.m = v;
  },
});

function onDialCommit() {
  if (selecting.value === 'hour') selecting.value = 'minute';
}

const hourEl = ref<HTMLInputElement | null>(null);
const minuteEl = ref<HTMLInputElement | null>(null);
let segBuf = '';

function setSeg(which: 'hour' | 'minute', v: number) {
  if (!editor.value) return;
  if (which === 'hour') editor.value.h = v;
  else editor.value.m = v;
}

function segAdvance(which: 'hour' | 'minute') {
  segBuf = '';
  if (which === 'hour') minuteEl.value?.focus();
  else minuteEl.value?.blur();
}

function onSegFocus(which: 'hour' | 'minute') {
  selecting.value = which;
  segBuf = '';
}

function onSegKeydown(which: 'hour' | 'minute', e: KeyboardEvent) {
  if (!editor.value) return;
  const max = which === 'hour' ? 23 : 59;
  const cur = which === 'hour' ? editor.value.h : editor.value.m;

  if (e.key >= '0' && e.key <= '9') {
    e.preventDefault();
    const d = parseInt(e.key);
    if (segBuf === '') {
      setSeg(which, d);
      // 首位已排除第二位可能（小时 3-9 / 分钟 6-9）→ 定值并跳段
      if (d > Math.floor(max / 10)) segAdvance(which);
      else segBuf = e.key;
    } else {
      setSeg(which, Math.min(max, parseInt(segBuf + e.key)));
      segAdvance(which);
    }
    return;
  }
  if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
    e.preventDefault();
    const delta = e.key === 'ArrowUp' ? 1 : -1;
    setSeg(which, (cur + delta + max + 1) % (max + 1));
    segBuf = '';
    return;
  }
  if (e.key === 'Backspace' || e.key === 'Delete') {
    e.preventDefault();
    setSeg(which, 0);
    segBuf = '';
    return;
  }
  if (e.key === 'Enter') {
    (e.target as HTMLInputElement).blur();
    return;
  }
  // 除 Tab 等导航键外屏蔽其余字符输入
  if (e.key.length === 1) e.preventDefault();
}

function daysSummary(a: AlarmItem): string {
  if (!a.days || a.days.length === 0 || a.days.length === 7) return t('alarmEveryday');
  return a.days.map((d) => weekdays.value[d]).join(' ');
}

const CUSTOM_VALUE = '__custom__';

const soundOptions = computed(() => [
  ...snap.defaultSounds.map((s) => ({ value: s.path, label: s.name, icon: 'fa-bell' })),
  ...(snap.sound && !snap.defaultSounds.some((s) => s.path === snap.sound?.path)
    ? [{ value: snap.sound.path, label: snap.sound.name, icon: 'fa-file-audio' }]
    : []),
  { value: CUSTOM_VALUE, label: t('alarmSoundCustom'), icon: 'fa-folder-open' },
]);

function onSoundPick(value: string) {
  if (value === CUSTOM_VALUE) {
    void pickCustomSound();
    return;
  }
  const found =
    alarmState.defaultSounds.find((s) => s.path === value) ??
    (alarmState.sound?.path === value ? alarmState.sound : null);
  if (found) alarmState.sound = found;
}
</script>

<template>
  <div class="alarm-mode-select">
    <button class="alarm-mode-btn" :class="{ active: mode === 'countdown' }" @click.stop="mode = 'countdown'">
      <i class="fa-solid fa-hourglass-half"></i>{{ t('alarmCountdown') }}
    </button>
    <button class="alarm-mode-btn" :class="{ active: mode === 'clock' }" @click.stop="mode = 'clock'">
      <i class="fa-solid fa-bell"></i>{{ t('alarmClock') }}
    </button>
  </div>

  <template v-if="mode === 'countdown'">
    <div class="alarm-ring-wrap">
      <svg class="alarm-ring" viewBox="0 0 104 104">
        <circle cx="52" cy="52" r="45" fill="none" stroke="rgba(128,128,128,0.18)" stroke-width="6" />
        <circle
          cx="52"
          cy="52"
          r="45"
          fill="none"
          stroke="var(--accent)"
          stroke-width="6"
          stroke-linecap="round"
          :stroke-dasharray="ringDash.circumference"
          :stroke-dashoffset="ringDash.offset"
          transform="rotate(-90 52 52)"
        />
      </svg>
      <div class="alarm-ring-center">
        <span class="alarm-time-display">{{ displayRemain }}</span>
        <span v-if="snap.countdown.paused" class="alarm-state-label">{{ t('alarmPaused') }}</span>
      </div>
    </div>

    <template v-if="!snap.countdown.running">
      <div class="alarm-presets">
        <button
          v-for="p in PRESETS"
          :key="p"
          class="alarm-preset"
          :class="{ active: snap.countdown.pickMinutes === p }"
          @click.stop="setPickMinutes(p)"
        >
          {{ p }}{{ t('alarmMin') }}
        </button>
      </div>
      <div class="alarm-duration-row">
        <input
          type="range"
          class="alarm-slider"
          min="1"
          max="120"
          step="1"
          :value="Math.min(snap.countdown.pickMinutes, 120)"
          @input="onPickMinutes"
        />
        <input
          class="alarm-min-input"
          inputmode="numeric"
          maxlength="3"
          :value="snap.countdown.pickMinutes"
          spellcheck="false"
          @click.stop
          @focus="($event.target as HTMLInputElement).select()"
          @mouseup.prevent="($event.target as HTMLInputElement).select()"
          @keydown.enter="($event.target as HTMLInputElement).blur()"
          @blur="onMinutesTyped"
        />
        <span class="alarm-min-unit">{{ t('alarmMin') }}</span>
      </div>
      <button class="alarm-action-btn" @click.stop="startCountdown(snap.countdown.pickMinutes)">
        {{ t('alarmStart') }}
      </button>
    </template>
    <div v-else class="alarm-running-actions">
      <button class="alarm-action-btn" @click.stop="pauseCountdown()">
        {{ snap.countdown.paused ? t('alarmResume') : t('alarmPause') }}
      </button>
      <button class="alarm-action-btn secondary" @click.stop="cancelCountdown()">
        {{ t('alarmCancel') }}
      </button>
    </div>
  </template>

  <template v-else>
    <div class="alarm-list">
      <div v-if="!alarmList.length" class="alarm-empty">
        <i class="fa-solid fa-bell-slash"></i>
        <span>{{ t('alarmEmpty') }}</span>
      </div>
      <div
        v-for="a in alarmList"
        :key="a.id"
        class="alarm-row"
        :class="{ off: !a.enabled }"
        @click.stop="openEdit(a)"
      >
        <div class="alarm-row-main">
          <span class="alarm-row-time">{{ a.time }}</span>
          <span class="alarm-row-sub"> {{ a.label ? a.label + ' · ' : '' }}{{ daysSummary(a) }} </span>
        </div>
        <button
          class="setting-toggle alarm-row-toggle"
          :class="{ on: a.enabled }"
          @click.stop="toggleAlarm(a.id)"
        >
          <span class="setting-toggle-knob"></span>
        </button>
      </div>
    </div>
    <button class="alarm-fab" @click.stop="openAdd">
      <i class="fa-solid fa-plus"></i>
    </button>
  </template>

  <!-- 提示音（两种模式共用；编辑抽屉打开时隐藏） -->
  <div v-if="!editor" class="alarm-sound-row">
    <SettingSelect
      :model-value="snap.sound?.path ?? ''"
      :options="soundOptions"
      @update:model-value="onSoundPick"
    />
    <button class="alarm-preview-btn" :title="t('alarmPreview')" @click.stop="preview()">
      <i class="fa-solid fa-play"></i>
    </button>
  </div>

  <!-- 编辑抽屉（覆盖整个面板） -->
  <div v-if="editor" class="alarm-editor" @click.stop>
    <div class="alarm-editor-time">
      <input
        ref="hourEl"
        class="alarm-editor-seg"
        :class="{ active: selecting === 'hour' }"
        :value="String(editor.h).padStart(2, '0')"
        readonly
        inputmode="numeric"
        spellcheck="false"
        @click.stop
        @focus="onSegFocus('hour')"
        @keydown="onSegKeydown('hour', $event)"
      />
      <span class="alarm-editor-colon">:</span>
      <input
        ref="minuteEl"
        class="alarm-editor-seg"
        :class="{ active: selecting === 'minute' }"
        :value="String(editor.m).padStart(2, '0')"
        readonly
        inputmode="numeric"
        spellcheck="false"
        @click.stop
        @focus="onSegFocus('minute')"
        @keydown="onSegKeydown('minute', $event)"
      />
    </div>

    <TimeDial v-model="dialValue" :mode="selecting" @commit="onDialCommit" />

    <div class="alarm-editor-days">
      <button
        v-for="(w, d) in weekdays"
        :key="d"
        class="alarm-day-chip"
        :class="{ on: editor.days.includes(d) }"
        @click.stop="toggleDay(d)"
      >
        {{ w }}
      </button>
    </div>

    <div class="alarm-editor-bottom">
      <input
        v-model="editor.label"
        class="alarm-add-label"
        :placeholder="t('alarmLabelPlaceholder')"
        spellcheck="false"
        @click.stop
      />
      <button v-if="editor.id" class="alarm-editor-del" @click.stop="deleteFromEditor">
        <i class="fa-solid fa-trash-can"></i>
      </button>
      <button class="alarm-editor-btn ghost" @click.stop="editor = null">{{ t('alarmCancel') }}</button>
      <button class="alarm-editor-btn" @click.stop="saveEditor">{{ t('alarmSave') }}</button>
    </div>
  </div>
</template>
