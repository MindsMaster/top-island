import type { WxKind } from './codes';
import clearDay from './icons/clear-day.svg';
import clearNight from './icons/clear-night.svg';
import extremeRain from './icons/extreme-rain.svg';
import fogDay from './icons/fog-day.svg';
import fogNight from './icons/fog-night.svg';
import overcastDay from './icons/overcast-day.svg';
import overcastNight from './icons/overcast-night.svg';
import partlyDay from './icons/partly-cloudy-day.svg';
import partlyNight from './icons/partly-cloudy-night.svg';
import rain from './icons/rain.svg';
import snow from './icons/snow.svg';
import thunderDay from './icons/thunderstorms-day-rain.svg';
import thunderNight from './icons/thunderstorms-night-rain.svg';

const ART: Record<WxKind, [day: string, night: string]> = {
  clear: [clearDay, clearNight],
  partly: [partlyDay, partlyNight],
  cloudy: [overcastDay, overcastNight],
  fog: [fogDay, fogNight],
  rain: [rain, rain],
  'rain-heavy': [extremeRain, extremeRain],
  snow: [snow, snow],
  thunder: [thunderDay, thunderNight],
};

export function artFor(kind: WxKind | null, night: boolean): string {
  const [day, dark] = ART[kind ?? 'cloudy'];
  return night ? dark : day;
}
