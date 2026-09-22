import { computed, ref } from 'vue';
import { systemApi } from '@/platform/system';
import type { Lang, Messages } from './types';
import zhCN from './locales/zh-CN';
import enUS from './locales/en-US';

export type { Lang, Messages } from './types';

const MESSAGES: Record<Lang, Messages> = {
  'zh-CN': zhCN,
  'en-US': enUS,
};

const lang = ref<Lang>('zh-CN');

function t(key: string, arg?: string | number): string {
  const dict = MESSAGES[lang.value] || MESSAGES['zh-CN'];
  const raw = dict[key] ?? MESSAGES['zh-CN'][key] ?? key;
  const str = typeof raw === 'string' ? raw : key;
  return arg != null ? str.replace('{0}', String(arg)) : str;
}

const weekdays = computed(() => MESSAGES[lang.value].weekdays);

function weatherCodeName(code: number | null): string {
  if (code === null) return t('weatherLoading');
  return MESSAGES[lang.value].weatherCodes[code] || t('weatherUnknown');
}

async function applyLangPref(pref: 'auto' | Lang) {
  if (pref !== 'auto') {
    lang.value = pref;
    return;
  }
  try {
    const locale = await systemApi.locale();
    lang.value = locale.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US';
  } catch {
    lang.value = (navigator.language || '').startsWith('zh') ? 'zh-CN' : 'en-US';
  }
}

async function initI18n() {
  await applyLangPref('auto');
  document.title = lang.value === 'zh-CN' ? '灵动岛' : 'Top Island';
}

export function useI18n() {
  return { lang, t, weekdays, weatherCodeName, initI18n, applyLangPref };
}
