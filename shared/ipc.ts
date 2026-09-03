export interface MusicState {
  isPlaying: boolean;
  track?: string;
  artist?: string;
  sourceAppId?: string;
  /** SMTC 时间轴，部分应用（旧版网易云等）不上报，此时两者为 0 */
  positionMs?: number;
  durationMs?: number;
  /** 会话是否支持外部改变播放位置（决定进度条能否拖动） */
  seekSupported?: boolean;
  /** 当前曲目封面的内容 hash；渲染层据此调 musicArtwork 拉取一次 */
  artworkHash?: string;
  /** SMTC 无时间轴时，来自歌词源的估算时长（ms） */
  estimatedDurationMs?: number;
  /** 当前曲目歌词已就绪的标识（曲目 key）；渲染层据此调 musicLyrics 拉取一次 */
  lyricsId?: string;
}

export interface MusicArtwork {
  hash: string;
  /** data: URL，可直接作为 <img> src */
  dataUrl: string;
}

export interface LyricLine {
  timeMs: number;
  text: string;
}

export interface LyricsData {
  lines: LyricLine[];
  /** 歌词源提供的歌曲时长（ms），SMTC 无时间轴时用于估算进度 */
  durationMs: number;
}

export type MusicAction = 'play' | 'pause' | 'next' | 'prev' | 'volume';

/** 可单独停用的后台子系统（故障排查用：逐个关闭定位鼠标卡顿等问题的来源） */
export interface DiagnosticsToggles {
  /** 剪贴板监听（winbridge 事件驱动，变化即读） */
  clipboardPoll: boolean;
  /** 音乐状态轮询（2s，SMTC helper） */
  musicPoll: boolean;
}

/** 主题 ID。新增主题：styles/_tokens.scss 加令牌块 + useSettings THEMES 注册 */
export type ThemeId =
  | 'dark'
  | 'light'
  | 'graphite'
  | 'cream'
  | 'pink'
  | 'cyberpink'
  | 'aurora'
  | 'sunset'
  | 'ocean'
  | 'mint'
  | 'custom';

/** 自定义主题：渐变双色 + 强调色；亮/暗系按背景亮度自动判定 */
export interface CustomTheme {
  a: string;
  b: string;
  accent: string;
}

/** 岛的布局设置 */
export interface IslandLayout {
  /** 缩放百分比（45~300） */
  scale: number;
  /** 上滑隐藏时露出的高度（px，2~20） */
  hiddenPeek: number;
  /** 所在显示器：'primary' 或 Electron display id 字符串 */
  displayId: string;
}

export interface DisplayInfo {
  id: string;
  label: string;
  primary: boolean;
}

/** 界面语言偏好；auto = 跟随系统 */
export type LangPref = 'auto' | 'zh-CN' | 'en-US';

/** 消息托管：来自系统通知中心（wpndatabase.db）的一条通知 */
export interface NotificationItem {
  /** wpndatabase.db 中的 Notification.Id（单调递增，作去重水位与激活定位） */
  id: number;
  /** 来源应用 AUMID（激活时用） */
  aumid: string;
  /** 应用显示名（HandlerAssets.DisplayName，缺省回退 AUMID） */
  app: string;
  /** 应用图标 URI（HandlerAssets.IconUri，可能为空/不可直接加载） */
  icon: string;
  /** toast 内嵌图片（聊天应用的发送人头像，多为本地文件路径），优先于 icon 展示 */
  image: string;
  title: string;
  body: string;
  /** toast 深链参数（`<toast launch=...>`），复现点击时回灌给应用 */
  launch: string;
  /** foreground | background | protocol；protocol 时 launch 为 URI */
  atype: string;
  /** 到达时间（unix ms） */
  arrival: number;
}

/** 隐私模式细项：弹窗卡按需遮挡各字段 */
export interface NotificationsPrivacy {
  /** 隐私模式总开关 */
  enabled: boolean;
  /** 模糊头像 */
  blurAvatar: boolean;
  /** 模糊发送人/会话名 */
  blurName: boolean;
  /** 正文替换为通用/自定义文案（不渲染真实内容） */
  replaceBody: boolean;
  /** 替换文案（空则用内置默认「有人给你发了条消息」） */
  bodyText: string;
}

export interface NotificationsConfig {
  /** 总开关（读通知库属隐私敏感，默认关闭，用户显式开启） */
  enabled: boolean;
  /** 新通知到达时在岛上弹出提示 */
  popup: boolean;
  /** 隐私模式（细颗粒度）：分别控制模糊头像/名字、替换正文，防旁人瞥见 */
  privacy: NotificationsPrivacy;
  /** 接管系统横幅：把来源应用的右下角弹窗关掉（仅关 ShowBanner，仍进通知中心），
   * 只保留岛上这一个。关闭本项时恢复被改过的应用。属系统设置修改，默认关闭。 */
  suppressBanner: boolean;
  /** 微信消息接入：从解密的本地库读新消息做通知（需先在设置里获取密钥）。默认关闭。 */
  wechat: boolean;
}

