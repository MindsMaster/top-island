import { reactive } from 'vue';
import { notifyApi } from '@/platform/notify';
import { storeApi } from '@/platform/store';
import { showAlert } from '../store/alert';
import { settings } from '../store/settings';
import { useI18n } from '../i18n';
import type { NotificationItem } from '@/platform/types';

const { t } = useI18n();

/** 通知内容 + 稳定列表 key（wpndatabase Id 可能随库重建重置） */
export interface NotifEntry extends NotificationItem {
  key: string;
}

/**
 * 弹出卡片：一条消息一张卡。最新若干张各显其内容，
 * 超出可见上限的折叠成一行；每张各自到点、错峰逐条消失（新的活得更久）。
 */
export interface PopupCard {
  /** = 消息 entryKey，稳定不复用 */
  key: string;
  entry: NotifEntry;
  /** 自动消失时刻（悬停时暂停：到期但悬停中则顺延） */
  dismissAt: number;
}

const MAX_ITEMS = 100;

const POPUP_MS = 8000;
/** 悬停结束后的余量（避免刚移开就消失） */
const POPUP_LINGER_MS = 2000;
/** 同批多条错峰消失的步长（越新存活越久 -> 挨个撤掉） */
const POPUP_STAGGER_MS = 400;
/** 最多同时显示的完整卡（超出部分折叠成“还有 N 条”） */
export const MAX_VISIBLE = 3;
/** 弹层里最多保留的卡（含折叠的）；更旧的只进历史面板 */
const MAX_KEPT = 12;

/**
 * 渲染状态。组件模板直读字段，写只走本文件的 action。
 * items：历史面板列表；popups：弹窗卡片（含折叠的）；
 * hoveredPopup：悬停中的卡片 key（悬停暂停自动消失，也用于窗口鼠标穿透——
 * 只在卡上时才拦截点击，其余透传）；
 * images：已解析的图片（key -> dataURL），头像优先，回退应用图标。
 */
export const notifyState = reactive({
  items: [] as NotifEntry[],
  popups: [] as PopupCard[],
  hoveredPopup: null as string | null,
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

/** 激活来源应用；失败给提示 */
export async function activate(n: NotificationItem) {
  const method = await notifyApi.activate({ aumid: n.aumid, launch: n.launch, atype: n.atype });
  if (method === 'failed') {
    showAlert({ icon: 'fa-triangle-exclamation', text: t('notifyOpenFailed'), duration: 3000 });
  }
}

/** 同一会话（同一个人/群）的归并键：来源应用 + 标题/会话名 */
function conversationKey(e: NotifEntry): string {
  return `${e.aumid}||${e.title}`;
}

/** 点击卡片：打开来源应用，并收起同一会话的所有卡 */
export function activatePopup(card: PopupCard) {
  const gk = conversationKey(card.entry);
  notifyState.popups = notifyState.popups.filter((c) => conversationKey(c.entry) !== gk);
  if (notifyState.hoveredPopup && !notifyState.popups.some((c) => c.key === notifyState.hoveredPopup)) {
    notifyState.hoveredPopup = null;
  }
  void activate(card.entry);
}

/** 右上角关闭：只收起这一张 */
export function closePopup(key: string) {
  notifyState.popups = notifyState.popups.filter((c) => c.key !== key);
  if (notifyState.hoveredPopup === key) notifyState.hoveredPopup = null;
}

export function setPopupHover(key: string | null) {
  const prev = notifyState.hoveredPopup;
  notifyState.hoveredPopup = key;
  // 悬停结束：给该卡留一点余量再消失
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
    // 旧->新排序，一条一张卡；越新 dismissAt 越晚，配合错峰逐条消失
    const cards: PopupCard[] = [...fresh]
      .sort((a, b) => a.arrival - b.arrival)
      .map((entry, i) => ({ key: entry.key, entry, dismissAt: now + POPUP_MS + i * POPUP_STAGGER_MS }));
    cards.reverse(); // 新的在前
    notifyState.popups = [...cards, ...notifyState.popups].slice(0, MAX_KEPT);
  }
}

export async function initNotifications() {
  const saved = await storeApi.get<NotifEntry[]>('notificationHistory');
  if (saved && Array.isArray(saved)) {
    notifyState.items = saved.filter((s) => s && typeof s.id === 'number');
    // 历史头像按需解析（列表可见范围足够）
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
