import type { CustomTheme, ThemeId } from '@/platform/types';

export type ThemeGroup = 'solid' | 'gradient' | 'custom';

export interface ThemeMeta {
  id: ThemeId;
  /** custom 主题仅占位 */
  scheme: 'dark' | 'light';
  group: ThemeGroup;
  nameKey: string;
  icon: string;
}

/** 新增主题须同步 _tokens.scss 令牌块 */
export const THEMES: ThemeMeta[] = [
  { id: 'dark', scheme: 'dark', group: 'solid', nameKey: 'settingsThemeDark', icon: 'fa-moon' },
  { id: 'light', scheme: 'light', group: 'solid', nameKey: 'settingsThemeLight', icon: 'fa-sun' },
  {
    id: 'graphite',
    scheme: 'dark',
    group: 'solid',
    nameKey: 'settingsThemeGraphite',
    icon: 'fa-circle-half-stroke',
  },
  { id: 'cream', scheme: 'light', group: 'solid', nameKey: 'settingsThemeCream', icon: 'fa-mug-saucer' },
  { id: 'pink', scheme: 'light', group: 'gradient', nameKey: 'settingsThemePink', icon: 'fa-heart' },
  {
    id: 'cyberpink',
    scheme: 'light',
    group: 'gradient',
    nameKey: 'settingsThemeCyber',
    icon: 'fa-wand-magic-sparkles',
  },
  {
    id: 'aurora',
    scheme: 'dark',
    group: 'gradient',
    nameKey: 'settingsThemeAurora',
    icon: 'fa-mountain-sun',
  },
  {
    id: 'sunset',
    scheme: 'light',
    group: 'gradient',
    nameKey: 'settingsThemeSunset',
    icon: 'fa-umbrella-beach',
  },
  { id: 'ocean', scheme: 'dark', group: 'gradient', nameKey: 'settingsThemeOcean', icon: 'fa-water' },
  { id: 'mint', scheme: 'light', group: 'gradient', nameKey: 'settingsThemeMint', icon: 'fa-leaf' },
  { id: 'custom', scheme: 'dark', group: 'custom', nameKey: 'settingsThemeCustom', icon: 'fa-palette' },
];

export function themeMeta(id: ThemeId): ThemeMeta {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}

export function nextThemeId(current: ThemeId): ThemeId {
  const idx = THEMES.findIndex((t) => t.id === current);
  return THEMES[(idx + 1) % THEMES.length].id;
}

export function hexLuminance(hex: string): number {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return 0;
  const v = parseInt(m[1], 16);
  return (0.2126 * ((v >> 16) & 0xff) + 0.7152 * ((v >> 8) & 0xff) + 0.0722 * (v & 0xff)) / 255;
}

export function isLightCustom(custom: CustomTheme): boolean {
  return (hexLuminance(custom.a) + hexLuminance(custom.b)) / 2 > 0.55;
}

/** custom 注入的令牌 切走时逐个清 */
const CUSTOM_VARS = [
  '--island-bg',
  '--ink',
  '--island-text',
  '--panel-bg',
  '--panel-border',
  '--accent',
  '--on-accent',
];

export function applyTheme(id: ThemeId, custom: CustomTheme) {
  const root = document.documentElement;
  const meta = themeMeta(id);
  root.setAttribute('data-theme', meta.id);

  if (meta.id !== 'custom') {
    root.setAttribute('data-scheme', meta.scheme);
    for (const v of CUSTOM_VARS) root.style.removeProperty(v);
    return;
  }

  const { a, b, accent } = custom;
  const light = isLightCustom(custom);
  root.setAttribute('data-scheme', light ? 'light' : 'dark');
  root.style.setProperty('--island-bg', `linear-gradient(135deg, ${a}, ${b})`);
  root.style.setProperty('--ink', light ? '0, 0, 0' : '255, 255, 255');
  root.style.setProperty('--island-text', light ? 'rgba(0, 0, 0, 0.85)' : '#fff');
  root.style.setProperty(
    '--panel-bg',
    `linear-gradient(150deg, color-mix(in srgb, ${a} 22%, ${light ? '#fbfbfd' : '#101014'}), ` +
      `color-mix(in srgb, ${b} 22%, ${light ? '#f4f4f8' : '#0c0c10'}))`
  );
  root.style.setProperty('--panel-border', light ? 'rgba(0, 0, 0, 0.1)' : 'rgba(255, 255, 255, 0.1)');
  root.style.setProperty('--accent', accent);
  root.style.setProperty('--on-accent', hexLuminance(accent) > 0.6 ? '#1a1a1a' : '#fff');
}
