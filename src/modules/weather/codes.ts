export type WxKind = 'clear' | 'partly' | 'cloudy' | 'fog' | 'rain' | 'rain-heavy' | 'snow' | 'thunder';

/** MSN symbol 形如 d210 昼夜 云量 降水强度 降水类型 */
export function kindFromMsnSymbol(symbol: string): WxKind | null {
  const m = /^[dn](\d)(\d)(\d)$/.exec(symbol);
  if (!m) return null;
  const [cloud, intensity, type] = [Number(m[1]), Number(m[2]), Number(m[3])];
  if (intensity === 4) return 'thunder';
  if (cloud >= 6) return type === 3 ? 'rain' : 'fog';
  if (intensity > 0) {
    if (type === 2) return 'snow';
    return intensity === 3 ? 'rain-heavy' : 'rain';
  }
  if (cloud <= 1) return 'clear';
  return cloud === 2 ? 'partly' : 'cloudy';
}

export function kindFromWmo(code: number): WxKind {
  if (code <= 1) return 'clear';
  if (code === 2) return 'partly';
  if (code === 3) return 'cloudy';
  if (code === 45 || code === 48) return 'fog';
  if (code === 65 || code === 82) return 'rain-heavy';
  if ((code >= 71 && code <= 77) || code === 85 || code === 86) return 'snow';
  if (code >= 95) return 'thunder';
  if (code >= 51 && code <= 81) return 'rain';
  return 'cloudy';
}
