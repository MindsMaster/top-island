import type { GeocodeResult, IpCityInfo, WeatherQueryOptions, WeatherResult } from '../../../shared/ipc';

/** 天气数据源：open-meteo（免 key）；IP 定位：ip-api.com。请求放在主进程避免渲染层 CORS/CSP 限制。 */

let ipCityCache: IpCityInfo | null = null;

async function fetchJson(target: string, timeoutMs = 5000): Promise<any> {
  const res = await fetch(target, { signal: AbortSignal.timeout(timeoutMs) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function ipCity(): Promise<IpCityInfo> {
  if (ipCityCache) return ipCityCache;
  try {
    const data = await fetchJson('http://ip-api.com/json/');
    if (data.status === 'success') {
      ipCityCache = {
        city: data.city || '',
        regionName: data.regionName || '',
        country: data.country || '',
        lat: data.lat,
        lon: data.lon,
      };
      return ipCityCache;
    }
  } catch (e) {
    console.error('[IP City] failed:', e);
  }
  return { city: '', regionName: '', country: '', lat: null, lon: null };
}

export async function geocode(city: string, lang: string): Promise<GeocodeResult> {
  try {
    const params = new URLSearchParams({ name: city, count: '1', language: lang || 'zh' });
    return await fetchJson(`https://geocoding-api.open-meteo.com/v1/search?${params}`);
  } catch (e) {
    console.error('[Geocode] failed:', e);
    return { error: 'geocode_failed' };
  }
}

export async function weatherQuery(
  lat: number,
  lon: number,
  opts?: WeatherQueryOptions
): Promise<WeatherResult> {
  try {
    const params = new URLSearchParams({
      latitude: String(lat),
      longitude: String(lon),
      current_weather: 'true',
    });
    if (opts?.daily) params.set('daily', opts.daily);
    if (opts?.forecastDays) params.set('forecast_days', String(opts.forecastDays));
    return await fetchJson(`https://api.open-meteo.com/v1/forecast?${params}`);
  } catch (e) {
    console.error('[Weather] failed:', e);
    return { error: 'weather_fetch_failed' };
  }
}
