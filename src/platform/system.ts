import { call } from './invoke';
import type { AppVersionInfo, DisplayInfo } from './types';

export const systemApi = {
  locale: () => call<string>('app_get_locale'),
  version: () => call<AppVersionInfo>('app_get_version'),
  /** 枚举显示器（岛落屏设置用） */
  displays: () => call<DisplayInfo[]>('displays_list'),
  /** 后端只放行 http(s)/mailto */
  openExternal: (url: string) => call('shell_open_external', { url }),
  /** 在资源管理器中定位数据目录 */
  revealDataDir: () => call('diag_reveal'),
};
