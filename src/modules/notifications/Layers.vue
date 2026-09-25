<script setup lang="ts">
import { computed } from 'vue';
import { shellView } from '@/shell/view';
import { notifyState } from './store';

/** 垫层封顶两层 */
const depth = computed(() =>
  shellView.capsuleOwner === 'notifications' ? Math.min(notifyState.popups.length - 1, 2) : 0
);
</script>

<template>
  <div class="notify-layers" :class="{ fan: notifyState.popupHovered }" aria-hidden="true">
    <div class="notify-layer layer-1" :class="{ on: depth >= 1 }"></div>
    <div class="notify-layer layer-2" :class="{ on: depth >= 2 }"></div>
  </div>
</template>
