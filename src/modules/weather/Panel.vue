<script setup lang="ts">
import { currentDate, currentTime } from '@/core/clock';
import { bgClass, desc, icon, weatherState as snap } from './store';
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