/** 全部窗口共享的应用设置（persistent + 跨窗口实时同步） */
export interface AppSettings {
  theme: ThemeId;
  customTheme: CustomTheme;
  island: IslandLayout;
  lang: LangPref;
  notifications: NotificationsConfig;
  diagnostics: DiagnosticsToggles;
  /** 开机自启动（默认开启；仅打包版实际生效） */
  autoLaunch: boolean;
}

export interface IpCityInfo {
  city: string;
  regionName: string;
  country: string;
  lat: number | null;
  lon: number | null;
}

export interface GeocodeResult {
  results?: Array<{ latitude: number; longitude: number; name?: string }>;
  error?: string;
}

export interface WeatherResult {
  current_weather?: { temperature: number; weathercode: number; windspeed: number };
  daily?: {
    time: string[];
    temperature_2m_max: number[];
    temperature_2m_min: number[];
    weathercode: number[];
  };
  error?: string;
}

export interface WeatherQueryOptions {
  daily?: string;
  forecastDays?: number;
}

/** 闹钟提示音条目（默认音来自 %windir%\Media\Alarm*.wav，自定义为任意本地音频） */
export interface AlarmSound {
  /** 音频文件绝对路径（亦作唯一标识） */
  path: string;
  name: string;
}

export const IpcChannels = {
  windowSetIgnoreMouse: 'window:set-ignore-mouse',
  windowClose: 'window:close',
  windowCloseSelf: 'window:close-self',
  windowGetCursorPoint: 'window:get-cursor-point',
  settingsOpen: 'settings:open',
  settingsUpdate: 'settings:update',
  /** 主进程 -> 各窗口的设置变更事件（payload: AppSettings） */
  settingsChanged: 'settings:changed',
  shellOpenExternal: 'shell:open-external',
  appGetLocale: 'app:get-locale',
  clipboardReadText: 'clipboard:read-text',
  clipboardWriteText: 'clipboard:write-text',
  clipboardHasImage: 'clipboard:has-image',
  clipboardReadFilePaths: 'clipboard:read-file-paths',
  /** 主进程 -> 岛：系统剪贴板发生变化（winbridge 事件驱动，无 payload） */
  clipboardChanged: 'clipboard:changed',
  diagReveal: 'diag:reveal',
  musicPoll: 'music:poll',
  /** 主进程 -> 岛：音乐状态变化（SMTC 事件驱动即时推送，payload: MusicState） */
  musicState: 'music:state',
  musicControl: 'music:control',
  musicSeek: 'music:seek',
  musicArtwork: 'music:artwork',
  musicLyrics: 'music:lyrics',
  weatherIpCity: 'weather:ip-city',
  weatherGeocode: 'weather:geocode',
  weatherQuery: 'weather:query',
  storeGet: 'store:get',
  storeSet: 'store:set',
  /** 清空全部本地数据并重启（设置里的「重置」） */
  storeClear: 'store:clear',
  alarmSoundList: 'alarm:sound-list',
  alarmSoundData: 'alarm:sound-data',
  alarmSoundPick: 'alarm:sound-pick',
  displaysList: 'displays:list',
  /** 主进程 -> 岛：新通知到达（payload: NotificationItem[]） */
  notifyIncoming: 'notify:incoming',
  notifyActivate: 'notify:activate',
  notifyImage: 'notify:image',
  wechatAcquireKey: 'wechat:acquire-key',
  wechatHasKey: 'wechat:has-key',
  appGetVersion: 'app:get-version',
  updateStatus: 'update:status',
  updateCheck: 'update:check',
  updateInstall: 'update:install',
  /** 主进程 -> 所有窗口：更新已下载（payload: { version }） */
  updateDownloaded: 'update:downloaded',
} as const;

export interface AppVersionInfo {
  version: string;
  gitHash: string;
  packaged: boolean;
}

export type UpdateCheckStatus = 'dev' | 'checking' | 'available' | 'not-available' | 'downloaded' | 'error';

export interface UpdateCheckResult {
  status: UpdateCheckStatus;
  version?: string;
  message?: string;
}

