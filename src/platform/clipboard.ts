import { call, on } from './invoke';

export const clipboardApi = {
  readText: () => call<string>('clipboard_read_text'),
  writeText: (text: string) => call('clipboard_write_text', { text }),

  /** 只探测是否含图片，绝不解码位图：后端同步解码会拖垮全局鼠标钩子 */
  hasImage: () => call<boolean>('clipboard_has_image'),

  readFilePaths: () => call<string[]>('clipboard_read_file_paths'),

  /** 系统剪贴板变化（winbridge 事件驱动，无 payload；回调里自行读取内容） */
  onChanged: (cb: () => void) => on('clipboard:changed', cb),
};
