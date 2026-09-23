import { describe, expect, it } from 'vitest';
import { kindFromWmo } from './codes';

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
