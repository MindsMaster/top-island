import { app, BrowserWindow, dialog, ipcMain, powerMonitor } from 'electron';
import * as fs from 'fs';
import * as path from 'path';
import { autoUpdater } from 'electron-updater';
import { IpcChannels } from '../../shared/ipc';
import type { AppVersionInfo, UpdateCheckResult } from '../../shared/ipc';

function isSnapshotVersion(version: string): boolean {
  return /SNAPSHOT$/i.test(version);
}

function readGitHash(): string {
  try {
    const pkg = JSON.parse(fs.readFileSync(path.join(app.getAppPath(), 'package.json'), 'utf-8')) as {
      gitHash?: string;
    };
    return pkg.gitHash ?? '';
  } catch {
    return '';
  }
}

function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

function fakeDevVersion(): string {
  const v = app.getVersion();
  if (v.endsWith('-SNAPSHOT')) return `${v.slice(0, -'-SNAPSHOT'.length)}-devsim-SNAPSHOT`;
  return `${v}-devsim`;
}

const HOUR_MS = 60 * 60 * 1000;

let last: UpdateCheckResult = { status: 'checking' };
let downloadedVersion: string | null = null;
let lastCheckDay = '';

function todayKey() {
  const d = new Date();
  return `${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}`;
}

function checkForUpdates(force: boolean) {
  if (downloadedVersion) return;
  const day = todayKey();
  if (!force && day === lastCheckDay) return;
  void autoUpdater
    .checkForUpdates()
    .then(() => {
      lastCheckDay = day;
    })
    .catch((err: Error) => {
      last = { status: 'error', message: err.message };
    });
}

function broadcastDownloaded(version: string) {
  for (const win of BrowserWindow.getAllWindows()) {
    if (!win.isDestroyed()) win.webContents.send(IpcChannels.updateDownloaded, { version });
  }
}

async function simulateDevUpdate(): Promise<UpdateCheckResult> {
  await sleep(450);
  const version = downloadedVersion ?? fakeDevVersion();
  downloadedVersion = version;
  last = { status: 'downloaded', version };
  broadcastDownloaded(version);
  return last;
}

export function startAutoUpdate() {
  if (!app.isPackaged) {
    last = { status: 'dev' };
    return;
  }

  const snapshot = isSnapshotVersion(app.getVersion());
  autoUpdater.autoDownload = true;
  autoUpdater.autoInstallOnAppQuit = true;
  autoUpdater.allowPrerelease = snapshot;
  autoUpdater.channel = snapshot ? 'snapshot' : 'latest';

  autoUpdater.on('update-available', (info) => {
    last = { status: 'available', version: info.version };
  });
  autoUpdater.on('update-not-available', () => {
    last = { status: 'not-available', version: app.getVersion() };
  });
  autoUpdater.on('update-downloaded', (info) => {
    downloadedVersion = info.version;
    last = { status: 'downloaded', version: info.version };
    broadcastDownloaded(info.version);
  });
  autoUpdater.on('error', (err) => {
    last = { status: 'error', message: err.message };
  });

  checkForUpdates(true);
  setInterval(() => checkForUpdates(false), HOUR_MS);
  powerMonitor.on('resume', () => checkForUpdates(false));
}

export function registerUpdateIpc() {
  ipcMain.handle(IpcChannels.appGetVersion, (): AppVersionInfo => ({
    version: app.getVersion(),
    gitHash: readGitHash(),
    packaged: app.isPackaged,
  }));

  ipcMain.handle(IpcChannels.updateStatus, (): UpdateCheckResult => last);

  ipcMain.handle(IpcChannels.updateCheck, async (): Promise<UpdateCheckResult> => {
    if (!app.isPackaged) return simulateDevUpdate();
    if (downloadedVersion) return { status: 'downloaded', version: downloadedVersion };
    if (last.status === 'available') return last;
    try {
      const r = await autoUpdater.checkForUpdates();
      lastCheckDay = todayKey();
      if (downloadedVersion) return { status: 'downloaded', version: downloadedVersion };
      const next = r?.updateInfo?.version;
      if (next && next !== app.getVersion()) {
        last = { status: 'available', version: next };
        return last;
      }
      last = { status: 'not-available', version: app.getVersion() };
      return last;
    } catch (e) {
      last = { status: 'error', message: e instanceof Error ? e.message : String(e) };
      return last;
    }
  });

  ipcMain.handle(IpcChannels.updateInstall, async () => {
    if (!app.isPackaged) {
      await dialog.showMessageBox({
        type: 'info',
        title: 'TopIsland',
        message: '开发模式：更新提示已模拟，不会安装。',
      });
      return;
    }
    autoUpdater.quitAndInstall(true, true);
  });
}
