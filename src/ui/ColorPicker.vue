<script setup lang="ts">
import { computed, ref, watch } from 'vue';

const props = defineProps<{ modelValue: string }>();
const emit = defineEmits<{ (e: 'update:modelValue', value: string): void }>();

const PRESETS = [
  '#ff5b8f',
  '#ff7a5c',
  '#ffc857',
  '#5bd68f',
  '#38bdf8',
  '#5b8cff',
  '#8b6cf0',
  '#e05bd8',
  '#23252b',
  '#f5f2ec',
];

const h = ref(0);
const s = ref(1);
const v = ref(1);
const dragging = ref(false);

function hexToHsv(hex: string): [number, number, number] | null {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const n = parseInt(m[1], 16);
  const r = ((n >> 16) & 0xff) / 255;
  const g = ((n >> 8) & 0xff) / 255;
  const b = (n & 0xff) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  let hh = 0;
  if (d !== 0) {
    if (max === r) hh = ((g - b) / d) % 6;
    else if (max === g) hh = (b - r) / d + 2;
    else hh = (r - g) / d + 4;
    hh *= 60;
    if (hh < 0) hh += 360;
  }
  return [hh, max === 0 ? 0 : d / max, max];
}

function hsvToHex(hh: number, ss: number, vv: number): string {
  const c = vv * ss;
  const x = c * (1 - Math.abs(((hh / 60) % 2) - 1));
  const m = vv - c;
  let r = 0,
    g = 0,
    b = 0;
  if (hh < 60) [r, g, b] = [c, x, 0];
  else if (hh < 120) [r, g, b] = [x, c, 0];
  else if (hh < 180) [r, g, b] = [0, c, x];
  else if (hh < 240) [r, g, b] = [0, x, c];
  else if (hh < 300) [r, g, b] = [x, 0, c];
  else [r, g, b] = [c, 0, x];
  const to2 = (u: number) =>
    Math.round((u + m) * 255)
      .toString(16)
      .padStart(2, '0');
  return `#${to2(r)}${to2(g)}${to2(b)}`;
}

watch(
  () => props.modelValue,
  (hex) => {
    if (dragging.value) return;
    const hsv = hexToHsv(hex);
    if (hsv) [h.value, s.value, v.value] = hsv;
  },
  { immediate: true }
);

function emitColor() {
  emit('update:modelValue', hsvToHex(h.value, s.value, v.value));
}

function clamp01(n: number) {
  return Math.max(0, Math.min(1, n));
}

// 饱和度/明度面板
function svApply(e: PointerEvent) {
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  s.value = clamp01((e.clientX - rect.left) / rect.width);
  v.value = 1 - clamp01((e.clientY - rect.top) / rect.height);
  emitColor();
}

// 色相条
function hueApply(e: PointerEvent) {
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  h.value = clamp01((e.clientX - rect.left) / rect.width) * 359.99;
  emitColor();
}

function makeDragHandlers(apply: (e: PointerEvent) => void) {
  return {
    down(e: PointerEvent) {
      (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
      dragging.value = true;
      apply(e);
    },
    move(e: PointerEvent) {
      if (dragging.value) apply(e);
    },
    up() {
      dragging.value = false;
    },
  };
}

const svDrag = makeDragHandlers(svApply);
const hueDrag = makeDragHandlers(hueApply);

function pickPreset(hex: string) {
  const hsv = hexToHsv(hex);
  if (hsv) [h.value, s.value, v.value] = hsv;
  emit('update:modelValue', hex);
}

const hueColor = computed(() => `hsl(${h.value}, 100%, 50%)`);
const hex = computed(() => hsvToHex(h.value, s.value, v.value));

// hex 手动输入（精调）
const hexDraft = ref('');
const hexEditing = ref(false);

watch(
  hex,
  (val) => {
    if (!hexEditing.value) hexDraft.value = val;
  },
  { immediate: true }
);

function commitHex() {
  hexEditing.value = false;
  let t = hexDraft.value.trim();
  if (t && !t.startsWith('#')) t = '#' + t;
  const hsv = hexToHsv(t);
  if (hsv) {
    [h.value, s.value, v.value] = hsv;
    emitColor();
  }
  hexDraft.value = hex.value;
}
</script>

<template>
  <div class="color-picker">
    <div
      class="cp-sv"
      :style="{ backgroundColor: hueColor }"
      @pointerdown="svDrag.down"
      @pointermove="svDrag.move"
      @pointerup="svDrag.up"
      @pointercancel="svDrag.up"
    >
      <div
        class="cp-sv-thumb"
        :style="{ left: s * 100 + '%', top: (1 - v) * 100 + '%', background: hex }"
      ></div>
    </div>
    <div
      class="cp-hue"
      @pointerdown="hueDrag.down"
      @pointermove="hueDrag.move"
      @pointerup="hueDrag.up"
      @pointercancel="hueDrag.up"
    >
      <div class="cp-hue-thumb" :style="{ left: (h / 360) * 100 + '%', background: hueColor }"></div>
    </div>
    <div class="cp-presets">
      <button
        v-for="p in PRESETS"
        :key="p"
        class="cp-preset"
        :style="{ background: p }"
        @click="pickPreset(p)"
      ></button>
    </div>
    <input
      v-model="hexDraft"
      class="cp-hex-input"
      maxlength="7"
      spellcheck="false"
      @focus="hexEditing = true"
      @keydown.enter="($event.target as HTMLInputElement).blur()"
      @blur="commitHex"
    />
  </div>
</template>
