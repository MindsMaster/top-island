import { computed, ref } from 'vue';
import { api } from '../api';
import { useAlert } from './useAlert';
import { useSettings } from './useSettings';
import { useI18n } from '../i18n';
import type { NotificationItem } from '../../shared/ipc';

const { t } = useI18n();
const alert = useAlert();

/** 通知内容 + 稳定列表 key（wpndatabase Id 可能随库重建重置） */
export interface NotifEntry extends NotificationItem {
  key: string;
}

/**
 * 弹出卡片：一条消息一张卡。最新若干张各显其内容，
 * 超出可见上限的折叠成一行；每张各自到点、错峰逐条消失（新的活得更久）。
 */
interface PopupCard {
  /** = 消息 entryKey，稳定不复用 */
  key: string;
  entry: NotifEntry;
  /** 自动消失时刻（悬停时暂停：到期但悬停中则顺延） */
  dismissAt: number;
}

const items = ref<NotifEntry[]>([]);
const MAX_ITEMS = 100;

const popups = ref<PopupCard[]>([]);
/** 悬停中的卡片 key（悬停暂停自动消失） */
const hoveredPopup = ref<string | null>(null);
const POPUP_MS = 8000;
/** 悬停结束后的余量（避免刚移开就消失） */
const POPUP_LINGER_MS = 2000;
/** 同批多条错峰消失的步长（越新存活越久 -> 挨个撤掉） */
const POPUP_STAGGER_MS = 400;
/** 最多同时显示的完整卡 */
const MAX_VISIBLE = 3;
/** 弹层里最多保留的卡（含折叠的）；更旧的只进历史面板 */
const MAX_KEPT = 12;

/** 已解析的图片（key -> dataURL）；头像优先，回退应用图标 */
const images = ref<Record<string, string>>({});

let subscribed = false;
let pruneTimer: number | null = null;

function save() {
  api.storeSet('notificationHistory', JSON.parse(JSON.stringify(items.value))).catch(() => {});
}

function entryKey(n: NotificationItem): string {
  return `${n.id}-${n.arrival}`;
}

async function resolveImage(e: NotifEntry) {
  if (images.value[e.key]) return;
  for (const src of [e.image, e.icon]) {
    if (!src) continue;
    const data = await api.notifyImage(src).catch(() => null);
    if (data) {
      images.value = { ...images.value, [e.key]: data };
      return;
    }
  }
}

/** 激活来源应用；失败给提示 */
async function activate(n: NotificationItem) {
  const method = await api.notifyActivate({ aumid: n.aumid, launch: n.launch, atype: n.atype });
  if (method === 'failed') {
    alert.show({ icon: 'fa-triangle-exclamation', text: t('notifyOpenFailed'), duration: 3000 });
  }
}

/** 同一会话（同一个人/群）的归并键：来源应用 + 标题/会话名 */
function conversationKey(e: NotifEntry): string {
  return `${e.aumid}||${e.title}`;
}

/** 点击卡片：打开来源应用，并收起同一会话的所有卡 */
function activatePopup(card: PopupCard) {
  const gk = conversationKey(card.entry);
  popups.value = popups.value.filter((c) => conversationKey(c.entry) !== gk);
  if (hoveredPopup.value && !popups.value.some((c) => c.key === hoveredPopup.value)) {
    hoveredPopup.value = null;
  }
  void activate(card.entry);
}

/** 右上角关闭：只收起这一张 */
function closePopup(key: string) {
  popups.value = popups.value.filter((c) => c.key !== key);
  if (hoveredPopup.value === key) hoveredPopup.value = null;
}

function setPopupHover(key: string | null) {
  const prev = hoveredPopup.value;
  hoveredPopup.value = key;
  // 悬停结束：给该卡留一点余量再消失
  if (prev && prev !== key) {
    const now = Date.now();
    popups.value = popups.value.map((c) =>
      c.key === prev && c.dismissAt < now + POPUP_LINGER_MS ? { ...c, dismissAt: now + POPUP_LINGER_MS } : c
    );
  }
}

function prunePopups() {
  if (!popups.value.length) return;
  const now = Date.now();
  const next = popups.value.filter((c) => c.dismissAt > now || c.key === hoveredPopup.value);
  if (next.length !== popups.value.length) popups.value = next;
}

function onIncoming(batch: NotificationItem[]) {
  const { notifications } = useSettings();
  const existing = new Set(items.value.map((i) => i.key));
  const fresh: NotifEntry[] = [];
  for (const n of batch) {
    const key = entryKey(n);
    if (existing.has(key)) continue;
    existing.add(key);
    fresh.push({ ...n, key });
  }
  if (!fresh.length) return;
  fresh.sort((a, b) => b.arrival - a.arrival);
  items.value = [...fresh, ...items.value].sort((a, b) => b.arrival - a.arrival).slice(0, MAX_ITEMS);
  save();
  for (const e of fresh) void resolveImage(e);

  if (notifications.value.popup) {
    const now = Date.now();
    // 旧->新排序，一条一张卡；越新 dismissAt 越晚，配合错峰逐条消失
    const cards: PopupCard[] = [...fresh]
      .sort((a, b) => a.arrival - b.arrival)
      .map((entry, i) => ({ key: entry.key, entry, dismissAt: now + POPUP_MS + i * POPUP_STAGGER_MS }));
    cards.reverse(); // 新的在前
    popups.value = [...cards, ...popups.value].slice(0, MAX_KEPT);
  }
}

async function initNotifications() {
  const saved = await api.storeGet<NotifEntry[]>('notificationHistory');
  if (saved && Array.isArray(saved)) {
    items.value = saved.filter((s) => s && typeof s.id === 'number');
    // 历史头像按需解析（列表可见范围足够）
    for (const e of items.value.slice(0, 40)) void resolveImage(e);
  }
  if (!subscribed) {
    subscribed = true;
    api.onNotifications(onIncoming);
  }
  if (pruneTimer === null) pruneTimer = window.setInterval(prunePopups, 400);
}

function remove(key: string) {
  items.value = items.value.filter((i) => i.key !== key);
  save();
}

function clearAll() {
  items.value = [];
  save();
}

function formatTime(ts: number): string {
  const diff = Date.now() - ts;
  if (diff < 60000) return t('timeJustNow');
  if (diff < 3600000) return Math.floor(diff / 60000) + t('timeMinutesAgo');
  if (diff < 86400000) return Math.floor(diff / 3600000) + t('timeHoursAgo');
  const d = new Date(ts);
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(
    d.getMinutes()
  ).padStart(2, '0')}`;
}

const hasItems = computed(() => items.value.length > 0);
const hasPopups = computed(() => popups.value.length > 0);
const visiblePopups = computed(() => popups.value.slice(0, MAX_VISIBLE));
/** 折叠掉的更旧消息条数（>0 时在栈底显示“还有 N 条”） */
const foldedCount = computed(() => Math.max(0, popups.value.length - MAX_VISIBLE));
/** 鼠标是否真的悬在某张弹窗卡上（用于窗口鼠标穿透：只在卡上时才拦截点击，其余透传） */
const hoveringPopup = computed(() => hoveredPopup.value !== null);

export function useNotifications() {
  return {
    items,
    hasItems,
    popups,
    hasPopups,
    visiblePopups,
    foldedCount,
    hoveringPopup,
    images,
    initNotifications,
    activate,
    activatePopup,
    closePopup,
    setPopupHover,
    remove,
    clearAll,
    formatTime,
  };
}
