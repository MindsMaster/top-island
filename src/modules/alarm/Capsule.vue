<script setup lang="ts">
import Ring from './Ring.vue';
import { cancelCountdown, displayRemain } from './store';
</script>

<template>
  <div class="alarm-quick">
    <Ring :size="30" :stroke="3.5" />
    <span class="alarm-quick-time">{{ displayRemain }}</span>
    <button class="alarm-quick-stop" @click.stop="cancelCountdown()">
      <i class="fa-solid fa-xmark"></i>
    </button>
  </div>
</template>
