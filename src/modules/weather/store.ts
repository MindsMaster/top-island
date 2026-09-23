import { computed, reactive, ref } from 'vue';
import { weatherApi } from '@/platform/weather';
import { isNightTime } from '@/core/clock';
import { useI18n } from '@/core/i18n';
import { kindFromMsnSymbol, kindFromWmo, type WxKind } from './codes';
import { dailyPoints, hoursAfter, nowcastOf, type DayPoint, type HourPoint, type Nowcast } from './forecast';

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

/** IP 定位失败的兜底 */
const FALLBACK_CITY = '北京';

const REFRESH_MS = 600000;
const HOURLY_COUNT = 23;

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
  feels: null as number | null,
  humidity: null as number | null,
  wind: '',
  uv: '',
  /** km */
  visibility: null as number | null,
  aqi: '',
  hourly: [] as HourPoint[],
  daily: [] as DayPoint[],
  nowcast: null as Nowcast | null,
  loading: false,
  error: null as string | null,
});

export const desc = computed(() => weatherState.cap ?? weatherCodeName(weatherState.code));

export const isNight = computed(() => weatherState.night ?? isNightTime.value);

export function iconFor(kind: WxKind | null, night: boolean): string {
  if (kind === null) return 'fa-cloud';
  return (night && NIGHT_ICONS[kind]) || ICONS[kind];
}

export const icon = computed(() => iconFor(weatherState.kind, isNight.value));

export type WeatherView = 'today' | 'week';

export const weatherView = ref<WeatherView | null>(null);

export const hint = computed<string | null>(() => {
  const n = weatherState.nowcast;
  if (n && n.values.some((v) => v > 0)) return n.short || n.summary;
  const [today, tomorrow] = weatherState.daily;
  if (!tomorrow) return null;
  const diff = tomorrow.hi - today.hi;
  if (Math.abs(diff) >= 5) return t(diff > 0 ? 'weatherWarmer' : 'weatherCooler', Math.abs(diff));
  return `${t('weatherTomorrow')} ${tomorrow.capDay} ${tomorrow.lo}° / ${tomorrow.hi}°`;
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
    const c = w.current;
    const days = w.forecast?.days ?? [];
    const daily = dailyPoints(days);
    Object.assign(weatherState, {
      temp: Math.round(c.temp),
      code: null,
      cap: c.cap,
      kind: kindFromMsnSymbol(c.symbol),
      night: c.symbol.startsWith('n'),
      tempHi: daily[0]?.hi ?? null,
      tempLo: daily[0]?.lo ?? null,
      feels: Math.round(c.feels),
      humidity: Math.round(c.rh),
      wind: `${c.pvdrWindDir} ${c.pvdrWindSpd}`,
      uv: `${Math.round(c.uv)} ${c.uvDesc}`,
      visibility: c.vis,
      aqi: c.aqi != null ? `AQI ${Math.round(c.aqi)} ${c.aqiSeverity ?? ''}`.trim() : '',
      hourly: hoursAfter(days, Date.now(), HOURLY_COUNT),
      daily,
      nowcast: nowcastOf(w.nowcasting),
    });
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
    Object.assign(weatherState, {
      feels: null,
      humidity: null,
      wind: '',
      uv: '',
      visibility: null,
      aqi: '',
      hourly: [],
      daily: [],
      nowcast: null,
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
