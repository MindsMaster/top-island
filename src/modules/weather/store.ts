import { computed, reactive } from 'vue';
import { weatherApi } from '@/platform/weather';
import { isNightTime } from '@/core/clock';
import { useI18n } from '@/core/i18n';
import { kindFromMsnSymbol, kindFromWmo, type WxKind } from './codes';

const ICONS: Record<WxKind, string> = {
  clear: 'fa-sun',
  partly: 'fa-cloud-sun',
  cloudy: 'fa-cloud',
  fog: 'fa-smog',
  rain: 'fa-cloud-rain',
  'rain-heavy': 'fa-cloud-showers-heavy',
  snow: 'fa-snowflake',
  thunder: 'fa-bolt',
};

/** 夜间仅晴少云换月系图标 */
const NIGHT_ICONS: Partial<Record<WxKind, string>> = {
  clear: 'fa-moon',
  partly: 'fa-cloud-moon',
};

const BG_CLASSES: Record<WxKind, string> = {
  clear: 'wx-clear',
  partly: 'wx-cloudy',
  cloudy: 'wx-cloudy',
  fog: 'wx-fog',
  rain: 'wx-rain',
  'rain-heavy': 'wx-rain-heavy',
  snow: 'wx-snow',
  thunder: 'wx-thunder',
};

/** IP 定位失败的兜底 */
const FALLBACK_CITY = '北京';

const REFRESH_MS = 600000;

const { lang, t, weatherCodeName } = useI18n();

export const weatherState = reactive({
  city: '',
  temp: null as number | null,
  tempHi: null as number | null,
  tempLo: null as number | null,
  code: null as number | null,
  /** MSN 已按语言本地化的天气描述 */
  cap: null as string | null,
  kind: null as WxKind | null,
  night: null as boolean | null,
  loading: false,
  error: null as string | null,
});

export const desc = computed(() => weatherState.cap ?? weatherCodeName(weatherState.code));

const isNight = computed(() => weatherState.night ?? isNightTime.value);

export const icon = computed(() => {
  const k = weatherState.kind;
  if (k === null) return 'fa-cloud';
  return (isNight.value && NIGHT_ICONS[k]) || ICONS[k];
});

export const bgClass = computed(() => {
  const bg = BG_CLASSES[weatherState.kind ?? 'cloudy'];
  return isNight.value && (bg === 'wx-clear' || bg === 'wx-cloudy') ? `${bg}-night` : bg;
});

let ipLat: number | null = null;
let ipLon: number | null = null;
let refreshTimer: number | null = null;

async function tryFetchIpCity() {
  try {
    const data = await weatherApi.ipCity(lang.value === 'zh-CN' ? 'zh-CN' : 'en');
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

async function tryFetchMsn(lat: number, lon: number): Promise<boolean> {
  try {
    const data = await weatherApi.msnOverview(lat, lon, lang.value === 'zh-CN' ? 'zh-cn' : 'en-us');
    const w = data.responses?.[0]?.weather?.[0];
    if (!w) return false;
    const { temp, cap, symbol } = w.current;
    weatherState.temp = Math.round(temp);
    weatherState.code = null;
    weatherState.cap = cap;
    weatherState.kind = kindFromMsnSymbol(symbol);
    weatherState.night = symbol.startsWith('n');
    const today = w.forecast?.days[0]?.daily;
    weatherState.tempHi = today ? Math.round(today.tempHi) : null;
    weatherState.tempLo = today ? Math.round(today.tempLo) : null;
    return true;
  } catch (e) {
    console.error('[weather] MSN 拉取失败 回退 Open-Meteo:', e);
    return false;
  }
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
    if (await tryFetchMsn(loc.lat, loc.lon)) return;
    const data = await weatherApi.query(loc.lat, loc.lon, {
      daily: 'temperature_2m_max,temperature_2m_min',
      forecastDays: 1,
    });
    if (data.current) {
      weatherState.temp = Math.round(data.current.temperature_2m);
      weatherState.code = data.current.weather_code;
      weatherState.cap = null;
      weatherState.kind = kindFromWmo(data.current.weather_code);
      weatherState.night = data.current.is_day === 0;
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
