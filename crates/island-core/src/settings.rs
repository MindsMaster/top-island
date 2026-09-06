use serde::{Deserialize, Serialize};

// 序列化全部 camelCase：要兼容 Electron 版留下的 store.json，前端 TS 类型也不用动。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeId {
    Dark,
    Light,
    Graphite,
    Cream,
    Pink,
    Cyberpink,
    Aurora,
    Sunset,
    Ocean,
    Mint,
    Custom,
}

impl Default for ThemeId {
    fn default() -> Self {
        Self::Dark
    }
}

/// 自定义主题：渐变双色 + 强调色
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTheme {
    pub a: String,
    pub b: String,
    pub accent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IslandLayout {
    /// 缩放百分比（45~300）
    pub scale: f64,
    /// 上滑隐藏时露出的高度（px，2~20）
    pub hidden_peek: f64,
    /// 所在显示器：'primary' 或显示器名
    pub display_id: String,
}

impl Default for IslandLayout {
    fn default() -> Self {
        Self { scale: 100.0, hidden_peek: 6.0, display_id: "primary".into() }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LangPref {
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "en-US")]
    EnUs,
}

/// 可单独停用的后台子系统（故障排查用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DiagnosticsToggles {
    pub clipboard_poll: bool,
    pub music_poll: bool,
    /// 开发者模式开关，仅前端渲染 FPS 浮层
    pub dev_overlay: bool,
}

impl Default for DiagnosticsToggles {
    fn default() -> Self {
        Self { clipboard_poll: true, music_poll: true, dev_overlay: false }
    }
}

/// 隐私模式细项：弹窗卡按需遮挡各字段
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationsPrivacy {
    pub enabled: bool,
    pub blur_avatar: bool,
    pub blur_name: bool,
    pub replace_body: bool,
    /// 替换文案（空则用内置默认）
    pub body_text: String,
}

impl Default for NotificationsPrivacy {
    fn default() -> Self {
        Self {
            enabled: false,
            blur_avatar: true,
            blur_name: true,
            replace_body: false,
            body_text: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationsConfig {
    /// 总开关：读通知库属隐私敏感，默认关闭，用户显式开启
    pub enabled: bool,
    pub popup: bool,
    pub privacy: NotificationsPrivacy,
    /// 接管系统横幅（改来源应用的 ShowBanner 注册表）。属系统设置修改，默认关闭
    pub suppress_banner: bool,
    /// 微信消息接入（需先获取密钥），默认关闭
    pub wechat: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MusicConfig {
    /// 网易云进程内增强，默认开启
    pub netease_bridge: bool,
}

impl Default for MusicConfig {
    fn default() -> Self {
        Self { netease_bridge: true }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub theme: ThemeId,
    pub custom_theme: CustomTheme,
    pub island: IslandLayout,
    pub lang: LangPref,
    pub notifications: NotificationsConfig,
    pub diagnostics: DiagnosticsToggles,
    pub music: MusicConfig,
    /// 开机自启动（默认开启；仅安装版实际生效）
    pub auto_launch: bool,
}

impl AppSettings {
    /// store.json 里 settings 可能缺字段（老版本），逐字段 serde default 补齐
    pub fn from_value(value: serde_json::Value) -> Self {
        serde_json::from_value(value).unwrap_or_default()
    }
}
