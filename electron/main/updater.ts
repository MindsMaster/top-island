import { app, dialog, ipcMain } from 'electron';
import * as fs from 'fs';
import * as path from 'path';
import { autoUpdater } from 'electron-updater';
import { IpcChannels } from '../../shared/ipc';
import type { AppVersionInfo, UpdateCheckResult } from '../../shared/ipc';
import { getIslandWindow } from './window';

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

let last: UpdateCheckResult = { status: 'checking' };
let downloadedVersion: string | null = null;

function broadcastDownloaded(version: string) {
  const win = getIslandWindow();
  if (win && !win.isDestroyed()) {
    win.webContents.send(IpcChannels.updateDownloaded, { version });
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

  void autoUpdater.checkForUpdates().catch((err: Error) => {
    last = { status: 'error', message: err.message };
  });
}

export function registerUpdateIpc() {
  ipcMain.handle(IpcChannels.appGetVersion, (): AppVersionInfo => ({
    version: app.getVersion(),
    gitHash: readGitHash(),
    packaged: app.isPackaged,
  }));

  ipcMain.handle(IpcChannels.updateCheck, async (): Promise<UpdateCheckResult> => {
    if (!app.isPackaged) return simulateDevUpdate();
    if (downloadedVersion) return { status: 'downloaded', version: downloadedVersion };
    try {
      const r = await autoUpdater.checkForUpdates();
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
    autoUpdater.quitAndInstall(false, true);
  });
}
