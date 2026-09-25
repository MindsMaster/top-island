use serde::{Deserialize, Serialize};

// camelCase 兼容旧版 store.json 与前端类型

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

/// a b 渐变双色 accent 强调色
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
    /// 百分比 45~300
    pub scale: f64,
    /// 露出高度 px 2~20
    pub hidden_peek: f64,
    /// 'primary' 或 GDI 设备名
    pub display_id: String,
}

impl Default for IslandLayout {
    fn default() -> Self {
        Self {
            scale: 100.0,
            hidden_peek: 6.0,
            display_id: "primary".into(),
        }
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

/// 故障排查用 逐项停用定位问题来源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DiagnosticsToggles {
    pub clipboard_poll: bool,
    pub music_poll: bool,
    /// 仅前端 FPS 浮层
    pub dev_overlay: bool,
}

impl Default for DiagnosticsToggles {
    fn default() -> Self {
        Self {
            clipboard_poll: true,
            music_poll: true,
            dev_overlay: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationsPrivacy {
    pub enabled: bool,
    pub blur_avatar: bool,
    pub blur_name: bool,
    pub replace_body: bool,
    /// 空则用内置默认文案
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
    pub enabled: bool,
    pub popup: bool,
    pub privacy: NotificationsPrivacy,
    /// 改来源应用 ShowBanner 注册表
    pub suppress_banner: bool,
    /// 需先在设置里获取密钥
    pub wechat: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MusicConfig {
    /// 网易云进程内增强
    pub netease_bridge: bool,
}

impl Default for MusicConfig {
    fn default() -> Self {
        Self {
            netease_bridge: true,
        }
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
    /// 仅打包版实际生效
    pub auto_launch: bool,
    #[serde(default = "default_weather_sky")]
    pub weather_sky: bool,
}

fn default_weather_sky() -> bool {
    true
}

impl AppSettings {
    /// 旧版 store.json 可能缺字段
    pub fn from_value(value: serde_json::Value) -> Self {
        serde_json::from_value(value).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_sky_defaults_on_for_old_store() {
        let s = AppSettings::from_value(serde_json::json!({ "theme": "pink" }));
        assert!(s.weather_sky);
        assert_eq!(s.theme, ThemeId::Pink);
    }

    #[test]
    fn weather_sky_round_trips_camel_case() {
        let s = AppSettings::from_value(serde_json::json!({ "weatherSky": false }));
        assert!(!s.weather_sky);
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["weatherSky"], false);
    }
}
