import { createBridge, Bridge } from './winbridge';
import { store } from '../store';
import type { NotificationItem } from '../../../shared/ipc';

let bridge: Bridge | null = null;
let watching = false;
/** 已上报的最大 Id（去重水位）；-1 = 让 helper 取当前基线（不弹历史通知） */
let watermark = -1;
let onItems: ((items: NotificationItem[]) => void) | null = null;

function ensureBridge(): Bridge {
  if (!bridge) {
    bridge = createBridge({
      mode: 'notify',
      tag: 'notify helper',
      onEvent: handleEvent,
      onSpawn: () => {
        if (watching) void sendWatch();
      },
    });
  }
  return bridge;
}

function handleEvent(event: string, data: any) {
  if (event !== 'toasts' || !data || !watching) return;
  const items: NotificationItem[] = Array.isArray(data.items) ? data.items : [];
  const maxId = Number(data.maxId) || watermark;
  if (maxId > watermark) watermark = maxId;
  if (items.length && onItems) onItems(items);
}

async function sendWatch() {
  const resp = await ensureBridge().request('watch', { sinceId: watermark });
  if (resp?.ok && resp.data) {
    const maxId = Number(resp.data.maxId);
    if (Number.isFinite(maxId) && maxId > watermark) watermark = maxId;
  }
}

/** 启动消息托管（幂等）。cb 接收每批新通知 */
export function startNotifications(cb: (items: NotificationItem[]) => void) {
  onItems = cb;
  if (watching) return;
  watching = true;
  watermark = -1; // 每次开启重新取基线：开启前堆积的历史通知不弹
  void sendWatch();
}

/** 停止托管。helper 进程保留（激活/前台查询等能力独立于托管开关） */
export function stopNotifications() {
  if (!watching) return;
  watching = false;
  void bridge?.request('unwatch');
}

/** 当前前台窗口的进程名（不含 .exe，如 "Weixin"）；取不到返回 ''。用于“已在看该应用就别弹” */
export async function foregroundExe(): Promise<string> {
  const resp = await ensureBridge().request('foreground', {}, 2000);
  return resp?.ok && resp.data ? String(resp.data.exe || '') : '';
}

/** 复现点击：激活来源应用（COM 深链 -> AAM -> shell 兜底）。返回激活方式 */
export async function activateNotification(aumid: string, launch: string, atype: string): Promise<string> {
  // 激活独立于托管开关：即便用户临时关了托管也应能点开已收到的消息
  const resp = await ensureBridge().request('activate', { aumid, launch, atype });
  return resp?.ok && resp.data ? String(resp.data.method) : 'failed';
}

// 接管系统横幅：把来源应用的 ShowBanner 关掉（仍进通知中心）。改动可逆：
// 记录每个被改应用的原值到 store，关闭时还原。-1 表示原本没有该值（还原=删除）。
const SUPPRESS_KEY = 'notifySuppressedApps';

function suppressedMap(): Record<string, number> {
  return (store.get(SUPPRESS_KEY) as Record<string, number>) || {};
}

/** 关掉某应用的右下角横幅（记录原值以便还原）。已处理过的跳过 */
export async function suppressBannerFor(aumid: string) {
  if (!aumid) return;
  const map = suppressedMap();
  if (aumid in map) return;
  const resp = await ensureBridge().request('bannerGet', { aumid });
  const prior = resp?.ok && resp.data && typeof resp.data.value === 'number' ? resp.data.value : -1;
  await ensureBridge().request('bannerSet', { aumid, value: 0 });
  map[aumid] = prior;
  store.set(SUPPRESS_KEY, map);
}

export async function restoreAllBanners() {
  const map = suppressedMap();
  for (const aumid of Object.keys(map)) {
    await ensureBridge().request('bannerSet', { aumid, value: map[aumid] });
  }
  store.set(SUPPRESS_KEY, {});
}

export function disposeNotifications() {
  watching = false;
  bridge?.dispose();
  bridge = null;
}
