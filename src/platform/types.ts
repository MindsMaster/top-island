//! 与 Rust 侧 serde 形状一一对应的数据契约。手写镜像，vue-tsc 在构建时挡漂移。

/** 锚点式播放状态：positionMs 是 anchorEpochMs 时刻的位置，渲染层按 rate 本地外推 */
export interface MusicState {
  /** 'ncm-bridge' | 'smtc' | '' */
  provider: string;
  isPlaying: boolean;
  track?: string;
  artist?: string;
  album?: string;
  sourceAppId?: string;
  songId?: string;
  positionMs: number;
  anchorEpochMs: number;
  /** 播放 1，暂停或无时间轴 0 */
  rate: number;
  durationMs?: number;
  seekSupported: boolean;
  /** 封面直链，可直接作 <img> src */
  artworkUrl?: string;
  /** 封面位图 hash，据此调 musicArtwork 拉取一次 */
  artworkHash?: string;
  /** 歌词就绪标识，据此调 musicLyrics 拉取一次 */
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

export type BridgeStatus = 'notDetected' | 'needsRestart' | 'installed' | 'connecting' | 'connected';

export interface MusicConfig {
  /** 网易云进程内增强，默认开启 */
  neteaseBridge: boolean;
}

/** 可单独停用的后台子系统（故障排查用：逐个关闭定位鼠标卡顿等问题的来源） */
export interface DiagnosticsToggles {
  /** 剪贴板监听（winbridge 事件驱动，变化即读） */
  clipboardPoll: boolean;
  /** 音乐状态轮询（2s，SMTC helper） */
  musicPoll: boolean;
  /** 开发者模式：岛窗显示 FPS 浮层 */
  devOverlay: boolean;
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
  music: MusicConfig;
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
