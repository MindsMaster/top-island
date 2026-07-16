import { app, Menu, nativeImage, Tray } from 'electron';
import { getIslandWindow } from './window';
import { resourcePath } from './paths';

let tray: Tray | null = null;

export function createTray() {
  const iconPath = resourcePath('icon_tray.png');
  let icon = nativeImage.createFromPath(iconPath);
  if (icon.isEmpty()) icon = nativeImage.createEmpty();

  tray = new Tray(icon);
  tray.setToolTip('Top Island');
  tray.setContextMenu(
    Menu.buildFromTemplate([
      {
        label: '打开开发者工具',
        click: () => getIslandWindow()?.webContents.openDevTools({ mode: 'detach' }),
      },
      { type: 'separator' },
      { label: '退出', click: () => app.quit() },
    ])
  );
  return tray;
}
