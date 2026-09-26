import { call } from './invoke';
import type {
  GeocodeResult,
  IpCityInfo,
  MsnOverview,
  WeatherCity,
  WeatherQueryOptions,
  WeatherResult,
} from './types';

export const weatherApi = {
  ipCity: (lang: string) => call<IpCityInfo>('weather_ip_city', { lang }),
  geocode: (city: string, lang: string) => call<GeocodeResult>('weather_geocode', { city, lang }),
  /** 服务端限速每秒一次 只在用户提交时调 */
  searchCity: (query: string, lang: string) => call<WeatherCity[]>('weather_search_city', { query, lang }),
  msnOverview: (lat: number, lon: number, locale: string) =>
    call<MsnOverview>('weather_msn_overview', { lat, lon, locale }),
  query: (lat: number, lon: number, opts?: WeatherQueryOptions) =>
    call<WeatherResult>('weather_query', {
      lat,
      lon,
      daily: opts?.daily ?? null,
      forecastDays: opts?.forecastDays ?? null,
    }),
};
