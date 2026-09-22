import { call, on } from './invoke';

export const clipboardApi = {
  readText: () => call<string>('clipboard_read_text'),
  writeText: (text: string) => call('clipboard_write_text', { text }),

  /** 仅探测不解码 防卡鼠标钩子 */
  hasImage: () => call<boolean>('clipboard_has_image'),

  readFilePaths: () => call<string[]>('clipboard_read_file_paths'),

  /** winbridge 事件驱动 无 payload */
  onChanged: (cb: () => void) => on('clipboard:changed', cb),
};
