import { reactive, toRaw, watch } from 'vue';
import { api } from '../api';
import { useI18n } from '../i18n';
import type {
  AppSettings,
  CustomTheme,
  DiagnosticsToggles,
  IslandLayout,
  NotificationsConfig,
  NotificationsPrivacy,
  ThemeId,
} from '../../shared/ipc';

export type ThemeGroup = 'solid' | 'gradient' | 'custom';

export const THEMES: Array<{
  id: ThemeId;
  /** custom 主题的 scheme 在 applyTheme 里按背景亮度动态判定，此处仅占位 */
  scheme: 'dark' | 'light';
  group: ThemeGroup;
  nameKey: string;
  icon: string;
}> = [
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

const DEFAULT_DIAGNOSTICS: DiagnosticsToggles = {
  clipboardPoll: true,
  musicPoll: true,
};

const DEFAULT_CUSTOM_THEME: CustomTheme = { a: '#6d4bce', b: '#e0508f', accent: '#ff7ab8' };

const DEFAULT_ISLAND: IslandLayout = { scale: 100, hiddenPeek: 6, displayId: 'primary' };

const DEFAULT_PRIVACY: NotificationsPrivacy = {
  enabled: false,
  blurAvatar: true,
  blurName: true,
  replaceBody: true,
  bodyText: '',
};

const DEFAULT_NOTIFICATIONS: NotificationsConfig = {
  enabled: false,
  popup: true,
  privacy: { ...DEFAULT_PRIVACY },
  suppressBanner: false,
  wechat: false,
};

/** 全部窗口共享的设置。组件读直接渲染字段（reactive 自动追踪），写也直接改字段——
 *  subscribe 统一负责持久化+广播+主题应用，加字段只需改默认值和类型 */
export const settings = reactive<AppSettings>({
  theme: 'dark',
  customTheme: { ...DEFAULT_CUSTOM_THEME },
  island: { ...DEFAULT_ISLAND },
  lang: 'auto',
  notifications: { ...DEFAULT_NOTIFICATIONS, privacy: { ...DEFAULT_PRIVACY } },
  diagnostics: { ...DEFAULT_DIAGNOSTICS },
  autoLaunch: true,
});

/** 归一化隐私配置：兼容旧版 privacy:boolean，补全缺省子项 */
function normalizePrivacy(raw: unknown): NotificationsPrivacy {
  if (typeof raw === 'boolean') return { ...DEFAULT_PRIVACY, enabled: raw };
  if (raw && typeof raw === 'object')
    return { ...DEFAULT_PRIVACY, ...(raw as Partial<NotificationsPrivacy>) };
  return { ...DEFAULT_PRIVACY };
}

/** 远端（其他窗口/持久化）数据灌进 store；缺省字段全部补全 */
function applyRemote(s: Partial<AppSettings> | null) {
  if (!s) return;
  if (s.theme && THEMES.some((t) => t.id === s.theme)) settings.theme = s.theme;
  if (s.customTheme) settings.customTheme = { ...DEFAULT_CUSTOM_THEME, ...s.customTheme };
  if (s.island) settings.island = { ...DEFAULT_ISLAND, ...s.island };
  if (s.lang) settings.lang = s.lang;
  if (s.notifications) {
    settings.notifications = {
      ...DEFAULT_NOTIFICATIONS,
      ...s.notifications,
      privacy: normalizePrivacy((s.notifications as { privacy?: unknown }).privacy),
    };
  }
  if (s.diagnostics) settings.diagnostics = { ...DEFAULT_DIAGNOSTICS, ...s.diagnostics };
  if (typeof s.autoLaunch === 'boolean') settings.autoLaunch = s.autoLaunch;
}

/** 岛布局 -> CSS 变量（缩放走 zoom；隐藏位移按露出高度换算，岛高 40px） */
function applyIslandStyle() {
  const root = document.documentElement;
  root.style.setProperty('--app-scale', String(settings.island.scale / 100));
  const shift = -(40 - settings.island.hiddenPeek);
  root.style.setProperty('--island-hidden-shift', `${shift}px`);
  root.style.setProperty('--island-hidden-shift-hover', `${shift + 4}px`);
}

/** #rrggbb -> 相对亮度（0~1，粗略 sRGB 加权） */
export function hexLuminance(hex: string): number {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return 0;
  const v = parseInt(m[1], 16);
  const r = (v >> 16) & 0xff;
  const g = (v >> 8) & 0xff;
  const b = v & 0xff;
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
}

/** 自定义主题由 JS 注入内联令牌（亮/暗系按背景平均亮度判定） */
const CUSTOM_VARS = [
  '--island-bg',
  '--ink',
  '--island-text',
  '--panel-bg',
  '--panel-border',
  '--accent',
  '--on-accent',
];

function applyTheme() {
  const root = document.documentElement;
  const meta = THEMES.find((t) => t.id === settings.theme) ?? THEMES[0];
  root.setAttribute('data-theme', meta.id);

  if (meta.id !== 'custom') {
    root.setAttribute('data-scheme', meta.scheme);
    for (const v of CUSTOM_VARS) root.style.removeProperty(v);
    return;
  }

  const { a, b, accent } = settings.customTheme;
  const light = (hexLuminance(a) + hexLuminance(b)) / 2 > 0.55;
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

/** 最近一次与远端同步过的快照（JSON）。远端灌进来的变更会把 lastSynced 一并推进，
 *  subscribe 里据此跳过回声写回——watch 回调是批处理异步触发的，
 *  用标志位挡不住异步回调，内容比对才可靠 */
let lastSynced = '';

let loaded = false;

export async function initSettings() {
  if (loaded) return;
  let saved: Partial<AppSettings> | null = null;
  try {
    saved = await api.storeGet<Partial<AppSettings>>('settings');
  } catch {
    // 后端 store 还没就绪（webview 可能先于 setup 加载）：稍后重试，
    // 不标记 loaded，否则主题会永远停在默认值
    window.setTimeout(() => {
      void initSettings();
    }, 500);
    return;
  }
  loaded = true;
  // 首次运行：跟随系统亮暗偏好选默认主题（黑/白仅是第一默认值）
  if (!saved?.theme) {
    settings.theme = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  applyRemote(saved);
  const { applyLangPref } = useI18n();
  applyTheme();
  applyIslandStyle();
  void applyLangPref(settings.lang);
  lastSynced = JSON.stringify(settings);

  // 本窗口的任何修改 -> 应用副作用 + 持久化并广播；与 lastSynced 相同说明来自远端，跳过
  watch(settings, () => {
    applyTheme();
    applyIslandStyle();
    void applyLangPref(settings.lang);
    const json = JSON.stringify(settings);
    if (json === lastSynced) return;
    lastSynced = json;
    api.settingsUpdate(JSON.parse(JSON.stringify(toRaw(settings))) as AppSettings).catch(() => {});
  });

  // 其他窗口的修改 -> 灌进本窗口（subscribe 的副作用部分照跑，持久化被 lastSynced 挡下）
  api.onSettingsChanged((s) => {
    applyRemote(s);
    lastSynced = JSON.stringify(settings);
  });
}

/** 快捷按钮：按注册顺序循环切换主题 */
export function toggleTheme() {
  const idx = THEMES.findIndex((t) => t.id === settings.theme);
  settings.theme = THEMES[(idx + 1) % THEMES.length].id;
}

/** 改色并即时切到 custom 主题 */
export function setCustomColor(part: keyof CustomTheme, hex: string) {
  settings.customTheme[part] = hex;
  if (settings.theme !== 'custom') settings.theme = 'custom';
}
