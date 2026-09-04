<script setup lang="ts">
import { computed } from 'vue';
import { useClock } from '../composables/useClock';
import { weatherState, weatherBgClass, weatherDesc, weatherIcon } from '../store/weather';

const { currentTime, currentDate, isNightTime } = useClock();
const snap = weatherState;
const icon = computed(() => weatherIcon(snap, isNightTime.value));
const desc = computed(() => weatherDesc(snap));
const bgClass = computed(() => weatherBgClass(snap, isNightTime.value));
</script>

<template>
  <div class="weather-card" :class="bgClass">
    <div class="weather-card-city">
      <i class="fa-solid fa-location-dot"></i>
      <span>{{ snap.city || '--' }}</span>
    </div>
    <div class="weather-card-main">
      <i class="weather-card-icon" :class="'fa-solid ' + icon"></i>
      <div class="weather-card-temp">{{ snap.temp ?? '--' }}<span class="weather-card-unit">°C</span></div>
      <div class="weather-card-side">
        <div class="weather-card-desc">{{ snap.error || desc }}</div>
        <div v-if="snap.tempHi !== null" class="weather-card-hilo">
          {{ snap.tempHi }}° / {{ snap.tempLo }}°
        </div>
      </div>
    </div>
    <div class="weather-card-fx"></div>
  </div>
  <div class="weather-clock">
    <div class="weather-time">{{ currentTime || '--:--' }}</div>
    <div class="weather-date">{{ currentDate }}</div>
  </div>
</template>
