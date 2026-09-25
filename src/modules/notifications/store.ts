import { reactive, watch } from 'vue';
import { notifyApi } from '@/platform/notify';
import { storeApi } from '@/platform/store';
import { mediaUrl } from '@/platform/media';
import { showAlert } from '@/ui/alert';
import { settings } from '@/core/settings';
import { shell } from '@/shell/commands';
import { shellView } from '@/shell/view';
import { useI18n } from '@/core/i18n';
import type { NotificationItem } from '@/platform/types';

const { t } = useI18n();

/** wpndatabase Id 随库重建重置 key 须自算 */
export interface NotifEntry extends NotificationItem {
  key: string;
}

const MAX_ITEMS = 100;

const POPUP_MS = 5000;
const POPUP_LINGER_MS = 2000;

/** images 值为空串表示候选图全部失败 */
export const notifyState = reactive({
  items: [] as NotifEntry[],
  /** 新来的直接顶替 */
  popup: null as NotifEntry | null,
  popupHovered: false,
  dismissAt: 0,
  images: {} as Record<string, string>,
});

let subscribed = false;
let pruneTimer: number | null = null;

function save() {
  storeApi.set('notificationHistory', JSON.parse(JSON.stringify(notifyState.items))).catch(() => {});
}

function entryKey(n: NotificationItem): string {
  return `${n.id}-${n.arrival}`;
}

function imageCandidates(e: NotifEntry): string[] {
  return [e.image, e.icon].filter((src): src is string => !!src).map((src) => mediaUrl('notify', src));
}

function resolveImage(e: NotifEntry) {
  if (e.key in notifyState.images) return;
  notifyState.images[e.key] = imageCandidates(e)[0] ?? '';
}

/** 同图在弹窗与面板各挂一份 只认当前候选的失败 */
export function imageFailed(e: NotifEntry, ev: Event) {
  const failed = (ev.target as HTMLImageElement).getAttribute('src');
  if (!failed || notifyState.images[e.key] !== failed) return;
  const candidates = [...new Set(imageCandidates(e))];
  notifyState.images[e.key] = candidates[candidates.indexOf(failed) + 1] ?? '';
}

export async function activate(n: NotificationItem) {
  const method = await notifyApi.activate({ aumid: n.aumid, launch: n.launch, atype: n.atype });
  if (method === 'failed') {
    showAlert({ icon: 'fa-triangle-exclamation', text: t('notifyOpenFailed'), duration: 3000 });
  }
}

export function openPopup() {
  closePopup();
  shell.openPanel('messages');
}

export function closePopup() {
  notifyState.popup = null;
  // 条被卸载 收不到 mouseleave
  notifyState.popupHovered = false;
}

export function setPopupHover(hovered: boolean) {
  notifyState.popupHovered = hovered;
  // 悬停结束补余量
  if (!hovered) notifyState.dismissAt = Math.max(notifyState.dismissAt, Date.now() + POPUP_LINGER_MS);
}

function prunePopups() {
  if (!notifyState.popup || notifyState.popupHovered) return;
  // 没露出来就不计时 展开操作或被提醒压住时攒着
  if (shellView.capsuleOwner !== 'notifications') {
    notifyState.dismissAt = Date.now() + POPUP_MS;
    return;
  }
  if (Date.now() >= notifyState.dismissAt) notifyState.popup = null;
}

function onIncoming(batch: NotificationItem[]) {
  const existing = new Set(notifyState.items.map((i) => i.key));
  const fresh: NotifEntry[] = [];
  for (const n of batch) {
    const key = entryKey(n);
    if (existing.has(key)) continue;
    existing.add(key);
    fresh.push({ ...n, key });
  }
  if (!fresh.length) return;
  fresh.sort((a, b) => b.arrival - a.arrival);
  notifyState.items = [...fresh, ...notifyState.items]
    .sort((a, b) => b.arrival - a.arrival)
    .slice(0, MAX_ITEMS);
  save();
  for (const e of fresh) void resolveImage(e);

  if (settings.notifications.popup) {
    notifyState.popup = fresh[0];
    notifyState.dismissAt = Date.now() + POPUP_MS;
  }
}

export async function initNotifications() {
  const saved = await storeApi.get<NotifEntry[]>('notificationHistory');
  if (saved && Array.isArray(saved)) {
    notifyState.items = saved.filter((s) => s && typeof s.id === 'number');
    // 头像只解析前 40 条
    for (const e of notifyState.items.slice(0, 40)) void resolveImage(e);
  }
  if (!subscribed) {
    subscribed = true;
    notifyApi.onIncoming(onIncoming);
    // 打开消息页即算看过
    watch(
      () => shellView.panel,
      (p) => {
        if (p === 'messages') notifyState.popup = null;
      }
    );
  }
  if (pruneTimer === null) pruneTimer = window.setInterval(prunePopups, 400);
}

export function remove(key: string) {
  notifyState.items = notifyState.items.filter((i) => i.key !== key);
  save();
}

export function clearAll() {
  notifyState.items = [];
  save();
}

export function formatTime(ts: number): string {
  const diff = Date.now() - ts;
  if (diff < 60000) return t('timeJustNow');
  if (diff < 3600000) return Math.floor(diff / 60000) + t('timeMinutesAgo');
  if (diff < 86400000) return Math.floor(diff / 3600000) + t('timeHoursAgo');
  const d = new Date(ts);
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(
    d.getMinutes()
  ).padStart(2, '0')}`;
}
