import { describe, expect, it } from 'vitest';
import { builtinLyricOffset, extrapolate, formatTimeMs, lyricIndexAt, trackIdentity } from './musicMath';

describe('extrapolate（锚点外推）', () => {
  it('播放中按经过的时间推进', () => {
    expect(extrapolate({ positionMs: 10_000, anchorEpochMs: 1_000, rate: 1 }, 3_500, 0)).toBe(12_500);
  });

  it('暂停（rate=0）时位置不变', () => {
    expect(extrapolate({ positionMs: 10_000, anchorEpochMs: 1_000, rate: 0 }, 99_000, 0)).toBe(10_000);
  });

  it('钳制到时长上限', () => {
    expect(extrapolate({ positionMs: 170_000, anchorEpochMs: 0, rate: 1 }, 20_000, 177_900)).toBe(177_900);
  });

  it('durationMs<=0 视为无上限', () => {
    expect(extrapolate({ positionMs: 170_000, anchorEpochMs: 0, rate: 1 }, 20_000, 0)).toBe(190_000);
  });

  it('不会低于 0（时钟回拨等）', () => {
    expect(extrapolate({ positionMs: 500, anchorEpochMs: 10_000, rate: 1 }, 0, 0)).toBe(0);
  });
});

describe('lyricIndexAt（歌词行二分定位）', () => {
  const lines = [
    { timeMs: 0, text: 'a' },
    { timeMs: 8_275, text: 'b' },
    { timeMs: 11_793, text: 'c' },
    { timeMs: 13_597, text: 'd' },
  ];

  it('空歌词返回 -1', () => {
    expect(lyricIndexAt([], 5_000)).toBe(-1);
  });

  it('未到首句返回 -1', () => {
    expect(lyricIndexAt(lines.slice(1), 1_000)).toBe(-1);
  });

  it('取最后一条 timeMs <= pos 的行', () => {
    expect(lyricIndexAt(lines, 9_000)).toBe(1);
    expect(lyricIndexAt(lines, 11_793)).toBe(2); // 恰好相等属于该行
    expect(lyricIndexAt(lines, 999_999)).toBe(3);
  });

  it('位置 0 命中 timeMs=0 的首行', () => {
    expect(lyricIndexAt(lines, 0)).toBe(0);
  });
});

describe('builtinLyricOffset', () => {
  it('QQ 音乐补 400ms，其它源为 0', () => {
    expect(builtinLyricOffset('QQMusic.exe')).toBe(400);
    expect(builtinLyricOffset('cloudmusic.exe')).toBe(0);
    expect(builtinLyricOffset('')).toBe(0);
  });
});

describe('formatTimeMs', () => {
  it('m:ss 并补零', () => {
    expect(formatTimeMs(0)).toBe('0:00');
    expect(formatTimeMs(65_000)).toBe('1:05');
    expect(formatTimeMs(177_900)).toBe('2:57');
  });

  it('负数与 NaN 视为 0', () => {
    expect(formatTimeMs(-5_000)).toBe('0:00');
    expect(formatTimeMs(Number.NaN)).toBe('0:00');
  });
});

describe('trackIdentity（切歌判定键）', () => {
  it('有 songId 以它为身份', () => {
    expect(trackIdentity('2088141882', 'Upset', 'Brent')).toBe('2088141882');
  });

  it('无 songId 退回 标题|艺术家', () => {
    expect(trackIdentity(undefined, 'Upset', 'Brent')).toBe('Upset|Brent');
    expect(trackIdentity('', 'Upset', 'Brent')).toBe('Upset|Brent');
  });
});
