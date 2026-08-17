import { app } from 'electron';
import { registerIpc, syncAutoLaunch, syncNotifications } from './ipc';
import { registerUpdateIpc, startAutoUpdate } from './updater';
import type { AppSettings } from '../../shared/ipc';
import { createIslandWindow } from './window';
import { createTray } from './tray';
import { disposeMusic } from './services/music';
import { disposeNotifications } from './services/notify';
import { disposeWeChat } from './services/wechat';
import { disposeClipboard } from './services/clipboard';
import { store } from './store';
import { startJankMonitor } from './diag';

app.commandLine.appendSwitch(
  'disable-features',
  'CalculateNativeWinOcclusion,MediaSessionService,HardwareMediaKeyHandling'
);

const gotLock = app.requestSingleInstanceLock();
if (!gotLock) {
  app.quit();
} else {
  app.whenReady().then(() => {
    registerIpc();
    registerUpdateIpc();
    createIslandWindow();
    createTray();
    startJankMonitor();
    syncNotifications();
    // 首次运行 settings 无 autoLaunch 字段 -> 默认开启并写入自启
    syncAutoLaunch(store.get('settings') as AppSettings | undefined);
    startAutoUpdate();
  });

  app.on('window-all-closed', () => {
    app.quit();
  });

  app.on('will-quit', () => {
    disposeMusic();
    disposeNotifications();
    disposeWeChat();
    disposeClipboard();
    store.flush();
  });
}
