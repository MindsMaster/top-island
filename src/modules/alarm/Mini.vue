<script setup lang="ts">
import { computed, ref } from 'vue';
import { shell } from '@/shell/commands';
import { shellView } from '@/shell/view';
import { contributeHotRect } from '@/shell/useHotRegion';
import Ring from './Ring.vue';
import { cancelCountdown, displayRemain } from './store';
import { miniHovered, showMini } from './mini';

const el = ref<HTMLElement | null>(null);
contributeHotRect(el);

/** 岛右缘外间距 */
const GAP = 10;

const style = computed(() => {
  const s: Record<string, string> = {
    left: `calc(50% + ${Math.round(shellView.islandWidth / 2) + GAP}px)`,
  };
  if (shellView.dragOffset < 0) {
    s.translate = `0 ${shellView.dragOffset}px`;
    s.transition = 'none';
  } else if (shellView.hidden) {
    s.translate = '0 var(--island-hidden-shift, -34px)';
  }
  return s;
});

function onClick() {
  if (shellView.hidden) {
    shell.reveal();
    return;
  }
  shell.openPanel('alarm');
}
</script>

<template>
  <Transition name="capfade">
    <div
      v-if="showMini"
      ref="el"
      class="alarm-mini"
      role="button"
      :style="style"
      @mouseenter="miniHovered = true"
      @mouseleave="miniHovered = false"
      @click.stop="onClick"
    >
      <Ring :size="18" :stroke="4" />
      <div class="alarm-mini-pop">
        <div class="alarm-mini-pop-card">
          <span class="alarm-mini-time">{{ displayRemain }}</span>
          <button class="alarm-mini-stop" @click.stop="cancelCountdown()">
            <i class="fa-solid fa-xmark"></i>
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>
