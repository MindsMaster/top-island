import { call, on } from './invoke';
import type { NotificationItem } from './types';

export const notifyApi = {
  onIncoming: (cb: (items: NotificationItem[]) => void) => on<NotificationItem[]>('notify:incoming', cb),

  /** 复现点击：激活来源应用（能拿到深链就跳到具体会话）。返回实际生效方式 */
  activate: (item: Pick<NotificationItem, 'aumid' | 'launch' | 'atype'>) =>
    call<string>('notify_activate', {
      aumid: item.aumid,
      launch: item.launch,
      atype: item.atype,
    }),

  /** 通知图片/头像 → data URL（渲染层加载不了本地路径）；不可解析返回 null */
  image: (src: string) => call<string | null>('notify_image', { src }),
};
