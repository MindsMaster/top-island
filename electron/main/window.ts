import { app, BrowserWindow, screen } from 'electron';
import * as path from 'path';
import type { AppSettings } from '../../shared/ipc';
import { store } from './store';

let islandWindow: BrowserWindow | null = null;

export function getIslandWindow() {
  return islandWindow;
}

function setIgnore(win: BrowserWindow, ignore: boolean) {
  win.setIgnoreMouseEvents(ignore, { forward: true });
}

const ISLAND_BASE_W = 900;
const ISLAND_BASE_H = 440;

function currentLayout(): { scale: number; displayId: string } {
  const s = store.get<Partial<AppSettings>>('settings');
  const pct = s?.island?.scale ?? 100;
  return {
    scale: Math.min(3.0, Math.max(0.45, pct / 100)),
    displayId: s?.island?.displayId ?? 'primary',
  };
}

function resolveDisplay(displayId: string) {
  if (displayId !== 'primary') {
    const d = screen.getAllDisplays().find((x) => String(x.id) === displayId);
    if (d) return d;
  }
  return screen.getPrimaryDisplay();
}

function islandBounds() {
  const { scale, displayId } = currentLayout();
  const d = resolveDisplay(displayId).bounds;
  const w = Math.round(ISLAND_BASE_W * scale);
  const h = Math.round(ISLAND_BASE_H * scale);
  return { x: Math.round(d.x + (d.width - w) / 2), y: d.y, width: w, height: h };
}

interface AuxWindowSpec {
  baseW: number;
  baseH: number;
  /** 距所在屏顶部的基准偏移（随缩放等比放大） */
  offsetY: number;
}

const auxWindows = new Map<BrowserWindow, AuxWindowSpec>();

function registerAuxWindow(win: BrowserWindow, spec: AuxWindowSpec) {
  auxWindows.set(win, spec);
  win.on('closed', () => auxWindows.delete(win));
  layoutAuxWindow(win, spec);
}

function layoutAuxWindow(win: BrowserWindow, spec: AuxWindowSpec) {
  const { scale, displayId } = currentLayout();
  const d = resolveDisplay(displayId).bounds;
  const w = Math.round(spec.baseW * scale);
  const h = Math.round(spec.baseH * scale);
  win.setBounds({
    x: Math.round(d.x + (d.width - w) / 2),
    y: d.y + Math.round(spec.offsetY * scale),
    width: w,
    height: h,
  });
}

/** 设置（缩放/所在显示器）或显示器拓扑变化后，重排全部窗口 */
export function applyWindowLayout() {
  if (islandWindow && !islandWindow.isDestroyed()) {
    islandWindow.setBounds(islandBounds());
  }
  for (const [win, spec] of auxWindows) {
    if (!win.isDestroyed()) layoutAuxWindow(win, spec);
  }
}

export function createIslandWindow() {
  const { x, y, width, height } = islandBounds();

  islandWindow = new BrowserWindow({
    width,
    height,
    x,
    y,
    frame: false,
    transparent: true,
    alwaysOnTop: true,
    skipTaskbar: true,
    hasShadow: false,
    resizable: false,
    backgroundColor: '#00000000',
    type: 'toolbar',
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
      preload: path.join(app.getAppPath(), 'dist', 'preload.js'),
    },
  });

  islandWindow.webContents.on('render-process-gone', (_event, details) => {
    console.error('[Island] Render process gone:', details.reason, details.exitCode);
    app.quit();
  });

  islandWindow.on('unresponsive', () => {
    console.warn('[Island] Renderer unresponsive, restoring mouse forwarding');
    if (islandWindow && !islandWindow.isDestroyed()) setIgnore(islandWindow, true);
  });

  islandWindow.setAlwaysOnTop(true, 'screen-saver');

  setIgnore(islandWindow, true);

  const devUrl = process.env.VITE_DEV_SERVER_URL;
  if (devUrl) {
    islandWindow.loadURL(devUrl);
  } else {
    islandWindow.loadFile(path.join(app.getAppPath(), 'dist', 'renderer', 'index.html'));
  }

  islandWindow.on('closed', () => {
    islandWindow = null;
  });

  // 分辨率/显示器拓扑变化时重排全部窗口
  screen.on('display-metrics-changed', applyWindowLayout);
  screen.on('display-added', applyWindowLayout);
  screen.on('display-removed', applyWindowLayout);

  return islandWindow;
}

const SETTINGS_W = 480;
const SETTINGS_H = 420;

let settingsWindow: BrowserWindow | null = null;

export function openSettingsWindow() {
  if (settingsWindow && !settingsWindow.isDestroyed()) {
    settingsWindow.show();
    settingsWindow.focus();
    return;
  }

  settingsWindow = new BrowserWindow({
    width: SETTINGS_W,
    height: SETTINGS_H,
    parent: islandWindow && !islandWindow.isDestroyed() ? islandWindow : undefined,
    frame: false,
    transparent: true,
    alwaysOnTop: true,
    skipTaskbar: true,
    resizable: false,
    hasShadow: false,
    backgroundColor: '#00000000',
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
      preload: path.join(app.getAppPath(), 'dist', 'preload.js'),
    },
  });

  // 与岛同层级（kSecuritySurface），避免被 Chromium 全屏降级逻辑压到游戏下
  settingsWindow.setAlwaysOnTop(true, 'screen-saver');

  // 注册进集中布局：跟随岛所在屏与缩放，实时重排
  registerAuxWindow(settingsWindow, { baseW: SETTINGS_W, baseH: SETTINGS_H, offsetY: 64 });

  const devUrl = process.env.VITE_DEV_SERVER_URL;
  if (devUrl) {
    settingsWindow.loadURL(devUrl + '#settings');
  } else {
    settingsWindow.loadFile(path.join(app.getAppPath(), 'dist', 'renderer', 'index.html'), {
      hash: 'settings',
    });
  }

  settingsWindow.on('closed', () => {
    settingsWindow = null;
  });
}
