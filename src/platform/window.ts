import { call, on } from './invoke';
import type { HotRect } from './types';

export const windowApi = {
  /** null 表全程可交互 或与交互区同 */
  setHotRect: (interactive: HotRect | null, hover: HotRect | null) =>
    call('window_set_hot_rect', { interactive, hover }),

  /** 退整个应用 */
  quit: () => call('window_close'),

  /** 仅关本窗口 */
  closeSelf: () => call('window_close_self'),

  /** 穿透态唯一可靠悬停源 */
  onHover: (cb: (inside: boolean) => void) => on<boolean>('island:hover', cb),

  /** 热区物理坐标变了须重报 */
  onGeometryChanged: (cb: () => void) => {
    on('tauri://move', cb);
    on('tauri://resize', cb);
    on('tauri://scale-change', cb);
  },
};
