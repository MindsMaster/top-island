use serde::{Deserialize, Serialize};

/// 消息托管：来自系统通知中心（wpndatabase.db）的一条通知
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationItem {
    /// wpndatabase.db 中的 Notification.Id（单调递增，作去重水位与激活定位）
    pub id: i64,
    /// 来源应用 AUMID（激活时用）
    pub aumid: String,
    /// 应用显示名（缺省回退 AUMID）
    pub app: String,
    /// 应用图标 URI（可能为空/不可直接加载）
    pub icon: String,
    /// toast 内嵌图片（聊天应用的发送人头像，多为本地文件路径），优先于 icon 展示
    pub image: String,
    pub title: String,
    pub body: String,
    /// toast 深链参数，复现点击时回灌给应用
    pub launch: String,
    /// foreground | background | protocol；protocol 时 launch 为 URI
    pub atype: String,
    /// 到达时间（unix ms）
    pub arrival: i64,
}
