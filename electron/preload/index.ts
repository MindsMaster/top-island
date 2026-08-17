import { contextBridge, ipcRenderer } from 'electron';
import { IpcChannels } from '../../shared/ipc';
import type {
  AppSettings,
  AppVersionInfo,
  IslandApi,
  MusicAction,
  MusicState,
  NotificationItem,
  UpdateCheckResult,
  WeatherQueryOptions,
} from '../../shared/ipc';

let settingsChangedCb: ((settings: AppSettings) => void) | null = null;
ipcRenderer.on(IpcChannels.settingsChanged, (_e, settings: AppSettings) => {
  settingsChangedCb?.(settings);
});

let musicStateCb: ((state: MusicState) => void) | null = null;
ipcRenderer.on(IpcChannels.musicState, (_e, state: MusicState) => {
  musicStateCb?.(state);
});

let notificationsCb: ((items: NotificationItem[]) => void) | null = null;
ipcRenderer.on(IpcChannels.notifyIncoming, (_e, items: NotificationItem[]) => {
  notificationsCb?.(items);
});

let clipboardChangedCb: (() => void) | null = null;
ipcRenderer.on(IpcChannels.clipboardChanged, () => {
  clipboardChangedCb?.();
});

let updateDownloadedCb: ((info: { version: string }) => void) | null = null;
ipcRenderer.on(IpcChannels.updateDownloaded, (_e, info: { version: string }) => {
  updateDownloadedCb?.(info);
});

const api: IslandApi = {
  setIgnoreMouseEvents: (ignore: boolean) => ipcRenderer.invoke(IpcChannels.windowSetIgnoreMouse, ignore),
  closeWindow: () => ipcRenderer.invoke(IpcChannels.windowClose),
  closeSelf: () => ipcRenderer.invoke(IpcChannels.windowCloseSelf),
  getCursorPoint: () => ipcRenderer.invoke(IpcChannels.windowGetCursorPoint),
  openSettings: () => ipcRenderer.invoke(IpcChannels.settingsOpen),
  settingsUpdate: (settings: AppSettings) => ipcRenderer.invoke(IpcChannels.settingsUpdate, settings),
  onSettingsChanged: (cb: (settings: AppSettings) => void) => {
    settingsChangedCb = cb;
  },
  openExternal: (url: string) => ipcRenderer.invoke(IpcChannels.shellOpenExternal, url),
  getLocale: () => ipcRenderer.invoke(IpcChannels.appGetLocale),
  clipboardReadText: () => ipcRenderer.invoke(IpcChannels.clipboardReadText),
  clipboardWriteText: (text: string) => ipcRenderer.invoke(IpcChannels.clipboardWriteText, text),
  clipboardHasImage: () => ipcRenderer.invoke(IpcChannels.clipboardHasImage),
  clipboardReadFilePaths: () => ipcRenderer.invoke(IpcChannels.clipboardReadFilePaths),
  onClipboardChanged: (cb: () => void) => {
    clipboardChangedCb = cb;
  },
  diagReveal: () => ipcRenderer.invoke(IpcChannels.diagReveal),
  musicPoll: () => ipcRenderer.invoke(IpcChannels.musicPoll),
  onMusicState: (cb: (state: MusicState) => void) => {
    musicStateCb = cb;
  },
  musicControl: (action: MusicAction, level?: number) =>
    ipcRenderer.invoke(IpcChannels.musicControl, action, level),
  musicSeek: (positionMs: number) => ipcRenderer.invoke(IpcChannels.musicSeek, positionMs),
  musicArtwork: (hash: string) => ipcRenderer.invoke(IpcChannels.musicArtwork, hash),
  musicLyrics: (id: string) => ipcRenderer.invoke(IpcChannels.musicLyrics, id),
  weatherIpCity: () => ipcRenderer.invoke(IpcChannels.weatherIpCity),
  weatherGeocode: (city: string, lang: string) => ipcRenderer.invoke(IpcChannels.weatherGeocode, city, lang),
  weatherQuery: (lat: number, lon: number, opts?: WeatherQueryOptions) =>
    ipcRenderer.invoke(IpcChannels.weatherQuery, lat, lon, opts),
  storeGet: (key: string) => ipcRenderer.invoke(IpcChannels.storeGet, key),
  storeSet: (key: string, value: unknown) => ipcRenderer.invoke(IpcChannels.storeSet, key, value),
  storeClear: () => ipcRenderer.invoke(IpcChannels.storeClear),
  alarmSoundList: () => ipcRenderer.invoke(IpcChannels.alarmSoundList),
  alarmSoundData: (path: string) => ipcRenderer.invoke(IpcChannels.alarmSoundData, path),
  alarmSoundPick: () => ipcRenderer.invoke(IpcChannels.alarmSoundPick),
  displaysList: () => ipcRenderer.invoke(IpcChannels.displaysList),
  onNotifications: (cb: (items: NotificationItem[]) => void) => {
    notificationsCb = cb;
  },
  notifyActivate: (item: Pick<NotificationItem, 'aumid' | 'launch' | 'atype'>) =>
    ipcRenderer.invoke(IpcChannels.notifyActivate, item),
  notifyImage: (src: string) => ipcRenderer.invoke(IpcChannels.notifyImage, src),
  wechatAcquireKey: () => ipcRenderer.invoke(IpcChannels.wechatAcquireKey),
  wechatHasKey: () => ipcRenderer.invoke(IpcChannels.wechatHasKey),
  getVersion: () => ipcRenderer.invoke(IpcChannels.appGetVersion) as Promise<AppVersionInfo>,
  checkUpdate: () => ipcRenderer.invoke(IpcChannels.updateCheck) as Promise<UpdateCheckResult>,
  installUpdate: () => ipcRenderer.invoke(IpcChannels.updateInstall),
  onUpdateDownloaded: (cb: (info: { version: string }) => void) => {
    updateDownloadedCb = cb;
  },
};

contextBridge.exposeInMainWorld('islandAPI', api);
