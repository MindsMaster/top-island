// 与 Rust 侧 serde 形状一一对应 手写镜像

/** positionMs 是 anchorEpochMs 时刻的位置 渲染层按 rate 本地外推 */
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
  /** 播放 1 暂停或无时间轴 0 */
  rate: number;
  durationMs?: number;
  seekSupported: boolean;
  artworkUrl?: string;
  /** 据此调 musicArtwork 拉取一次 */
  artworkHash?: string;
  /** 据此调 musicLyrics 拉取一次 */
  lyricsId?: string;
}

export interface MusicArtwork {
  hash: string;
  dataUrl: string;
}

export interface LyricLine {
  timeMs: number;
  text: string;
}

export interface LyricsData {
  lines: LyricLine[];
  /** SMTC 无时间轴时用于估算进度 */
  durationMs: number;
}

export type MusicAction = 'play' | 'pause' | 'next' | 'prev' | 'volume';

export type BridgeStatus = 'notDetected' | 'needsRestart' | 'installed' | 'connecting' | 'connected';

export interface MusicConfig {
  neteaseBridge: boolean;
}

/** 故障排查用 逐个停用定位鼠标卡顿等问题来源 */
export interface DiagnosticsToggles {
  /** 实为 winbridge 事件驱动 非轮询 */
  clipboardPoll: boolean;
  musicPoll: boolean;
  devOverlay: boolean;
}

/** 新增主题需同步 styles/_tokens.scss 令牌块与 core/theme 的 THEMES */
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

/** a b 渐变双色 accent 强调色 亮暗系按背景亮度自动判定 */
export interface CustomTheme {
  a: string;
  b: string;
  accent: string;
}

export interface IslandLayout {
  /** 百分比 45~300 */
  scale: number;
  /** 露出高度 px 2~20 */
  hiddenPeek: number;
  /** 'primary' 或 GDI 设备名 */
  displayId: string;
}

export interface DisplayInfo {
  id: string;
  label: string;
  primary: boolean;
}

export type LangPref = 'auto' | 'zh-CN' | 'en-US';

/** 来自系统通知中心 wpndatabase.db */
export interface NotificationItem {
  /** 单调递增 作去重水位与激活定位 */
  id: number;
  aumid: string;
  /** HandlerAssets.DisplayName 缺省回退 AUMID */
  app: string;
  /** HandlerAssets.IconUri 可能为空或不可直接加载 */
  icon: string;
  /** toast 内嵌图 多为本地路径 优先于 icon 展示 */
  image: string;
  title: string;
  body: string;
  /** toast 深链参数 复现点击时回灌给应用 */
  launch: string;
  /** foreground | background | protocol protocol 时 launch 为 URI */
  atype: string;
  /** unix ms */
  arrival: number;
}

export interface NotificationsPrivacy {
  enabled: boolean;
  blurAvatar: boolean;
  blurName: boolean;
  replaceBody: boolean;
  /** 空则用内置默认文案 */
  bodyText: string;
}

export interface NotificationsConfig {
  enabled: boolean;
  popup: boolean;
  privacy: NotificationsPrivacy;
  /** 改系统设置 关掉来源应用的 ShowBanner 关闭本项时恢复 */
  suppressBanner: boolean;
  /** 需先在设置里获取密钥 */
  wechat: boolean;
}

/** 全部窗口共享 持久化且跨窗口实时同步 */
export interface AppSettings {
  theme: ThemeId;
  customTheme: CustomTheme;
  island: IslandLayout;
  lang: LangPref;
  notifications: NotificationsConfig;
  diagnostics: DiagnosticsToggles;
  music: MusicConfig;
  /** 仅打包版实际生效 */
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
  /** is_day 为 0 或 1 */
  current?: { temperature_2m: number; weather_code: number; is_day: number };
  daily?: {
    time: string[];
    temperature_2m_max: number[];
    temperature_2m_min: number[];
    weathercode: number[];
  };
  error?: string;
}

export interface MsnCondition {
  temp: number;
  cap: string;
  symbol: string;
  /** 降水概率 % */
  precip?: number;
}

export interface MsnCurrent extends MsnCondition {
  feels: number;
  rh: number;
  uv: number;
  uvDesc: string;
  /** km */
  vis: number;
  pvdrWindDir: string;
  pvdrWindSpd: string;
  aqi?: number;
  aqiSeverity?: string;
}

export interface MsnDay {
  hourly: Array<MsnCondition & { valid: string }>;
  daily: {
    valid: string;
    symbol: string;
    tempHi: number;
    tempLo: number;
    precip: number;
    /** mm */
    rainAmount: number;
    day: { cap: string };
    night: { cap: string };
  };
  /** 带时区偏移的当地时刻 */
  almanac: { sunrise: string; sunset: string };
}

export interface MsnNowcast {
  summary: string;
  shortSummary: string;
  /** 相对强度 单位未公开 */
  precipitation: number[];
  minutesBetweenHorrizons: number;
}

/** MSN weatherfalcon overview 仅列用到的字段 */
export interface MsnOverview {
  responses?: Array<{
    weather?: Array<{
      current: MsnCurrent;
      forecast?: { days: MsnDay[] };
      nowcasting?: MsnNowcast;
    }>;
  }>;
}

export interface WeatherQueryOptions {
  daily?: string;
  forecastDays?: number;
}

export interface AlarmSound {
  /** 绝对路径 亦作唯一标识 */
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

/** CSS 像素 相对窗口内容区 null 为全程可交互 */
export interface HotRect {
  x: number;
  y: number;
  width: number;
  height: number;
}
