import { describe, expect, it } from 'vitest';
import type { MsnDay } from '@/platform/types';
import { dailyPoints, hoursAfter, nowcastOf } from './forecast';

const hour = (valid: string, temp: number, symbol = 'd200', precip = 0) => ({
  valid,
  temp,
  cap: '',
  symbol,
  precip,
});

const day = (date: string, hourly: MsnDay['hourly']): MsnDay => ({
  hourly,
  daily: {
    valid: `${date}T00:00:00+08:00`,
    symbol: 'd210',
    tempHi: 25.6,
    tempLo: 9.2,
    precip: 49,
    rainAmount: 2.51,
    day: { cap: '小阵雨' },
    night: { cap: '多云' },
  },
  almanac: { sunrise: `${date}T05:22:00+08:00`, sunset: `${date}T17:31:00+08:00` },
});

const days = [
  day('2026-09-23', [
    hour('2026-09-23T15:00:00+08:00', 25.4),
    hour('2026-09-23T16:00:00+08:00', 24.6, 'd210', 35),
    hour('2026-09-23T23:00:00+08:00', 14, 'n300'),
  ]),
  day('2026-09-24', [hour('2026-09-24T00:00:00+08:00', 13)]),
];

describe('hoursAfter', () => {
  const now = Date.parse('2026-09-23T15:30:00+08:00');

  it('跳过已过去的整点 跨天续接', () => {
    expect(hoursAfter(days, now, 5).map((h) => h.hour)).toEqual([16, 23, 0]);
  });

  it('截到 count 条并换算天气类型', () => {
    const [h] = hoursAfter(days, now, 1);
    expect(h).toMatchObject({ temp: 25, kind: 'rain', night: false, pop: 35 });
  });

  it('夜间 symbol 标记为夜', () => {
    expect(hoursAfter(days, now, 2)[1].night).toBe(true);
  });
});

describe('dailyPoints', () => {
  it('取整温度并保留当地日出日落', () => {
    expect(dailyPoints(days)[0]).toMatchObject({
      hi: 26,
      lo: 9,
      kind: 'rain',
      pop: 49,
      capDay: '小阵雨',
      sunrise: '05:22',
      sunset: '17:31',
    });
  });
});

describe('nowcastOf', () => {
  it('没有 summary 视为无数据', () => {
    expect(nowcastOf(undefined)).toBeNull();
  });

  it('去掉句末标点', () => {
    const n = nowcastOf({
      summary: '大约 2 小时后开始降雨。',
      shortSummary: '大约 2 小时后开始降雨',
      precipitation: [0, 7.1],
      minutesBetweenHorrizons: 4,
    });
    expect(n?.summary).toBe('大约 2 小时后开始降雨');
  });
});
