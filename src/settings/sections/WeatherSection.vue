<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { weatherApi } from '@/platform/weather';
import { useI18n } from '@/core/i18n';
import { settings } from '@/core/settings';
import SettingSwitch from '@/ui/SettingSwitch.vue';
import type { WeatherCity } from '@/platform/types';

const MAX_CITIES = 5;
const SEARCH_DEBOUNCE_MS = 350;

const { t, lang } = useI18n();

const autoCity = ref('');
const query = ref('');
const results = ref<WeatherCity[]>([]);
const searching = ref(false);
const listEl = ref<HTMLElement | null>(null);
const dragFrom = ref(-1);

const cities = computed(() => settings.weather.cities);
const full = computed(() => cities.value.length >= MAX_CITIES);

function place(c: WeatherCity): string {
  return [c.admin, c.country].filter(Boolean).join(' · ');
}

let searchTimer = 0;
let searchSeq = 0;

function onQueryInput() {
  window.clearTimeout(searchTimer);
  const q = query.value.trim();
  if (!q) {
    searchSeq++;
    results.value = [];
    searching.value = false;
    return;
  }
  searching.value = true;
  searchTimer = window.setTimeout(() => void search(q), SEARCH_DEBOUNCE_MS);
}

async function search(q: string) {
  const seq = ++searchSeq;
  try {
    const geo = await weatherApi.geocode(q, lang.value === 'zh-CN' ? 'zh' : 'en');
    if (seq !== searchSeq) return;
    // 结果混有山峰机场公园
    results.value = (geo.results ?? [])
      .filter((r) => r.feature_code?.startsWith('PPL'))
      .slice(0, MAX_CITIES)
      .map((r) => ({
        id: String(r.id),
        name: r.name ?? q,
        admin: [...new Set([r.admin1, r.admin2].filter(Boolean))].join(' '),
        country: r.country ?? '',
        lat: r.latitude,
        lon: r.longitude,
      }));
  } catch {
    if (seq === searchSeq) results.value = [];
  } finally {
    if (seq === searchSeq) searching.value = false;
  }
}

function add(c: WeatherCity) {
  if (full.value || cities.value.some((x) => x.id === c.id)) return;
  settings.weather.cities = [...cities.value, c];
  query.value = '';
  results.value = [];
}

function remove(id: string) {
  settings.weather.cities = cities.value.filter((c) => c.id !== id);
  if (settings.weather.active === id) settings.weather.active = 'auto';
}

function move(from: number, to: number) {
  if (to < 0 || to >= cities.value.length || to === from) return;
  const next = [...cities.value];
  next.splice(to, 0, ...next.splice(from, 1));
  settings.weather.cities = next;
}

/** Tauri 默认拦截 HTML5 拖放 */
function onGripDown(i: number, e: PointerEvent) {
  dragFrom.value = i;
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
}

function onGripMove(e: PointerEvent) {
  if (dragFrom.value < 0 || !listEl.value) return;
  const rows = [...listEl.value.children];
  const to = rows.findIndex((row) => {
    const r = row.getBoundingClientRect();
    return e.clientY >= r.top && e.clientY < r.bottom;
  });
  if (to < 0 || to === dragFrom.value) return;
  move(dragFrom.value, to);
  dragFrom.value = to;
}

function onGripUp() {
  dragFrom.value = -1;
}

onMounted(async () => {
  const info = await weatherApi.ipCity(lang.value === 'zh-CN' ? 'zh-CN' : 'en').catch(() => null);
  autoCity.value = info?.city ?? '';
});
</script>

<template>
  <section class="setting-group">
    <SettingSwitch v-model="settings.weather.auto" :label="t('weatherAuto')">
      <span v-if="autoCity" class="setting-status">{{ autoCity }}</span>
    </SettingSwitch>
  </section>

  <section class="setting-section">
    <h2 class="setting-group-title">
      {{ t('weatherCities') }}
      <span class="weather-city-count">{{ cities.length }} / {{ MAX_CITIES }}</span>
    </h2>
    <ul v-if="cities.length" ref="listEl" class="setting-group weather-city-list">
      <li v-for="(c, i) in cities" :key="c.id" class="weather-city-row" :class="{ dragging: dragFrom === i }">
        <button
          class="weather-city-grip"
          :aria-label="t('weatherCityMove', c.name)"
          @pointerdown="onGripDown(i, $event)"
          @pointermove="onGripMove"
          @pointerup="onGripUp"
          @pointercancel="onGripUp"
          @keydown.up.prevent="move(i, i - 1)"
          @keydown.down.prevent="move(i, i + 1)"
        >
          <i class="fa-solid fa-grip-vertical" aria-hidden="true"></i>
        </button>
        <div class="weather-city-text">
          <span>{{ c.name }}</span>
          <small>{{ place(c) }}</small>
        </div>
        <button class="weather-city-btn" :aria-label="t('weatherCityRemove', c.name)" @click="remove(c.id)">
          <i class="fa-solid fa-trash" aria-hidden="true"></i>
        </button>
      </li>
    </ul>

    <label class="weather-search">
      <i class="fa-solid fa-magnifying-glass" aria-hidden="true"></i>
      <input
        v-model="query"
        type="text"
        maxlength="40"
        :disabled="full"
        :aria-label="t('weatherSearch')"
        :placeholder="full ? t('weatherCitiesFull') : t('weatherSearch')"
        @input="onQueryInput"
      />
    </label>
    <ul v-if="results.length" class="setting-group weather-city-list weather-results">
      <li v-for="r in results" :key="r.id" class="weather-city-row">
        <i class="fa-solid fa-location-dot weather-city-pin" aria-hidden="true"></i>
        <div class="weather-city-text">
          <span>{{ r.name }}</span>
          <small>{{ place(r) }}</small>
        </div>
        <button class="setting-action-btn" :disabled="cities.some((c) => c.id === r.id)" @click="add(r)">
          {{ cities.some((c) => c.id === r.id) ? t('weatherCityAdded') : t('weatherCityAdd') }}
        </button>
      </li>
    </ul>
    <p v-else-if="query.trim() && !searching" class="setting-feedback weather-empty" role="status">
      {{ t('weatherSearchEmpty') }}
    </p>
  </section>
</template>
