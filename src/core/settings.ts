import { reactive, toRaw, watch } from 'vue';
import { settingsApi } from '@/platform/settings';
import { storeApi } from '@/platform/store';
import type {
  AppSettings,
  CustomTheme,
  DiagnosticsToggles,
  IslandLayout,
  MusicConfig,
  NotificationsConfig,
  NotificationsPrivacy,
} from '@/platform/types';
import { useI18n } from './i18n';
import { THEMES, applyTheme, nextThemeId } from './theme';

const DEFAULT_DIAGNOSTICS: DiagnosticsToggles = {
  clipboardPoll: true,
  musicPoll: true,
  devOverlay: false,
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

const DEFAULT_MUSIC: MusicConfig = { neteaseBridge: true };

/** 加字段须同步 platform/types 与默认值常量 */
export const settings = reactive<AppSettings>({
  theme: 'dark',
  customTheme: { ...DEFAULT_CUSTOM_THEME },
  island: { ...DEFAULT_ISLAND },
  lang: 'auto',
  notifications: { ...DEFAULT_NOTIFICATIONS, privacy: { ...DEFAULT_PRIVACY } },
  diagnostics: { ...DEFAULT_DIAGNOSTICS },
  music: { ...DEFAULT_MUSIC },
  autoLaunch: true,
});

/** 兼容旧版 boolean */
function normalizePrivacy(raw: unknown): NotificationsPrivacy {
  if (typeof raw === 'boolean') return { ...DEFAULT_PRIVACY, enabled: raw };
  if (raw && typeof raw === 'object')
    return { ...DEFAULT_PRIVACY, ...(raw as Partial<NotificationsPrivacy>) };
  return { ...DEFAULT_PRIVACY };
}

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
  if (s.music) settings.music = { ...DEFAULT_MUSIC, ...s.music };
  if (typeof s.autoLaunch === 'boolean') settings.autoLaunch = s.autoLaunch;
}

/** 岛高 40 与 _base.scss 一致 */
function applyIslandStyle() {
  const root = document.documentElement;
  root.style.setProperty('--app-scale', String(settings.island.scale / 100));
  const shift = -(40 - settings.island.hiddenPeek);
  root.style.setProperty('--island-hidden-shift', `${shift}px`);
  root.style.setProperty('--island-hidden-shift-hover', `${shift + 4}px`);
}

function applyAll() {
  applyTheme(settings.theme, settings.customTheme);
  applyIslandStyle();
  void useI18n().applyLangPref(settings.lang);
}

/** 防回声 只比内容 */
let lastSynced = '';
let loaded = false;

export async function initSettings() {
  if (loaded) return;
  let saved: Partial<AppSettings> | null = null;
  try {
    saved = await storeApi.get<Partial<AppSettings>>('settings');
  } catch {
    // store 未就绪 不标 loaded 否则停在默认值
    window.setTimeout(() => void initSettings(), 500);
    return;
  }
  loaded = true;
  if (!saved?.theme) {
    settings.theme = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  applyRemote(saved);
  applyAll();
  lastSynced = JSON.stringify(settings);

  watch(settings, () => {
    applyAll();
    const json = JSON.stringify(settings);
    if (json === lastSynced) return;
    lastSynced = json;
    settingsApi.update(JSON.parse(JSON.stringify(toRaw(settings))) as AppSettings).catch(() => {});
  });

  settingsApi.onChanged((s) => {
    applyRemote(s);
    lastSynced = JSON.stringify(settings);
  });
}

export function toggleTheme() {
  settings.theme = nextThemeId(settings.theme);
}

export function setCustomColor(part: keyof CustomTheme, hex: string) {
  settings.customTheme[part] = hex;
  if (settings.theme !== 'custom') settings.theme = 'custom';
}
