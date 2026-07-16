import { app, BrowserWindow, clipboard, dialog, ipcMain, screen, shell } from 'electron';
import * as fs from 'fs';
import * as path from 'path';
import * as url from 'url';
import { IpcChannels } from '../../shared/ipc';
import type {
  AppSettings,
  MusicAction,
  NotificationItem,
  WeatherQueryOptions,
} from '../../shared/ipc';
import { store } from './store';
import { applyWindowLayout, getIslandWindow, openSettingsWindow } from './window';
import { diagLog, diagPath, timed } from './diag';
import {
  getArtwork,
  getLyrics,
  musicControl,
  musicSeek,
  pollMusicState,
  startMusicEvents,
} from './services/music';
import {
  activateNotification,
  foregroundExe,
  restoreAllBanners,
  startNotifications,
  stopNotifications,
  suppressBannerFor,
} from './services/notify';
import { acquireKey, hasKey, startWeChat, stopWeChat } from './services/wechat';
import { geocode, ipCity, weatherQuery } from './services/weather';
import { startClipboardEvents } from './services/clipboard';

/** 横幅接管是否生效（关闭时据此判断是否需还原） */
let bannerActive = false;

/** 新通知批次：推给岛；接管横幅开启时关掉来源应用的右下角弹窗 */
function handleIncoming(items: NotificationItem[]) {
  const win = getIslandWindow();
  if (win && !win.isDestroyed()) win.webContents.send(IpcChannels.notifyIncoming, items);
  const settings = store.get('settings') as AppSettings | undefined;
  if (settings?.notifications?.enabled && settings.notifications.suppressBanner) {
    for (const it of items) void suppressBannerFor(it.aumid);
  }
}

/**
 * 开机自启：默认开启（settings 缺 autoLaunch 字段时视为 true）。仅打包版写
 * 注册表 Run 项；dev 下跳过，否则注册的是 electron.exe。
 */
export function syncAutoLaunch(settings?: AppSettings | null) {
  if (!app.isPackaged) return;
  const openAtLogin = settings?.autoLaunch !== false;
  app.setLoginItemSettings({ openAtLogin });
}

/** 按当前设置启停消息托管 + 横幅接管（幂等，设置变更与启动时调用） */
export function syncNotifications() {
  const settings = store.get('settings') as AppSettings | undefined;
  if (settings?.notifications?.enabled) {
    startNotifications(handleIncoming);
  } else {
    stopNotifications();
  }
  const suppress = !!(settings?.notifications?.enabled && settings?.notifications?.suppressBanner);
  // 关闭接管（或整体关闭托管）时，还原此前改过的应用横幅
  if (bannerActive && !suppress) void restoreAllBanners();
  bannerActive = suppress;

  // 微信消息接入（独立于系统通知托管；仅需已缓存密钥）
  if (settings?.notifications?.enabled && settings.notifications.wechat && hasKey()) {
    startWeChat(async (items) => {
      const win = getIslandWindow();
      if (!win || win.isDestroyed()) return;
      // 已聚焦微信窗口时不弹提示（用户正在看微信）；取不到前台信息则照常弹
      const fg = (await foregroundExe().catch(() => '')).toLowerCase();
      if (fg === 'weixin' || fg === 'wechat') return;
      win.webContents.send(IpcChannels.notifyIncoming, items);
    });
  } else {
    stopWeChat();
  }
}

