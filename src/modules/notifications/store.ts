import { computed, reactive } from 'vue';
import { notifyApi } from '@/platform/notify';
import { storeApi } from '@/platform/store';
import { showAlert } from '@/ui/alert';
import { settings } from '@/core/settings';
import { useI18n } from '@/core/i18n';
import type { NotificationItem } from '@/platform/types';

const { t } = useI18n();

/** wpndatabase Id 随库重建重置 key 须自算 */
export interface NotifEntry extends NotificationItem {
  key: string;
}

export interface PopupCard {
  key: string;
  entry: NotifEntry;
  /** 悬停中到期顺延 */
  dismissAt: number;
}

const MAX_ITEMS = 100;

const POPUP_MS = 8000;
const POPUP_LINGER_MS = 2000;
const POPUP_STAGGER_MS = 400;
export const MAX_VISIBLE = 3;
const MAX_KEPT = 12;

/** images 为已解析 dataURL */
export const notifyState = reactive({
  items: [] as NotifEntry[],
  popups: [] as PopupCard[],
  hoveredPopup: null as string | null,
  images: {} as Record<string, string>,
});

export const visiblePopups = computed(() => notifyState.popups.slice(0, MAX_VISIBLE));
export const foldedCount = computed(() => Math.max(0, notifyState.popups.length - MAX_VISIBLE));

let subscribed = false;
let pruneTimer: number | null = null;

function save() {
  storeApi.set('notificationHistory', JSON.parse(JSON.stringify(notifyState.items))).catch(() => {});
}

function entryKey(n: NotificationItem): string {
  return `${n.id}-${n.arrival}`;
}

async function resolveImage(e: NotifEntry) {
  if (notifyState.images[e.key]) return;
  for (const src of [e.image, e.icon]) {
    if (!src) continue;
    const data = await notifyApi.image(src).catch(() => null);
    if (data) {
      notifyState.images[e.key] = data;
      return;
    }
  }
}

export async function activate(n: NotificationItem) {
  const method = await notifyApi.activate({ aumid: n.aumid, launch: n.launch, atype: n.atype });
  if (method === 'failed') {
    showAlert({ icon: 'fa-triangle-exclamation', text: t('notifyOpenFailed'), duration: 3000 });
  }
}

/** 标题充当会话名 */
function conversationKey(e: NotifEntry): string {
  return `${e.aumid}||${e.title}`;
}

export function activatePopup(card: PopupCard) {
  const gk = conversationKey(card.entry);
  notifyState.popups = notifyState.popups.filter((c) => conversationKey(c.entry) !== gk);
  if (notifyState.hoveredPopup && !notifyState.popups.some((c) => c.key === notifyState.hoveredPopup)) {
    notifyState.hoveredPopup = null;
  }
  void activate(card.entry);
}

export function closePopup(key: string) {
  notifyState.popups = notifyState.popups.filter((c) => c.key !== key);
  if (notifyState.hoveredPopup === key) notifyState.hoveredPopup = null;
}

export function setPopupHover(key: string | null) {
  const prev = notifyState.hoveredPopup;
  notifyState.hoveredPopup = key;
  // 悬停结束补余量
  if (prev && prev !== key) {
    const now = Date.now();
    notifyState.popups = notifyState.popups.map((c) =>
      c.key === prev && c.dismissAt < now + POPUP_LINGER_MS ? { ...c, dismissAt: now + POPUP_LINGER_MS } : c
    );
  }
}

function prunePopups() {
  if (!notifyState.popups.length) return;
  const now = Date.now();
  const next = notifyState.popups.filter((c) => c.dismissAt > now || c.key === notifyState.hoveredPopup);
  if (next.length !== notifyState.popups.length) notifyState.popups = next;
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
    const now = Date.now();
    // 越新越晚消失
    const cards: PopupCard[] = [...fresh]
      .sort((a, b) => a.arrival - b.arrival)
      .map((entry, i) => ({ key: entry.key, entry, dismissAt: now + POPUP_MS + i * POPUP_STAGGER_MS }));
    cards.reverse();
    notifyState.popups = [...cards, ...notifyState.popups].slice(0, MAX_KEPT);
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
