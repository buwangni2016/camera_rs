/*!
 * HTTP 请求处理器（模块化结构）
 *
 * 子模块按资源域划分：
 *   auth       — 登录/登出
 *   stream     — MJPEG 流 / WebSocket / 多路分屏
 *   camera_ctrl — 摄像头控制（切换、图像调节、录像、运动参数）
 *   files      — 文件管理（列表、下载、删除）
 *   config     — 所有配置类端点（邮件、通知、云存储、水印、…）
 *   rest_api   — REST API（X-API-Key 认证）
 *   system     — 系统工具（health、sysinfo、QR、PWA manifest）
 *   heatmap    — 热力图与每日报告
 */

pub mod auth;
pub mod camera_ctrl;
pub mod config;
pub mod files;
pub mod heatmap;
pub mod rest_api;
pub mod stream;
pub mod system;

// 将所有子模块的公开符号提升到 handlers:: 顶层，保持 `use handlers::*` 兼容性
pub use auth::*;
pub use camera_ctrl::*;
pub use config::*;
pub use files::*;
pub use heatmap::*;
pub use rest_api::*;
pub use stream::*;
pub use system::*;

// ============================================================
//  共享类型与辅助函数（所有子模块可见）
// ============================================================

use axum_extra::extract::cookie::PrivateCookieJar;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Default)]
pub struct OkResp {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub struct ValQuery {
    pub val: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct EventQuery {
    pub limit: Option<usize>,
    pub kind: Option<String>,
}

pub(crate) fn is_authed(jar: &PrivateCookieJar) -> bool {
    jar.get("session")
        .map(|c| c.value() == "ok")
        .unwrap_or(false)
}

pub(crate) fn now_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn ts_str() -> String {
    chrono::Local::now().format("%Y%m%d_%H%M%S").to_string()
}

/// 安全响应版 SecurityConfig：绝不回传明文密码
#[derive(Serialize)]
pub struct SecurityConfigSafe {
    pub ip_whitelist: Vec<String>,
    pub https_enabled: bool,
    pub cert_path: String,
    pub key_path: String,
    pub has_password: bool, // 只暴露"是否设置了密码"
    pub max_login_attempts: u32,
    pub lockout_secs: u64,
}

impl From<&crate::state::SecurityConfig> for SecurityConfigSafe {
    fn from(c: &crate::state::SecurityConfig) -> Self {
        Self {
            ip_whitelist: c.ip_whitelist.clone(),
            https_enabled: c.https_enabled,
            cert_path: c.cert_path.clone(),
            key_path: c.key_path.clone(),
            has_password: !c.password.is_empty(),
            max_login_attempts: c.max_login_attempts,
            lockout_secs: c.lockout_secs,
        }
    }
}
