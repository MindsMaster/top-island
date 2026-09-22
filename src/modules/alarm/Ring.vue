<script setup lang="ts">
import { computed } from 'vue';
import { progress } from './store';

const { size, stroke } = defineProps<{ size: number; stroke: number }>();

const R = 13;
const circumference = 2 * Math.PI * R;
const dashOffset = computed(() => circumference * (1 - progress.value));
</script>

<template>
  <svg class="alarm-quick-ring" viewBox="0 0 32 32" :width="size" :height="size">
    <circle cx="16" cy="16" :r="R" fill="none" stroke="rgba(128,128,128,0.28)" :stroke-width="stroke" />
    <circle
      cx="16"
      cy="16"
      :r="R"
      fill="none"
      stroke="var(--accent)"
      :stroke-width="stroke"
      stroke-linecap="round"
      :stroke-dasharray="circumference"
      :stroke-dashoffset="dashOffset"
      transform="rotate(-90 16 16)"
    />
  </svg>
</template>
