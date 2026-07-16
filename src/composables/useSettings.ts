import { ref, watch } from 'vue';
import { api } from '../api';
import { useI18n } from '../i18n';
import type {
  AppSettings,
  CustomTheme,
  DiagnosticsToggles,
  IslandLayout,
  LangPref,
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

/** 归一化隐私配置：兼容旧版 privacy:boolean，补全缺省子项 */
function normalizePrivacy(raw: unknown): NotificationsPrivacy {
  if (typeof raw === 'boolean') return { ...DEFAULT_PRIVACY, enabled: raw };
  if (raw && typeof raw === 'object')
    return { ...DEFAULT_PRIVACY, ...(raw as Partial<NotificationsPrivacy>) };
  return { ...DEFAULT_PRIVACY };
}

const theme = ref<ThemeId>('dark');
const customTheme = ref<CustomTheme>({ ...DEFAULT_CUSTOM_THEME });
const island = ref<IslandLayout>({ ...DEFAULT_ISLAND });
const lang = ref<LangPref>('auto');
const notifications = ref<NotificationsConfig>({ ...DEFAULT_NOTIFICATIONS });
const diagnostics = ref<DiagnosticsToggles>({ ...DEFAULT_DIAGNOSTICS });
const autoLaunch = ref(true);

const { applyLangPref } = useI18n();

let loaded = false;
/** 最近一次与外部同步过的设置快照（JSON）；watch 里据此跳过回声写回 */
let lastSynced = '';

function snapshot(): AppSettings {
  return {
    theme: theme.value,
    customTheme: { ...customTheme.value },
    island: { ...island.value },
    lang: lang.value,
    notifications: { ...notifications.value, privacy: { ...notifications.value.privacy } },
    diagnostics: { ...diagnostics.value },
    autoLaunch: autoLaunch.value,
  };
}

function apply(s: Partial<AppSettings> | null) {
  if (!s) return;
  if (s.theme && THEMES.some((t) => t.id === s.theme)) theme.value = s.theme;
  if (s.customTheme) customTheme.value = { ...DEFAULT_CUSTOM_THEME, ...s.customTheme };
  if (s.island) island.value = { ...DEFAULT_ISLAND, ...s.island };
  if (s.lang) lang.value = s.lang;
  if (s.notifications) {
    notifications.value = {
      ...DEFAULT_NOTIFICATIONS,
      ...s.notifications,
      privacy: normalizePrivacy((s.notifications as { privacy?: unknown }).privacy),
    };
  }
  if (s.diagnostics) diagnostics.value = { ...DEFAULT_DIAGNOSTICS, ...s.diagnostics };
  if (typeof s.autoLaunch === 'boolean') autoLaunch.value = s.autoLaunch;
}

/** 岛布局 -> CSS 变量（缩放走 zoom；隐藏位移按露出高度换算，岛高 40px） */
function applyIslandStyle() {
  const root = document.documentElement;
  root.style.setProperty('--app-scale', String(island.value.scale / 100));
  const shift = -(40 - island.value.hiddenPeek);
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
  const meta = THEMES.find((t) => t.id === theme.value) ?? THEMES[0];
  root.setAttribute('data-theme', meta.id);

  if (meta.id !== 'custom') {
    root.setAttribute('data-scheme', meta.scheme);
    for (const v of CUSTOM_VARS) root.style.removeProperty(v);
    return;
  }

  const { a, b, accent } = customTheme.value;
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

async function initSettings() {
  if (loaded) return;
  loaded = true;
  const saved = await api.storeGet<Partial<AppSettings>>('settings');
  // 首次运行：跟随系统亮暗偏好选默认主题（黑/白仅是第一默认值）
  if (!saved?.theme) {
    theme.value = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  apply(saved);
  applyTheme();
  applyIslandStyle();
  void applyLangPref(lang.value);
  lastSynced = JSON.stringify(snapshot());

  // 本窗口的修改 -> 持久化并广播；与 lastSynced 相同说明来自远端应用，跳过
  watch([theme, customTheme, island, lang, notifications, diagnostics, autoLaunch], () => {
    applyTheme();
    applyIslandStyle();
    void applyLangPref(lang.value);
    const json = JSON.stringify(snapshot());
    if (json === lastSynced) return;
    lastSynced = json;
    api.settingsUpdate(snapshot()).catch(() => {});
  });

  // 其他窗口的修改 -> 应用到本窗口
  api.onSettingsChanged((s) => {
    apply(s);
    applyTheme();
    applyIslandStyle();
    void applyLangPref(lang.value);
    lastSynced = JSON.stringify(snapshot());
  });
}

/** 快捷按钮：按注册顺序循环切换主题 */
function toggleTheme() {
  const idx = THEMES.findIndex((t) => t.id === theme.value);
  theme.value = THEMES[(idx + 1) % THEMES.length].id;
  applyTheme();
}

function setDiagnostic(key: keyof DiagnosticsToggles, on: boolean) {
  diagnostics.value = { ...diagnostics.value, [key]: on };
}

/** 改色并即时切到 custom 主题（整体替换以触发 watch） */
function setCustomColor(part: keyof CustomTheme, hex: string) {
  customTheme.value = { ...customTheme.value, [part]: hex };
  if (theme.value !== 'custom') theme.value = 'custom';
  applyTheme();
}

/** 整体替换以触发 watch */
function setIsland(patch: Partial<IslandLayout>) {
  island.value = { ...island.value, ...patch };
}

/** 整体替换以触发 watch */
function setNotifications(patch: Partial<NotificationsConfig>) {
  notifications.value = { ...notifications.value, ...patch };
}

/** 整体替换以触发 watch */
function setPrivacy(patch: Partial<NotificationsPrivacy>) {
  notifications.value = {
    ...notifications.value,
    privacy: { ...notifications.value.privacy, ...patch },
  };
}

export function useSettings() {
  return {
    theme,
    customTheme,
    island,
    lang,
    notifications,
    diagnostics,
    autoLaunch,
    initSettings,
    toggleTheme,
    setDiagnostic,
    setCustomColor,
    setIsland,
    setNotifications,
    setPrivacy,
  };
}
