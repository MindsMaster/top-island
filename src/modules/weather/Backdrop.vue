<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { settings } from '@/core/settings';
import { shellView } from '@/shell/view';
import { WeatherScene } from './scene';
import { isNight, weatherState, weatherView } from './store';

/** 详情页压暗 半帧率即可 */
const DIMMED_FRAME_MS = 1000 / 30;

const canvasEl = ref<HTMLCanvasElement | null>(null);
const pageVisible = ref(!document.hidden);
const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');

const running = computed(() => shellView.panel === 'weather' && pageVisible.value);

let scene: WeatherScene | null = null;
let raf: number | null = null;
let last = 0;

function syncScene() {
  const light = document.documentElement.dataset.scheme === 'light';
  scene?.set(weatherState.kind, isNight.value, light);
}

function loop(now: number) {
  raf = requestAnimationFrame(loop);
  const elapsed = now - last;
  if (weatherView.value && elapsed < DIMMED_FRAME_MS) return;
  last = now;
  scene?.frame(Math.min(elapsed / 1000, 0.05));
}

function start() {
  syncScene();
  if (reducedMotion.matches) {
    scene?.frame(0);
    return;
  }
  if (raf !== null) return;
  last = performance.now();
  raf = requestAnimationFrame(loop);
}

function stop() {
  if (raf !== null) cancelAnimationFrame(raf);
  raf = null;
}

function onVisibility() {
  pageVisible.value = !document.hidden;
}

watch(running, (on) => (on ? start() : stop()));
watch([() => weatherState.kind, isNight], () => {
  if (!running.value) return;
  syncScene();
  if (reducedMotion.matches) scene?.frame(0);
});

onMounted(() => {
  scene = new WeatherScene(canvasEl.value!);
  scene.resize();
  document.addEventListener('visibilitychange', onVisibility);
  if (running.value) start();
});

onBeforeUnmount(() => {
  stop();
  document.removeEventListener('visibilitychange', onVisibility);
});
</script>

<template>
  <div
    class="wx-sky"
    :class="[
      `wx-${weatherState.kind ?? 'cloudy'}`,
      { 'wx-night': isNight, 'wx-dim': weatherView, 'wx-plain': !settings.weatherSky },
    ]"
  >
    <canvas ref="canvasEl" class="wx-canvas"></canvas>
  </div>
</template>
