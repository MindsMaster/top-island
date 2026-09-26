import { call, on } from './invoke';
import type { AppSettings } from './types';

export const settingsApi = {
  /** 已开则聚焦 */
  open: (section?: string) => call('settings_open', { section: section ?? null }),

  onSection: (cb: (section: string) => void) => on<string>('settings:section', cb),

  /** 每次开窗触发 窗口常驻 */
  onOpened: (cb: () => void) => on('settings:opened', cb),

  /** 持久化并跨窗广播 */
  update: (settings: AppSettings) => call('settings_update', { settings }),

  onChanged: (cb: (settings: AppSettings) => void) => on<AppSettings>('settings:changed', cb),
};
