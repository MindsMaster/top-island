import { computed, reactive } from 'vue';
import { weatherApi } from '@/platform/weather';
import { isNightTime } from '@/core/clock';
import { useI18n } from '@/core/i18n';

const DAY_ICONS: Record<number, string> = {
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

/** 夜间仅晴少云换月系图标 */
const NIGHT_ICONS: Record<number, string> = {
  0: 'fa-moon',
  1: 'fa-cloud-moon',
  2: 'fa-cloud-moon',
  3: 'fa-cloud-moon',
};

/** IP 定位失败的兜底 */
const FALLBACK_CITY = '北京';

const REFRESH_MS = 600000;

const { t, weatherCodeName } = useI18n();

export const weatherState = reactive({
  city: '',
  temp: null as number | null,
  tempHi: null as number | null,
  tempLo: null as number | null,
  code: null as number | null,
  loading: false,
  error: null as string | null,
});

export const desc = computed(() => weatherCodeName(weatherState.code));

export const icon = computed(() => {
  const c = weatherState.code;
  if (c === null) return 'fa-cloud';
  if (isNightTime.value && NIGHT_ICONS[c]) return NIGHT_ICONS[c];
  return DAY_ICONS[c] ?? 'fa-cloud';
});

export const bgClass = computed(() => {
  const c = weatherState.code;
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

let ipLat: number | null = null;
let ipLon: number | null = null;
let refreshTimer: number | null = null;

async function tryFetchIpCity() {
  try {
    const data = await weatherApi.ipCity();
    if (!data.city) return;
    weatherState.city = data.city;
    if (data.lat != null && data.lon != null) {
      ipLat = data.lat;
      ipLon = data.lon;
    }
  } catch (e) {
    console.error('[weather] IP 定位失败:', e);
  }
}

async function resolveLocation(): Promise<{ lat: number; lon: number } | null> {
  if (ipLat == null || ipLon == null) await tryFetchIpCity();
  if (ipLat != null && ipLon != null) return { lat: ipLat, lon: ipLon };

  const geo = await weatherApi.geocode(FALLBACK_CITY, 'zh');
  if (!geo.results?.length) return null;
  if (!weatherState.city) weatherState.city = FALLBACK_CITY;
  return { lat: geo.results[0].latitude, lon: geo.results[0].longitude };
}

export async function fetchWeather() {
  if (weatherState.loading) return;
  weatherState.loading = true;
  weatherState.error = null;
  try {
    const loc = await resolveLocation();
    if (!loc) {
      weatherState.error = t('weatherFetchError');
      return;
    }
    const data = await weatherApi.query(loc.lat, loc.lon, {
      daily: 'temperature_2m_max,temperature_2m_min',
      forecastDays: 1,
    });
    if (data.current_weather) {
      weatherState.temp = Math.round(data.current_weather.temperature);
      weatherState.code = data.current_weather.weathercode;
    }
    if (data.daily?.temperature_2m_max?.length) {
      weatherState.tempHi = Math.round(data.daily.temperature_2m_max[0]);
      weatherState.tempLo = Math.round(data.daily.temperature_2m_min[0]);
    }
  } catch (e) {
    console.error('[weather] 拉取失败:', e);
    weatherState.error = t('weatherFetchError');
  } finally {
    weatherState.loading = false;
  }
}

export function startWeather() {
  void fetchWeather();
  if (refreshTimer === null) refreshTimer = window.setInterval(fetchWeather, REFRESH_MS);
}