/** 岛窗交互热区（CSS 像素，相对窗口内容区）；null = 全程可交互（拖动等手势期间） */
export interface HotRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** preload 通过 contextBridge 暴露给渲染层的 API 形状（Tauri 版由 src/api.ts 用 invoke/listen 实现同一形状） */
export interface IslandApi {
  /**
   * 上报岛窗交互热区。WebView2 没有 setIgnoreMouseEvents(forward:) 等价物，
   * 穿透由 Rust 侧 WH_MOUSE_LL 钩子按热区切换；热区外穿透、热区内可交互。
   * 传 null 表示全程可交互（hide 拖动、通知卡片悬停等 keepInteractive 场景）。
   */
  setHotRect(rect: HotRect | null): Promise<void>;
  closeWindow(): Promise<void>;
  /** 仅关闭调用方所在窗口（设置窗等辅助窗口用；closeWindow 是退出整个应用） */
  closeSelf(): Promise<void>;
  /**
   * 全局光标位置（相对调用方窗口内容区的坐标）。
   * 悬停看门狗用：鼠标快速划出屏幕/切到其他显示器时 mouseleave 可能永远
   * 不触发，需要不依赖事件的兜底校验。
   */
  getCursorPoint(): Promise<{ x: number; y: number }>;
  /** 订阅 Rust 钩子判定的光标进出热区（穿透态下唯一可靠的悬停来源） */
  onIslandHover(cb: (inside: boolean) => void): void;
  /** 窗口移动/缩放/DPI 变化（热区物理坐标变了，需要重报） */
  onWindowGeometryChanged(cb: () => void): void;
  /** 打开（或聚焦已打开的）设置窗口 */
  openSettings(): Promise<void>;
  /** 持久化设置并广播给其他窗口 */
  settingsUpdate(settings: AppSettings): Promise<void>;
  /** 订阅其他窗口引起的设置变更 */
  onSettingsChanged(cb: (settings: AppSettings) => void): void;
  openExternal(url: string): Promise<void>;
  getLocale(): Promise<string>;
  clipboardReadText(): Promise<string>;
  clipboardWriteText(text: string): Promise<void>;
  /** 仅探测剪贴板是否含图片（availableFormats，零解码；绝不解码位图——主进程同步解码会拖垮全局鼠标钩子） */
  clipboardHasImage(): Promise<boolean>;
  clipboardReadFilePaths(): Promise<string[]>;
  /** 订阅系统剪贴板变化（winbridge 事件驱动，替代轮询；回调里自行读取内容） */
  onClipboardChanged(cb: () => void): void;
  /** 在资源管理器中定位诊断日志文件 */
  diagReveal(): Promise<void>;
  musicPoll(): Promise<MusicState>;
  /** 订阅主进程推送的音乐状态（SMTC 变化即时到达，轮询之外的低延迟通道） */
  onMusicState(cb: (state: MusicState) => void): void;
  musicControl(action: MusicAction, level?: number): Promise<string>;
  musicSeek(positionMs: number): Promise<boolean>;
  /** 按 hash 取当前曲目封面；hash 不匹配（已切歌）时返回 null */
  musicArtwork(hash: string): Promise<MusicArtwork | null>;
  /** 按 lyricsId 取当前曲目歌词；id 不匹配（已切歌）时返回 null */
  musicLyrics(id: string): Promise<LyricsData | null>;
  weatherIpCity(): Promise<IpCityInfo>;
  weatherGeocode(city: string, lang: string): Promise<GeocodeResult>;
  weatherQuery(lat: number, lon: number, opts?: WeatherQueryOptions): Promise<WeatherResult>;
  storeGet<T>(key: string): Promise<T | null>;
  storeSet(key: string, value: unknown): Promise<void>;
  /** 清空全部本地数据并重启应用（不可撤销） */
  storeClear(): Promise<void>;
  /** 列出系统默认闹钟音（%windir%\Media\Alarm*.wav） */
  alarmSoundList(): Promise<AlarmSound[]>;
  /** 读取音频文件为 data URL（渲染层 file:// 受限，经主进程转运） */
  alarmSoundData(path: string): Promise<string | null>;
  /** 打开文件对话框选择自定义音频；取消返回 null */
  alarmSoundPick(): Promise<AlarmSound | null>;
  /** 枚举显示器（岛落屏设置用） */
  displaysList(): Promise<DisplayInfo[]>;
  /** 订阅新到达的系统通知 */
  onNotifications(cb: (items: NotificationItem[]) => void): void;
  /** 复现点击：激活来源应用（能拿到深链就跳到具体会话）。返回激活方式 */
  notifyActivate(item: Pick<NotificationItem, 'aumid' | 'launch' | 'atype'>): Promise<string>;
  /** 读取通知图片/头像为 data URL（本地路径经主进程转运；不可解析返回 null） */
  notifyImage(src: string): Promise<string | null>;
  /** 获取并缓存微信解密密钥（扫内存，需 Weixin.exe 在运行）。返回结果与账号 */
  wechatAcquireKey(): Promise<{ ok: boolean; wxid?: string; error?: string }>;
  /** 是否已有可用的微信密钥缓存 */
  wechatHasKey(): Promise<boolean>;
  getVersion(): Promise<AppVersionInfo>;
  getUpdateStatus(): Promise<UpdateCheckResult>;
  checkUpdate(): Promise<UpdateCheckResult>;
  installUpdate(): Promise<void>;
  onUpdateDownloaded(cb: (info: { version: string }) => void): void;
}
