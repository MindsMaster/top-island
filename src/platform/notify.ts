import { call, on } from './invoke';
import type { NotificationItem } from './types';

export const notifyApi = {
  onIncoming: (cb: (items: NotificationItem[]) => void) => on<NotificationItem[]>('notify:incoming', cb),

  /** 返回实际生效方式 */
  activate: (item: Pick<NotificationItem, 'aumid' | 'launch' | 'atype'>) =>
    call<string>('notify_activate', {
      aumid: item.aumid,
      launch: item.launch,
      atype: item.atype,
    }),

  /** 本地路径转 data URL 失败返回 null */
  image: (src: string) => call<string | null>('notify_image', { src }),
};
