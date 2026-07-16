<script setup lang="ts">
import { useClock } from '../composables/useClock';
import { useWeather } from '../composables/useWeather';

const { currentTime, currentDate } = useClock();
const weather = useWeather();
</script>

<template>
  <div class="weather-card" :class="weather.bgClass.value">
    <div class="weather-card-city">
      <i class="fa-solid fa-location-dot"></i>
      <span>{{ weather.city.value || '--' }}</span>
    </div>
    <div class="weather-card-main">
      <i class="weather-card-icon" :class="'fa-solid ' + weather.icon.value"></i>
      <div class="weather-card-temp">
        {{ weather.temp.value ?? '--' }}<span class="weather-card-unit">°C</span>
      </div>
      <div class="weather-card-side">
        <div class="weather-card-desc">{{ weather.error.value || weather.desc.value }}</div>
        <div v-if="weather.tempHi.value !== null" class="weather-card-hilo">
          {{ weather.tempHi.value }}° / {{ weather.tempLo.value }}°
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
