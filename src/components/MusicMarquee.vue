<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';

const props = defineProps<{
  text: string;
  active: boolean;
}>();

const trackEl = ref<HTMLElement | null>(null);
let anim: Animation | null = null;
let watchdog: number | null = null;

function build() {
  stop();
  if (!props.text) return;
  const track = trackEl.value;
  if (!track) return;
  const items = track.querySelectorAll<HTMLElement>('.marquee-item');
  if (items.length < 2) return;
  const [a, b] = [items[0], items[1]];
  const box = track.parentElement;
  if (!box) return;
  const itemWidth = a.offsetWidth;
  const boxWidth = box.offsetWidth;
  if (!itemWidth || !boxWidth) return;
  const spacing = Math.max(boxWidth - 8, itemWidth);
  a.style.marginRight = spacing + 'px';
  b.style.marginRight = spacing + 'px';
  const itemStep = itemWidth + spacing;
  const dur = Math.max(8000, (itemStep / 35) * 1000); // 约 35px/s
  anim = track.animate([{ transform: 'translateX(0)' }, { transform: `translateX(-${itemStep}px)` }], {
    duration: dur,
    iterations: Infinity,
    easing: 'linear',
  });
  watchdog = window.setInterval(() => {
    if (!props.active) return;
    if (!anim) build();
    else if (anim.playState !== 'running') anim.play();
  }, 1000);
}

function stop() {
  if (anim) {
    anim.cancel();
    anim = null;
  }
  if (watchdog !== null) {
    clearInterval(watchdog);
    watchdog = null;
  }
}

watch(
  () => [props.text, props.active] as const,
  async ([, active]) => {
    if (active) {
      await nextTick();
      build();
    } else {
      stop();
    }
  }
);

onMounted(async () => {
  if (props.active) {
    await nextTick();
    build();
  }
});

onBeforeUnmount(stop);
</script>

<template>
  <div class="marquee-box">
    <div ref="trackEl" class="marquee-track">
      <span class="marquee-item">{{ text }}</span
      ><span class="marquee-item">{{ text }}</span>
    </div>
  </div>
</template>
