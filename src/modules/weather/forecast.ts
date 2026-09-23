import type { MsnDay, MsnNowcast } from '@/platform/types';
import { kindFromMsnSymbol, type WxKind } from './codes';

export interface HourPoint {
  time: number;
  hour: number;
  temp: number;
  kind: WxKind | null;
  night: boolean;
  pop: number;
}

export interface DayPoint {
  time: number;
  hi: number;
  lo: number;
  kind: WxKind | null;
  pop: number;
  capDay: string;
  capNight: string;
  rain: number;
  sunrise: string;
  sunset: string;
}

export interface Nowcast {
  summary: string;
  short: string;
  values: number[];
  stepMin: number;
}

/** 取当地钟点 不经本机时区换算 */
const localHm = (iso: string) => iso.slice(11, 16);

export function hoursAfter(days: MsnDay[], nowMs: number, count: number): HourPoint[] {
  const out: HourPoint[] = [];
  for (const d of days) {
    for (const h of d.hourly) {
      const time = Date.parse(h.valid);
      if (time <= nowMs) continue;
      out.push({
        time,
        hour: Number(h.valid.slice(11, 13)),
        temp: Math.round(h.temp),
        kind: kindFromMsnSymbol(h.symbol),
        night: h.symbol.startsWith('n'),
        pop: Math.round(h.precip ?? 0),
      });
      if (out.length === count) return out;
    }
  }
  return out;
}

export function dailyPoints(days: MsnDay[]): DayPoint[] {
  return days.map(({ daily, almanac }) => ({
    time: Date.parse(daily.valid),
    hi: Math.round(daily.tempHi),
    lo: Math.round(daily.tempLo),
    kind: kindFromMsnSymbol(daily.symbol),
    pop: Math.round(daily.precip),
    capDay: daily.day.cap,
    capNight: daily.night.cap,
    rain: daily.rainAmount,
    sunrise: localHm(almanac.sunrise),
    sunset: localHm(almanac.sunset),
  }));
}

export function nowcastOf(n: MsnNowcast | undefined): Nowcast | null {
  if (!n?.summary) return null;
  return {
    summary: n.summary.replace(/[。.]$/, ''),
    short: n.shortSummary,
    values: n.precipitation,
    stepMin: n.minutesBetweenHorrizons,
  };
}
