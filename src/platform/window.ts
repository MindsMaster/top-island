import { call, on } from './invoke';
import type { HotRect } from './types';

export const windowApi = {
  /** 上报岛窗热区：interactive 切穿透（null = 全程可交互），hover 判岛悬停（null = 与交互区同） */
  setHotRect: (interactive: HotRect | null, hover: HotRect | null) =>
    call('window_set_hot_rect', { interactive, hover }),

  /** 退出整个应用 */
  quit: () => call('window_close'),

  /** 仅关闭调用方所在窗口 */
  closeSelf: () => call('window_close_self'),

  /**
   * 全局光标位置（相对调用方窗口内容区）。
   * 鼠标快速划出屏幕时 mouseleave 可能永不触发，悬停看门狗据此兜底校验。
   */
  cursorPoint: () => call<{ x: number; y: number }>('window_get_cursor_point'),

  /** Rust 钩子判定的光标进出热区（穿透态下唯一可靠的悬停来源） */
  onHover: (cb: (inside: boolean) => void) => on<boolean>('island-hover', cb),

  /** 窗口移动/缩放/DPI 变化：热区物理坐标变了，需要重报 */
  onGeometryChanged: (cb: () => void) => {
    on('tauri://move', cb);
    on('tauri://resize', cb);
    on('tauri://scale-change', cb);
  },
};
