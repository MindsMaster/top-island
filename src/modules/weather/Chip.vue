<script setup lang="ts">
import { icon, weatherState as snap } from './store';
</script>

<template>
  <div v-if="snap.temp !== null" class="quick-weather">
    <i :class="'fa-solid ' + icon"></i>
    <span>{{ snap.temp }}°</span>
    <span class="quick-weather-city">{{ snap.city }}</span>
  </div>
</template>
