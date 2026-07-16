import { computed, ref } from 'vue';
import { api } from '../api';
import { useI18n } from '../i18n';
import { useClock } from './useClock';

const WEATHER_ICONS: Record<number, string> = {
  0: 'fa-sun',
  1: 'fa-cloud-sun',
  2: 'fa-cloud-sun',
  3: 'fa-cloud-sun',
  45: 'fa-smog',
  48: 'fa-smog',
  51: 'fa-cloud-rain',
  53: 'fa-cloud-rain',
  55: 'fa-cloud-rain',
  61: 'fa-cloud-rain',
  63: 'fa-cloud-rain',
  65: 'fa-cloud-showers-heavy',
  71: 'fa-snowflake',
  73: 'fa-snowflake',
  75: 'fa-snowflake',
  95: 'fa-bolt',
  96: 'fa-bolt',
  99: 'fa-bolt',
};

const NIGHT_WEATHER_ICONS: Record<number, string> = {
  0: 'fa-moon',
  1: 'fa-cloud-moon',
  2: 'fa-cloud-moon',
  3: 'fa-cloud-moon',
};

/** IP 定位失败时的兜底城市（仅内部地理编码用，不提供用户自定义） */
const FALLBACK_CITY = '北京';

const { t, weatherCodeName } = useI18n();
const { isNightTime } = useClock();

const city = ref('');
const temp = ref<number | null>(null);
const tempHi = ref<number | null>(null);
const tempLo = ref<number | null>(null);
const code = ref<number | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);

let ipLat: number | null = null;
let ipLon: number | null = null;
let refreshTimer: number | null = null;

async function tryFetchIpCity() {
  try {
    const data = await api.weatherIpCity();
    if (data.city) {
      city.value = data.city;
      if (data.lat != null && data.lon != null) {
        ipLat = data.lat;
        ipLon = data.lon;
      }
    }
  } catch (e) {
    console.error('[IP City] fetch failed:', e);
  }
}

async function fetchWeather() {
  if (loading.value) return;
  loading.value = true;
  error.value = null;
  try {
    // IP 定位失败过则每轮刷新重试，成功前用兜底城市地理编码
    if (ipLat == null || ipLon == null) await tryFetchIpCity();
    let latitude: number;
    let longitude: number;
    if (ipLat != null && ipLon != null) {
      latitude = ipLat;
      longitude = ipLon;
    } else {
      const geo = await api.weatherGeocode(FALLBACK_CITY, 'zh');
      if (!geo.results || !geo.results.length) {
        error.value = t('weatherFetchError');
        loading.value = false;
        return;
      }
      latitude = geo.results[0].latitude;
      longitude = geo.results[0].longitude;
      if (!city.value) city.value = FALLBACK_CITY;
    }
    const data = await api.weatherQuery(latitude, longitude, {
      daily: 'temperature_2m_max,temperature_2m_min',
      forecastDays: 1,
    });
    if (data.current_weather) {
      temp.value = Math.round(data.current_weather.temperature);
      code.value = data.current_weather.weathercode;
    }
    if (data.daily?.temperature_2m_max?.length) {
      tempHi.value = Math.round(data.daily.temperature_2m_max[0]);
      tempLo.value = Math.round(data.daily.temperature_2m_min[0]);
    }
  } catch (e) {
    console.error('[Weather] fetch failed:', e);
    error.value = t('weatherFetchError');
  }
  loading.value = false;
}

async function initWeather() {
  await fetchWeather();
  if (refreshTimer === null) {
    refreshTimer = window.setInterval(fetchWeather, 600000);
  }
}

const desc = computed(() => weatherCodeName(code.value));

const icon = computed(() => {
  if (code.value !== null && isNightTime.value && NIGHT_WEATHER_ICONS[code.value]) {
    return NIGHT_WEATHER_ICONS[code.value];
  }
  return (code.value !== null && WEATHER_ICONS[code.value]) || 'fa-cloud';
});

/** 天气背景类（按天气码 + 昼夜） */
const bgClass = computed(() => {
  const c = code.value;
  const night = isNightTime.value;
  if (c === null) return night ? 'wx-cloudy-night' : 'wx-cloudy';
  if (c === 0 || c === 1) return night ? 'wx-clear-night' : 'wx-clear';
  if (c === 2 || c === 3) return night ? 'wx-cloudy-night' : 'wx-cloudy';
  if (c === 45 || c === 48) return 'wx-fog';
  if (c === 65) return 'wx-rain-heavy';
  if (c >= 51 && c <= 65) return 'wx-rain';
  if (c >= 71 && c <= 75) return 'wx-snow';
  if (c >= 95) return 'wx-thunder';
  return night ? 'wx-cloudy-night' : 'wx-cloudy';
});

export function useWeather() {
  return { city, temp, tempHi, tempLo, code, loading, error, desc, icon, bgClass, initWeather, fetchWeather };
}
