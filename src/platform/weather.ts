import { call } from './invoke';
import type { GeocodeResult, IpCityInfo, MsnOverview, WeatherQueryOptions, WeatherResult } from './types';

export const weatherApi = {
  ipCity: () => call<IpCityInfo>('weather_ip_city'),
  geocode: (city: string, lang: string) => call<GeocodeResult>('weather_geocode', { city, lang }),
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