export function registerIpc() {
  // SMTC 事件 → 即时推最新音乐状态给岛（渲染层 2s 轮询之外的低延迟通道）
  startMusicEvents((state) => {
    const win = getIslandWindow();
    if (win && !win.isDestroyed()) win.webContents.send(IpcChannels.musicState, state);
  });

  // 剪贴板变化事件 → 通知岛（渲染层收到后自行读取内容，替代每秒轮询）
  startClipboardEvents(() => {
    const win = getIslandWindow();
    if (win && !win.isDestroyed()) win.webContents.send(IpcChannels.clipboardChanged);
  });

  ipcMain.handle(IpcChannels.windowSetIgnoreMouse, (event, ignore: boolean) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    win?.setIgnoreMouseEvents(ignore, { forward: true });
  });

  ipcMain.handle(IpcChannels.windowClose, () => {
    app.quit();
  });

  ipcMain.handle(IpcChannels.windowCloseSelf, (event) => {
    BrowserWindow.fromWebContents(event.sender)?.close();
  });

  ipcMain.handle(IpcChannels.windowGetCursorPoint, (event) => {
    const p = screen.getCursorScreenPoint();
    const b = BrowserWindow.fromWebContents(event.sender)?.getContentBounds();
    return b ? { x: p.x - b.x, y: p.y - b.y } : { x: p.x, y: p.y };
  });

  ipcMain.handle(IpcChannels.settingsOpen, () => {
    openSettingsWindow();
  });

  // 设置写入 + 广播给其余窗口（跳过发送方，避免回声）
  ipcMain.handle(IpcChannels.settingsUpdate, (event, settings: AppSettings) => {
    store.set('settings', settings);
    applyWindowLayout();
    syncNotifications();
    syncAutoLaunch(settings);
    for (const win of BrowserWindow.getAllWindows()) {
      if (win.webContents !== event.sender && !win.isDestroyed()) {
        win.webContents.send(IpcChannels.settingsChanged, settings);
      }
    }
  });

  ipcMain.handle(
    IpcChannels.notifyActivate,
    (_e, item: Pick<NotificationItem, 'aumid' | 'launch' | 'atype'>) => {
      // 微信消息卡片：拉起/聚焦微信（自绘应用无系统激活器，用协议唤起）
      if (item.aumid === 'wechat') {
        shell.openExternal('weixin://').catch(() => {});
        return Promise.resolve('wechat');
      }
      return activateNotification(item.aumid, item.launch, item.atype);
    }
  );

  ipcMain.handle(IpcChannels.wechatAcquireKey, async () => {
    const r = await acquireKey();
    // 拿到密钥后重启监听：切换账号时 key/目录都变了，必须先停（释放旧 watcher/timer）
    // 再以新账号重新拉起，否则幂等的 startWeChat 会沿用旧账号
    if (r.ok) {
      stopWeChat();
      syncNotifications();
    }
    return r;
  });
  ipcMain.handle(IpcChannels.wechatHasKey, () => hasKey());

  // 通知图片/头像 -> dataURL。来源多为聊天应用缓存到本地的头像文件；
  // ms-appx/ms-resource 这类包资源解析不了，返回 null 由渲染层回退首字母块
  const IMG_MIME: Record<string, string> = {
    '.png': 'image/png',
    '.jpg': 'image/jpeg',
    '.jpeg': 'image/jpeg',
    '.gif': 'image/gif',
    '.webp': 'image/webp',
    '.bmp': 'image/bmp',
    '.ico': 'image/x-icon',
  };
  const notifyImgCache = new Map<string, string | null>();

  ipcMain.handle(IpcChannels.notifyImage, async (_e, src: string) => {
    if (!src || typeof src !== 'string') return null;
    if (notifyImgCache.has(src)) return notifyImgCache.get(src);
    let out: string | null = null;
    try {
      let p = src.trim();
      if (/^https?:\/\//i.test(p)) {
        const res = await fetch(p);
        const ab = await res.arrayBuffer();
        if (res.ok && ab.byteLength > 0 && ab.byteLength <= 2 * 1024 * 1024) {
          const mime = res.headers.get('content-type')?.split(';')[0] || 'image/png';
          out = `data:${mime};base64,${Buffer.from(ab).toString('base64')}`;
        }
      } else {
        if (/^file:\/\//i.test(p)) p = url.fileURLToPath(p);
        if (fs.existsSync(p) && fs.statSync(p).size <= 2 * 1024 * 1024) {
          const mime = IMG_MIME[path.extname(p).toLowerCase()] || 'image/png';
          out = `data:${mime};base64,${fs.readFileSync(p).toString('base64')}`;
        }
      }
    } catch {
      out = null;
    }
    if (notifyImgCache.size > 200) notifyImgCache.clear();
    notifyImgCache.set(src, out);
    return out;
  });

  ipcMain.handle(IpcChannels.displaysList, () => {
    const primaryId = screen.getPrimaryDisplay().id;
    return screen.getAllDisplays().map((d, i) => ({
      id: String(d.id),
      primary: d.id === primaryId,
      label: `${d.label || `Display ${i + 1}`} (${d.bounds.width}x${d.bounds.height})`,
    }));
  });

  ipcMain.handle(IpcChannels.shellOpenExternal, (_e, target: string) => shell.openExternal(target));

  ipcMain.handle(IpcChannels.appGetLocale, () => app.getLocale());

  ipcMain.handle(IpcChannels.clipboardReadText, () => {
    try {
      return timed('clipboard:read-text', () => clipboard.readText() || '');
    } catch {
      return '';
    }
  });

  ipcMain.handle(IpcChannels.clipboardWriteText, (_e, text: string) => {
    try {
      clipboard.writeText(String(text ?? ''));
    } catch {}
  });

  ipcMain.handle(IpcChannels.clipboardHasImage, () => {
    try {
      return timed('clipboard:has-image', () =>
        clipboard.availableFormats().some((f) => f.startsWith('image/'))
      );
    } catch {
      return false;
    }
  });

  ipcMain.handle(IpcChannels.diagReveal, () => {
    diagLog('diag.log revealed by user');
    shell.showItemInFolder(diagPath());
  });

  ipcMain.handle(IpcChannels.clipboardReadFilePaths, () => {
    try {
      let paths = (clipboard as any).readFilePaths?.() || [];
      if (!paths.length) {
        const fileUrl = clipboard.read('public.file-url');
        if (fileUrl) {
          const fp = url.fileURLToPath(fileUrl.trim());
          if (fp && fs.existsSync(fp)) paths = [fp];
        }
      }
      return paths;
    } catch {
      return [];
    }
  });

  ipcMain.handle(IpcChannels.musicPoll, () => pollMusicState());
  ipcMain.handle(IpcChannels.musicControl, (_e, action: MusicAction, level?: number) =>
    musicControl(action, level)
  );
  ipcMain.handle(IpcChannels.musicSeek, (_e, positionMs: number) => musicSeek(positionMs));
  ipcMain.handle(IpcChannels.musicArtwork, (_e, hash: string) => getArtwork(hash));
  ipcMain.handle(IpcChannels.musicLyrics, (_e, id: string) => getLyrics(id));

  ipcMain.handle(IpcChannels.weatherIpCity, () => ipCity());
  ipcMain.handle(IpcChannels.weatherGeocode, (_e, city: string, lang: string) => geocode(city, lang));
  ipcMain.handle(IpcChannels.weatherQuery, (_e, lat: number, lon: number, opts?: WeatherQueryOptions) =>
    weatherQuery(lat, lon, opts)
  );

  ipcMain.handle(IpcChannels.storeGet, (_e, key: string) => store.get(key));
  ipcMain.handle(IpcChannels.storeSet, (_e, key: string, value: unknown) => store.set(key, value));

  // 清空全部数据并回到初次安装状态。
  ipcMain.handle(IpcChannels.storeClear, () => {
    store.clear();
    if (app.isPackaged) {
      // 打包版：换新进程从空 store 起步，最干净
      app.relaunch();
      app.exit(0);
      return;
    }
    // dev：进程由 dev.mjs 编排（electron 是其子进程），relaunch/exit 会让
    // dev.mjs 连带关掉 vite、新进程又无 server 可连。改为原地重置——按空
    // 配置重整主进程服务，再重载全部窗口，等效初次启动。
    applyWindowLayout();
    syncNotifications();
    for (const win of BrowserWindow.getAllWindows()) {
      if (!win.isDestroyed()) win.webContents.reload();
    }
  });

  ipcMain.handle(IpcChannels.alarmSoundList, () => {
    try {
      const mediaDir = path.join(process.env.WINDIR || 'C:\\Windows', 'Media');
      return fs
        .readdirSync(mediaDir)
        .filter((f) => /^Alarm\d+\.wav$/i.test(f))
        .sort()
        .map((f) => ({
          path: path.join(mediaDir, f),
          name: f.replace(/\.wav$/i, '').replace(/^Alarm0?/, 'Alarm '),
        }));
    } catch {
      return [];
    }
  });

  const AUDIO_MIME: Record<string, string> = {
    '.wav': 'audio/wav',
    '.mp3': 'audio/mpeg',
    '.ogg': 'audio/ogg',
    '.m4a': 'audio/mp4',
    '.flac': 'audio/flac',
  };

  ipcMain.handle(IpcChannels.alarmSoundData, (_e, p: string) => {
    try {
      const mime = AUDIO_MIME[path.extname(p).toLowerCase()];
      if (!mime) return null;
      if (fs.statSync(p).size > 20 * 1024 * 1024) return null; // 20MB 上限
      return `data:${mime};base64,${fs.readFileSync(p).toString('base64')}`;
    } catch {
      return null;
    }
  });

  ipcMain.handle(IpcChannels.alarmSoundPick, async (event) => {
    const win = BrowserWindow.fromWebContents(event.sender) ?? undefined;
    const r = await dialog.showOpenDialog(win!, {
      properties: ['openFile'],
      filters: [{ name: 'Audio', extensions: ['wav', 'mp3', 'ogg', 'm4a', 'flac'] }],
    });
    if (r.canceled || !r.filePaths.length) return null;
    const p = r.filePaths[0];
    return { path: p, name: path.basename(p, path.extname(p)) };
  });
}
