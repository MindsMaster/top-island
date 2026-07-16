import { computed, ref } from 'vue';
import { api } from '../api';
import { useAlert } from './useAlert';
import { useI18n } from '../i18n';
import { useSettings } from './useSettings';

export type ClipType = 'url' | 'email' | 'phone' | 'image' | 'audio' | 'video' | 'file' | 'json' | 'text';

export interface ClipItem {
  id: string;
  text: string;
  time: number;
  source: 'system' | 'manual' | 'ai';
  type: ClipType;
}

const { t } = useI18n();
const alert = useAlert();

const history = ref<ClipItem[]>([]);
const pinned = ref<string[]>([]);
const search = ref('');

let watching = false;

export function detectContentType(text: string): ClipType {
  if (!text) return 'text';
  const t = text.trim();
  if (/\.(mp3|wav|ogg|flac|aac|m4a|wma)$/i.test(t)) return 'audio';
  if (/\.(mp4|webm|mov|avi|mkv|wmv|flv)$/i.test(t)) return 'video';
  if (/\.(png|jpg|jpeg|gif|webp|svg|bmp|ico|tiff)$/i.test(t)) return 'image';
  if (!/^https?:\/\//i.test(t) && /\.(pdf|doc|docx|xls|xlsx|ppt|pptx)$/i.test(t)) return 'file';
  if (/^https?:\/\/\S+$/i.test(t)) return 'url';
  if (/^[\w.-]+@[\w.-]+\.\w+$/.test(t)) return 'email';
  if (/^(\d{3,4}[-.\s]?\d{3,4}[-.\s]?\d{4,6})$/.test(t)) return 'phone';
  if (/^[\[{]/.test(t) && /[\]}]$/.test(t)) return 'json';
  return 'text';
}

export function openUrl(url: string) {
  let finalUrl = url;
  if (!/^https?:\/\/|mailto:|tel:|file:\/\//i.test(url)) {
    finalUrl = 'file://' + (url.startsWith('/') ? '' : '/') + url;
  }
  api.openExternal(finalUrl).catch(() => {});
}

export function firstUrl(text: string): string | null {
  const m = /https?:\/\/[^\s<>"'`]+/i.exec(text || '');
  return m ? m[0].replace(/[.,;:!?)\]}'"]+$/, '') : null; // 去掉尾部误粘的标点
}

async function initClipboard() {
  history.value = ((await api.storeGet<ClipItem[]>('clipboardHistory')) || []).filter(
    (h) => h && typeof h.text === 'string'
  );
  history.value.forEach((h) => {
    if (!h.type) h.type = detectContentType(h.text || '');
  });
  pinned.value = (await api.storeGet<string[]>('clipboardPinned')) || [];
}

function save() {
  api.storeSet('clipboardHistory', JSON.parse(JSON.stringify(history.value)));
}

function savePinned() {
  api.storeSet('clipboardPinned', JSON.parse(JSON.stringify(pinned.value)));
}

function alertForItem(item: ClipItem) {
  // 多个链接只弹第一个
  const url = item.type === 'url' ? item.text.trim() : firstUrl(item.text);
  if (url) {
    alert.show({
      icon: 'fa-globe',
      text: url.substring(0, 60) + (url.length > 60 ? '...' : ''),
      actionLabel: t('alertOpenWeb'),
      actionHandler: () => openUrl(url),
      duration: 6000,
    });
    return;
  }
  if (item.type === 'email') {
    alert.show({
      icon: 'fa-envelope',
      text: item.text.substring(0, 60) + (item.text.length > 60 ? '...' : ''),
      actionLabel: t('alertCompose'),
      actionHandler: () => openUrl('mailto:' + item.text),
      duration: 6000,
    });
  }
}

function addItem(text: string, source: ClipItem['source']) {
  if (!text) return;
  const latest = history.value[0];
  if (latest && latest.text === text) {
    // 内容未变则跳过，不重复落盘；只补历史遗留条目缺失的字段
    let changed = false;
    if (!latest.source) {
      latest.source = source || 'manual';
      changed = true;
    }
    if (!latest.type) {
      latest.type = detectContentType(text);
      changed = true;
    }
    if (changed) save();
    return;
  }
  history.value = [
    {
      id: Date.now().toString(36) + Math.random().toString(36).slice(2, 5),
      text,
      time: Date.now(),
      source: source || 'manual',
      type: detectContentType(text),
    },
    ...history.value,
  ].slice(0, 100);
  save();
  if (source === 'system') alertForItem(history.value[0]);
}

async function readCurrent() {
  try {
    const filePaths = await api.clipboardReadFilePaths();
    if (filePaths && filePaths.length > 0) {
      for (const fp of filePaths) addItem(fp, 'system');
      return;
    }
  } catch {}
  try {
    const text = await api.clipboardReadText();
    if (text) {
      addItem(text, 'system');
      return;
    }
  } catch {}
  try {
    // 不用 readImage：主进程同步解码位图会拖垮全局鼠标钩子致系统级卡顿
    if (await api.clipboardHasImage()) addItem(t('clipboardImagePlaceholder'), 'system');
  } catch {}
}

function startClipboardWatch() {
  if (watching) return;
  watching = true;
  const { diagnostics } = useSettings();
  // 事件驱动：剪贴板变化才读一次（不再轮询）；诊断开关关时忽略。readCurrent 内部去重
  api.onClipboardChanged(() => {
    if (diagnostics.value.clipboardPoll) void readCurrent();
  });
}

function write(text: string) {
  api.clipboardWriteText(text).catch(() => {});
}

function copy(text: string) {
  write(text);
  addItem(text, 'manual');
}

function togglePin(id: string) {
  const idx = pinned.value.indexOf(id);
  if (idx >= 0) pinned.value.splice(idx, 1);
  else pinned.value.unshift(id);
  savePinned();
}

function isPinned(id: string) {
  return pinned.value.includes(id);
}

function deleteItem(id: string) {
  history.value = history.value.filter((h) => h.id !== id);
  pinned.value = pinned.value.filter((pid) => pid !== id);
  save();
  savePinned();
  if (history.value.length === 0) write('');
}

function clearAll() {
  history.value = [];
  pinned.value = [];
  search.value = '';
  save();
  savePinned();
  write('');
}

function formatTime(ts: number): string {
  const diff = Date.now() - ts;
  if (diff < 60000) return t('timeJustNow');
  if (diff < 3600000) return Math.floor(diff / 60000) + t('timeMinutesAgo');
  if (diff < 86400000) return Math.floor(diff / 3600000) + t('timeHoursAgo');
  const d = new Date(ts);
  return (
    d.getMonth() +
    1 +
    '/' +
    d.getDate() +
    ' ' +
    String(d.getHours()).padStart(2, '0') +
    ':' +
    String(d.getMinutes()).padStart(2, '0')
  );
}

function typeIcon(type: ClipType): string {
  const icons: Record<ClipType, string> = {
    url: 'fa-globe',
    email: 'fa-envelope',
    phone: 'fa-phone',
    image: 'fa-image',
    audio: 'fa-headphones',
    video: 'fa-video',
    file: 'fa-file-lines',
    json: 'fa-code',
    text: 'fa-align-left',
  };
  return 'fa-solid ' + (icons[type] || 'fa-align-left');
}

function canOpenPath(item: ClipItem): boolean {
  if (item.type === 'url' || item.type === 'email') return true;
  return /^(https?:\/\/|file:\/\/|\/|[A-Za-z]:[\\/])/.test(item.text.trim());
}

const filtered = computed(() => {
  const all = [
    ...(pinned.value.map((id) => history.value.find((h) => h.id === id)).filter(Boolean) as ClipItem[]),
    ...history.value.filter((h) => !pinned.value.includes(h.id)),
  ];
  if (!search.value) return all;
  const q = search.value.toLowerCase();
  return all.filter((h) => h.text.toLowerCase().includes(q));
});

export function useClipboard() {
  return {
    history,
    pinned,
    search,
    filtered,
    initClipboard,
    startClipboardWatch,
    readCurrent,
    addItem,
    copy,
    write,
    togglePin,
    isPinned,
    deleteItem,
    clearAll,
    formatTime,
    typeIcon,
    canOpenPath,
  };
}
