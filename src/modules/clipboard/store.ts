import { computed, reactive } from 'vue';
import { clipboardApi } from '@/platform/clipboard';
import { storeApi } from '@/platform/store';
import { systemApi } from '@/platform/system';
import { showAlert } from '@/ui/alert';
import { useI18n } from '@/core/i18n';
import { settings } from '@/core/settings';

export type ClipType = 'url' | 'email' | 'phone' | 'image' | 'audio' | 'video' | 'file' | 'json' | 'text';

export interface ClipItem {
  id: string;
  text: string;
  time: number;
  source: 'system' | 'manual' | 'ai';
  type: ClipType;
}

const { t } = useI18n();

export const clipboardState = reactive({
  history: [] as ClipItem[],
  pinned: [] as string[],
  search: '',
});

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
  systemApi.openExternal(finalUrl).catch(() => {});
}

export function firstUrl(text: string): string | null {
  const m = /https?:\/\/[^\s<>"'`]+/i.exec(text || '');
  return m ? m[0].replace(/[.,;:!?)\]}'"]+$/, '') : null; // 去尾部误粘标点
}

export async function initClipboard() {
  clipboardState.history = ((await storeApi.get<ClipItem[]>('clipboardHistory')) || []).filter(
    (h) => h && typeof h.text === 'string'
  );
  clipboardState.history.forEach((h) => {
    if (!h.type) h.type = detectContentType(h.text || '');
  });
  clipboardState.pinned = (await storeApi.get<string[]>('clipboardPinned')) || [];
}

function save() {
  storeApi.set('clipboardHistory', JSON.parse(JSON.stringify(clipboardState.history)));
}

function savePinned() {
  storeApi.set('clipboardPinned', JSON.parse(JSON.stringify(clipboardState.pinned)));
}

function alertForItem(item: ClipItem) {
  // 多链接只弹第一个
  const url = item.type === 'url' ? item.text.trim() : firstUrl(item.text);
  if (url) {
    showAlert({
      icon: 'fa-globe',
      text: url.substring(0, 60) + (url.length > 60 ? '...' : ''),
      actionLabel: t('alertOpenWeb'),
      actionHandler: () => openUrl(url),
      duration: 6000,
    });
    return;
  }
  if (item.type === 'email') {
    showAlert({
      icon: 'fa-envelope',
      text: item.text.substring(0, 60) + (item.text.length > 60 ? '...' : ''),
      actionLabel: t('alertCompose'),
      actionHandler: () => openUrl('mailto:' + item.text),
      duration: 6000,
    });
  }
}

export function addItem(text: string, source: ClipItem['source']) {
  if (!text) return;
  const latest = clipboardState.history[0];
  if (latest && latest.text === text) {
    // 未变则只补遗留字段
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
  clipboardState.history = [
    {
      id: Date.now().toString(36) + Math.random().toString(36).slice(2, 5),
      text,
      time: Date.now(),
      source: source || 'manual',
      type: detectContentType(text),
    },
    ...clipboardState.history,
  ].slice(0, 100);
  save();
  if (source === 'system') alertForItem(clipboardState.history[0]);
}

export async function readCurrent() {
  try {
    const filePaths = await clipboardApi.readFilePaths();
    if (filePaths && filePaths.length > 0) {
      for (const fp of filePaths) addItem(fp, 'system');
      return;
    }
  } catch {}
  try {
    const text = await clipboardApi.readText();
    if (text) {
      addItem(text, 'system');
      return;
    }
  } catch {}
  try {
    // 只探测不解码 解码拖垮鼠标钩子
    if (await clipboardApi.hasImage()) addItem(t('clipboardImagePlaceholder'), 'system');
  } catch {}
}

export function startClipboardWatch() {
  if (watching) return;
  watching = true;
  // 重复事件内部去重
  clipboardApi.onChanged(() => {
    if (settings.diagnostics.clipboardPoll) void readCurrent();
  });
}

export function write(text: string) {
  clipboardApi.writeText(text).catch(() => {});
}

export function copy(text: string) {
  write(text);
  addItem(text, 'manual');
}

export function togglePin(id: string) {
  const idx = clipboardState.pinned.indexOf(id);
  if (idx >= 0) clipboardState.pinned.splice(idx, 1);
  else clipboardState.pinned.unshift(id);
  savePinned();
}

export function isPinned(id: string) {
  return clipboardState.pinned.includes(id);
}

export function deleteItem(id: string) {
  clipboardState.history = clipboardState.history.filter((h) => h.id !== id);
  clipboardState.pinned = clipboardState.pinned.filter((pid) => pid !== id);
  save();
  savePinned();
  if (clipboardState.history.length === 0) write('');
}

export function clearAll() {
  clipboardState.history = [];
  clipboardState.pinned = [];
  clipboardState.search = '';
  save();
  savePinned();
  write('');
}

/** v-model 回写端 */
export function setSearch(v: string) {
  clipboardState.search = v;
}

export function formatTime(ts: number): string {
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

export function typeIcon(type: ClipType): string {
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

export function canOpenPath(item: ClipItem): boolean {
  if (item.type === 'url' || item.type === 'email') return true;
  return /^(https?:\/\/|file:\/\/|\/|[A-Za-z]:[\\/])/.test(item.text.trim());
}

export const filteredClips = computed(() => {
  const { history, pinned, search } = clipboardState;
  const all = [
    ...(pinned.map((id) => history.find((h) => h.id === id)).filter(Boolean) as ClipItem[]),
    ...history.filter((h) => !pinned.includes(h.id)),
  ];
  if (!search) return all;
  const q = search.toLowerCase();
  return all.filter((h) => h.text.toLowerCase().includes(q));
});
