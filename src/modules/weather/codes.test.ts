import { describe, expect, it } from 'vitest';
import { kindFromMsnSymbol, kindFromWmo } from './codes';

describe('kindFromMsnSymbol', () => {
  it.each([
    ['d000', 'clear'],
    ['n100', 'clear'],
    ['d200', 'partly'],
    ['n300', 'cloudy'],
    ['d400', 'cloudy'],
    ['d500', 'cloudy'],
    ['d210', 'rain'],
    ['n410', 'rain'],
    ['d211', 'rain'],
    ['d430', 'rain-heavy'],
    ['d212', 'snow'],
    ['d432', 'snow'],
    ['d240', 'thunder'],
    ['d440', 'thunder'],
    ['d600', 'fog'],
    ['d905', 'fog'],
    ['d603', 'rain'],
  ] as const)('%s → %s', (symbol, kind) => {
    expect(kindFromMsnSymbol(symbol)).toBe(kind);
  });

  it('无法识别的 symbol 返回 null', () => {
    expect(kindFromMsnSymbol('')).toBeNull();
    expect(kindFromMsnSymbol('x200')).toBeNull();
  });
});

describe('kindFromWmo', () => {
  it.each([
    [0, 'clear'],
    [2, 'partly'],
    [3, 'cloudy'],
    [48, 'fog'],
    [56, 'rain'],
    [67, 'rain'],
    [80, 'rain'],
    [82, 'rain-heavy'],
    [77, 'snow'],
    [86, 'snow'],
    [99, 'thunder'],
  ] as const)('%i → %s', (code, kind) => {
    expect(kindFromWmo(code)).toBe(kind);
  });
});
