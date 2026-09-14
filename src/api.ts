import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  AlarmSound,
  AppSettings,
  AppVersionInfo,
  BridgeStatus,
  DisplayInfo,
  HotRect,
  IpCityInfo,
  IslandApi,
  LyricsData,
  MusicAction,
  MusicArtwork,
  MusicState,
  NotificationItem,
  UpdateCheckResult,
  WeatherQueryOptions,
} from '../shared/ipc';

/** Tauri 版 IslandApi：与 Electron preload 暴露的形状一致，事件名沿用 shared/ipc.ts 的通道字符串 */
export const api: IslandApi = {
  setHotRect: (interactive: HotRect | null, hover: HotRect | null) =>
    invoke('window_set_hot_rect', { interactive, hover }),
  closeWindow: () => invoke('window_close'),
  closeSelf: () => invoke('window_close_self'),
  getCursorPoint: () => invoke('window_get_cursor_point'),
  onIslandHover: (cb) => {
    void listen<boolean>('island-hover', (e) => cb(e.payload));
  },
  onWindowGeometryChanged: (cb) => {
    void listen('tauri://move', () => cb());
    void listen('tauri://resize', () => cb());
    void listen('tauri://scale-change', () => cb());
  },
  openSettings: () => invoke('settings_open'),
  onSettingsOpened: (cb) => {
    void listen('settings:opened', () => cb());
  },
  settingsUpdate: (settings: AppSettings) => invoke('settings_update', { settings }),
  onSettingsChanged: (cb) => {
    void listen<AppSettings>('settings:changed', (e) => cb(e.payload));
  },
  openExternal: (url) => invoke('shell_open_external', { url }),
  getLocale: () => invoke('app_get_locale'),
  clipboardReadText: () => invoke('clipboard_read_text'),
  clipboardWriteText: (text) => invoke('clipboard_write_text', { text }),
  clipboardHasImage: () => invoke('clipboard_has_image'),
  clipboardReadFilePaths: () => invoke('clipboard_read_file_paths'),
  onClipboardChanged: (cb) => {
    void listen('clipboard:changed', () => cb());
  },
  diagReveal: () => invoke('diag_reveal'),
  musicPoll: () => invoke('music_poll'),
  onMusicState: (cb) => {
    void listen<MusicState>('music:state', (e) => cb(e.payload));
  },
  musicControl: (action: MusicAction, level?: number) =>
    invoke('music_control', { action, level: level ?? null }),
  musicSeek: (positionMs) => invoke('music_seek', { positionMs }),
  musicArtwork: (hash) => invoke<MusicArtwork | null>('music_artwork', { hash }),
  musicLyrics: (id) => invoke<LyricsData | null>('music_lyrics', { id }),
  musicBridgeStatus: () => invoke<BridgeStatus>('music_bridge_status'),
  weatherIpCity: () => invoke<IpCityInfo>('weather_ip_city'),
  weatherGeocode: (city, lang) => invoke('weather_geocode', { city, lang }),
  weatherQuery: (lat, lon, opts?: WeatherQueryOptions) =>
    invoke('weather_query', {
      lat,
      lon,
      daily: opts?.daily ?? null,
      forecastDays: opts?.forecastDays ?? null,
    }),
  storeGet: <T>(key: string) => invoke<T | null>('store_get', { key }),
  storeSet: (key, value) => invoke('store_set', { key, value }),
  storeClear: () => invoke('store_clear'),
  alarmSoundList: () => invoke<AlarmSound[]>('alarm_sound_list'),
  alarmSoundData: (path) => invoke<string | null>('alarm_sound_data', { path }),
  alarmSoundPick: () => invoke<AlarmSound | null>('alarm_sound_pick'),
  displaysList: () => invoke<DisplayInfo[]>('displays_list'),
  onNotifications: (cb) => {
    void listen<NotificationItem[]>('notify:incoming', (e) => cb(e.payload));
  },
  notifyActivate: (item) =>
    invoke('notify_activate', { aumid: item.aumid, launch: item.launch, atype: item.atype }),
  notifyImage: (src) => invoke<string | null>('notify_image', { src }),
  wechatAcquireKey: () => invoke('wechat_acquire_key'),
  wechatHasKey: () => invoke('wechat_has_key'),
  getVersion: () => invoke<AppVersionInfo>('app_get_version'),
  getUpdateStatus: () => invoke<UpdateCheckResult>('update_status'),
  checkUpdate: () => invoke<UpdateCheckResult>('update_check'),
  installUpdate: () => invoke('update_install'),
  onUpdateDownloaded: (cb) => {
    void listen<{ version: string }>('update:downloaded', (e) => cb(e.payload));
  },
};
