import { createBridge, Bridge } from '../winbridge';

export interface SmtcSession {
  title: string;
  artist: string;
  app: string;
  playing: boolean;
  positionMs: number;
  durationMs: number;
  seekSupported: boolean;
}

export interface SmtcThumbnail {
  hash: string;
  b64: string;
}

let bridge: Bridge | null = null;
let disposed = false;
let onChange: (() => void) | null = null;

function ensureBridge(): Bridge | null {
  if (disposed) return null;
  if (!bridge) {
    bridge = createBridge({
      mode: 'smtc',
      tag: 'SMTC helper',
      onEvent: (event) => {
        if (event === 'state') onChange?.();
      },
      onSpawn: () => void bridge?.request('watch'),
    });
  }
  return bridge;
}

/** 订阅 SMTC 会话变化（helper 侧已防抖、内容去重） */
export function onSmtcChange(cb: () => void) {
  onChange = cb;
  ensureBridge();
}

export async function queryWindows(): Promise<SmtcSession | null> {
  const resp = await ensureBridge()?.request('query');
  if (resp && resp.ok && resp.data) return resp.data as SmtcSession;
  return null;
}

/** 读取当前会话专辑封面（原始 base64），无封面返回 null。可能耗时较长，超时放宽。 */
export async function thumbnailWindows(): Promise<SmtcThumbnail | null> {
  const resp = await ensureBridge()?.request('thumbnail', {}, 10000);
  if (resp && resp.ok && resp.data) return resp.data as SmtcThumbnail;
  return null;
}

export async function seekWindows(positionMs: number): Promise<boolean> {
  const resp = await ensureBridge()?.request('seek', { positionMs: Math.max(0, Math.round(positionMs)) });
  return !!(resp && resp.ok);
}

export async function controlWindows(action: string, level?: number): Promise<string> {
  switch (action) {
    case 'play':
      await ensureBridge()?.request('play');
      return '已发送播放指令。';
    case 'pause':
      await ensureBridge()?.request('pause');
      return '已发送暂停指令。';
    case 'next':
      await ensureBridge()?.request('next');
      return '已切换到下一首。';
    case 'prev':
      await ensureBridge()?.request('prev');
      return '已切换到上一首。';
    case 'volume': {
      const lvl = Math.max(0, Math.min(100, Math.round(level ?? 50)));
      const resp = await ensureBridge()?.request('volume', { level: lvl });
      if (resp && resp.ok) return `音量已设置为 ${lvl}%`;
      return '无法调整音量。';
    }
    default:
      return '未知操作';
  }
}

export function disposeWindowsHelper() {
  disposed = true;
  bridge?.dispose();
  bridge = null;
}
