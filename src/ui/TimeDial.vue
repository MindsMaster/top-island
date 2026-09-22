<script setup lang="ts">
import { computed, ref } from 'vue';

const props = defineProps<{ mode: 'hour' | 'minute'; modelValue: number }>();
const emit = defineEmits<{
  (e: 'update:modelValue', value: number): void;
  /** 松手提交 */
  (e: 'commit'): void;
}>();

const SIZE = 180;
const C = SIZE / 2;
const R_OUTER = 71;
const R_INNER = 45;

const dragging = ref(false);

function ringRadius(v: number): number {
  if (props.mode === 'minute') return R_OUTER;
  return v < 12 ? R_OUTER : R_INNER;
}

function angleOf(v: number): number {
  // 顶部为 0 顺时针
  return props.mode === 'minute' ? v * 6 : (v % 12) * 30;
}

function posOf(v: number, r?: number) {
  const rad = (angleOf(v) * Math.PI) / 180;
  const radius = r ?? ringRadius(v);
  return { x: C + radius * Math.sin(rad), y: C - radius * Math.cos(rad) };
}

const numbers = computed(() => {
  if (props.mode === 'hour') {
    return Array.from({ length: 24 }, (_, v) => ({ v, label: String(v), ...posOf(v) }));
  }
  return Array.from({ length: 12 }, (_, i) => {
    const v = i * 5;
    return { v, label: String(v).padStart(2, '0'), ...posOf(v) };
  });
});

const knob = computed(() => posOf(props.modelValue));

function isSelected(v: number): boolean {
  return v === props.modelValue;
}

function apply(e: PointerEvent) {
  const rect = (e.currentTarget as SVGElement).getBoundingClientRect();
  const dx = e.clientX - rect.left - C;
  const dy = e.clientY - rect.top - C;
  let angle = (Math.atan2(dx, -dy) * 180) / Math.PI;
  if (angle < 0) angle += 360;
  if (props.mode === 'minute') {
    emit('update:modelValue', Math.round(angle / 6) % 60);
    return;
  }
  const idx = Math.round(angle / 30) % 12;
  const dist = Math.hypot(dx, dy);
  const inner = dist < (R_OUTER + R_INNER) / 2;
  emit('update:modelValue', inner ? idx + 12 : idx);
}

function onDown(e: PointerEvent) {
  (e.currentTarget as SVGElement).setPointerCapture?.(e.pointerId);
  dragging.value = true;
  apply(e);
}

function onMove(e: PointerEvent) {
  if (dragging.value) apply(e);
}

function onUp() {
  if (!dragging.value) return;
  dragging.value = false;
  emit('commit');
}
</script>

<template>
  <svg
    class="time-dial"
    :width="SIZE"
    :height="SIZE"
    :viewBox="`0 0 ${SIZE} ${SIZE}`"
    @pointerdown="onDown"
    @pointermove="onMove"
    @pointerup="onUp"
    @pointercancel="onUp"
  >
    <circle class="dial-bg" :cx="C" :cy="C" :r="C - 2" />
    <line class="dial-hand" :x1="C" :y1="C" :x2="knob.x" :y2="knob.y" />
    <circle class="dial-knob" :cx="knob.x" :cy="knob.y" r="14" />
    <circle class="dial-center" :cx="C" :cy="C" r="3" />
    <text
      v-for="n in numbers"
      :key="n.v"
      :x="n.x"
      :y="n.y"
      class="dial-num"
      :class="{ sel: isSelected(n.v), inner: mode === 'hour' && n.v >= 12 }"
    >
      {{ n.label }}
    </text>
  </svg>
</template>
