import { call, on } from './invoke';
import type { AppSettings } from './types';

export const settingsApi = {
  /** 打开（或聚焦已打开的）设置窗口 */
  open: () => call('settings_open'),

  /** 设置窗每次被打开时触发（窗口常驻不销毁，靠它重播进入动画） */
  onOpened: (cb: () => void) => on('settings:opened', cb),

  /** 持久化设置并广播给其他窗口 */
  update: (settings: AppSettings) => call('settings_update', { settings }),

  onChanged: (cb: (settings: AppSettings) => void) => on<AppSettings>('settings:changed', cb),
};
