import { call } from './invoke';
import type { AppVersionInfo, DisplayInfo } from './types';

export const systemApi = {
  quit: () => call('app_quit'),
  locale: () => call<string>('app_get_locale'),
  version: () => call<AppVersionInfo>('app_get_version'),
  displays: () => call<DisplayInfo[]>('displays_list'),
  /** 后端只放行 http mailto */
  openExternal: (url: string) => call('shell_open_external', { url }),
  revealDataDir: () => call('diag_reveal'),
};
