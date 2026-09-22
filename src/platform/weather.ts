import { call } from './invoke';
import type { GeocodeResult, IpCityInfo, WeatherQueryOptions, WeatherResult } from './types';

export const weatherApi = {
  ipCity: () => call<IpCityInfo>('weather_ip_city'),
  geocode: (city: string, lang: string) => call<GeocodeResult>('weather_geocode', { city, lang }),
  query: (lat: number, lon: number, opts?: WeatherQueryOptions) =>
    call<WeatherResult>('weather_query', {
      lat,
      lon,
      daily: opts?.daily ?? null,
      forecastDays: opts?.forecastDays ?? null,
    }),
};
