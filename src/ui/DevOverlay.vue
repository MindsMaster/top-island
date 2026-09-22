<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { settings } from '@/core/settings';

/** 帧耗时采样环形缓冲，算中位/p95/最差 */
const BUF = 240;
const deltas: number[] = [];

const fps = ref(0);
const medianMs = ref(0);
const p95Ms = ref(0);
const worstMs = ref(0);
const longtasks = ref(0);

let rafId = 0;
let lastTs = 0;
let frames = 0;
let windowStart = 0;
let observer: PerformanceObserver | null = null;

function percentile(sorted: number[], p: number): number {
  if (!sorted.length) return 0;
  return sorted[Math.min(sorted.length - 1, Math.floor((sorted.length * p) / 100))];
}

function tick(ts: number) {
  if (lastTs) {
    deltas.push(ts - lastTs);
    if (deltas.length > BUF) deltas.shift();
  }
  lastTs = ts;
  frames++;
  if (ts - windowStart >= 500) {
    fps.value = Math.round((frames * 1000) / (ts - windowStart));
    const sorted = [...deltas].sort((a, b) => a - b);
    medianMs.value = Math.round(percentile(sorted, 50) * 10) / 10;
    p95Ms.value = Math.round(percentile(sorted, 95) * 10) / 10;
    worstMs.value = Math.round(sorted[sorted.length - 1] ?? 0);
    frames = 0;
    windowStart = ts;
  }
  rafId = requestAnimationFrame(tick);
}

function start() {
  if (rafId) return;
  deltas.length = 0;
  lastTs = 0;
  frames = 0;
  windowStart = performance.now();
  rafId = requestAnimationFrame(tick);
  observer = new PerformanceObserver((list) => {
    longtasks.value += list.getEntries().length;
  });
  observer.observe({ entryTypes: ['longtask'] });
}

function stop() {
  cancelAnimationFrame(rafId);
  rafId = 0;
  observer?.disconnect();
  observer = null;
}

onMounted(() => {
  watch(
    () => settings.diagnostics.devOverlay,
    (on) => (on ? start() : stop()),
    { immediate: true }
  );
});

onBeforeUnmount(stop);
</script>

<template>
  <div v-if="settings.diagnostics.devOverlay" class="dev-overlay">
    <span class="dev-fps">{{ fps }} FPS</span>
    <span>中位 {{ medianMs }}ms</span>
    <span>p95 {{ p95Ms }}ms</span>
    <span>最差 {{ worstMs }}ms</span>
    <span>长任务 {{ longtasks }}</span>
  </div>
</template>

<style scoped>
.dev-overlay {
  position: fixed;
  left: 8px;
  top: 8px;
  z-index: 2147483647;
  display: flex;
  gap: 10px;
  padding: 4px 10px;
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.72);
  color: #7ee787;
  font:
    11px/1.6 'Cascadia Mono',
    Consolas,
    monospace;
  pointer-events: none;
  white-space: nowrap;
}
.dev-fps {
  font-weight: 700;
  color: #fff;
}
</style>
