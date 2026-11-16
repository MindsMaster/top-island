use island_core::MusicConfig;

use super::provider::MusicProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IntegrationStatus {
    NotDetected,
    NeedsRestart,
    Installed,
    Connecting,
    Connected,
    NoPermission,
    Conflict,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GrantResult {
    Granted,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationInfo {
    pub id: &'static str,
    pub status: IntegrationStatus,
}

/// 播放器专属增强 自带权威源与部署
pub trait Integration: Send + Sync {
    /// 与前端集成表 id 一致
    fn id(&self) -> &'static str;

    fn enabled(&self, music: &MusicConfig) -> bool;

    /// 首次调用才启动
    fn provider(&'static self, on_change: fn()) -> &'static dyn MusicProvider;

    fn set_wanted(&self, want: bool);

    fn status(&self) -> IntegrationStatus;

    /// 可能弹 UAC
    fn grant(&self) -> GrantResult;
}
