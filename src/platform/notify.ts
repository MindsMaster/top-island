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
};
