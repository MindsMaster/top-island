export type WxKind = 'clear' | 'partly' | 'cloudy' | 'fog' | 'rain' | 'rain-heavy' | 'snow' | 'thunder';

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
