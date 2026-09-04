import { reactive } from 'vue';
import { api } from '../api';
import { useI18n } from '../i18n';

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

/** 天气域共享状态。组件模板直读字段（reactive 自动追踪）；
 *  派生数据（desc/icon/bgClass）不放这里，用下面的派生函数包 computed 自取 */
export const weatherState = reactive({
  city: '',
  temp: null as number | null,
  tempHi: null as number | null,
  tempLo: null as number | null,
  code: null as number | null,
  loading: false,
  error: null as string | null,
});

/** 派生函数的入参：直接传 weatherState */
export type WeatherStateView = Readonly<typeof weatherState>;

// 纯内部簿记，不参与渲染，不进 proxy
let ipLat: number | null = null;
let ipLon: number | null = null;
let refreshTimer: number | null = null;

async function tryFetchIpCity() {
  try {
    const data = await api.weatherIpCity();
    if (data.city) {
      weatherState.city = data.city;
      if (data.lat != null && data.lon != null) {
        ipLat = data.lat;
        ipLon = data.lon;
      }
    }
  } catch (e) {
    console.error('[IP City] fetch failed:', e);
  }
}

export async function fetchWeather() {
  if (weatherState.loading) return;
  weatherState.loading = true;
  weatherState.error = null;
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
        weatherState.error = t('weatherFetchError');
        weatherState.loading = false;
        return;
      }
      latitude = geo.results[0].latitude;
      longitude = geo.results[0].longitude;
      if (!weatherState.city) weatherState.city = FALLBACK_CITY;
    }
    const data = await api.weatherQuery(latitude, longitude, {
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
    console.error('[Weather] fetch failed:', e);
    weatherState.error = t('weatherFetchError');
  }
  weatherState.loading = false;
}

export async function initWeather() {
  await fetchWeather();
  if (refreshTimer === null) {
    refreshTimer = window.setInterval(fetchWeather, 600000);
  }
}

/* ---- 响应式派生：组件里包 computed(fn(state)) 用 ---- */

export function weatherDesc(state: WeatherStateView): string {
  return weatherCodeName(state.code);
}

/** 天气图标（夜间对晴/少云类换月亮系图标） */
export function weatherIcon(state: WeatherStateView, isNight: boolean): string {
  if (state.code !== null && isNight && NIGHT_WEATHER_ICONS[state.code]) {
    return NIGHT_WEATHER_ICONS[state.code];
  }
  return (state.code !== null && WEATHER_ICONS[state.code]) || 'fa-cloud';
}

/** 天气背景类（按天气码 + 昼夜） */
export function weatherBgClass(state: WeatherStateView, isNight: boolean): string {
  const c = state.code;
  if (c === null) return isNight ? 'wx-cloudy-night' : 'wx-cloudy';
  if (c === 0 || c === 1) return isNight ? 'wx-clear-night' : 'wx-clear';
  if (c === 2 || c === 3) return isNight ? 'wx-cloudy-night' : 'wx-cloudy';
  if (c === 45 || c === 48) return 'wx-fog';
  if (c === 65) return 'wx-rain-heavy';
  if (c >= 51 && c <= 65) return 'wx-rain';
  if (c >= 71 && c <= 75) return 'wx-snow';
  if (c >= 95) return 'wx-thunder';
  return isNight ? 'wx-cloudy-night' : 'wx-cloudy';
}
