/*!
 * 共享状态定义
 *
 * 设计：AppState 本身可 Clone（cheap），内部通过 Arc<Mutex<...>> 共享可变数据。
 * 这是 axum 的推荐模式，也允许从 AppState 直接提取 Key（FromRef<AppState>）。
 */

use std::sync::Arc;
use parking_lot::Mutex;
use tokio::sync::broadcast;
use axum_extra::extract::cookie::Key;
use axum::extract::FromRef;

// ============================================================
//  摄像头状态
// ============================================================
pub struct CameraState {
    pub latest_jpeg: Option<Arc<Vec<u8>>>,
    pub recording: bool,
    pub record_frames: Vec<Vec<u8>>,
    pub record_start: Option<u64>,
    pub motion_detect: bool,
    pub motion_gate: bool,
    pub auto_capture: bool,
    pub auto_interval: u64,
    pub resolution: (u32, u32),
    pub sensitivity: u8,
    pub min_area: u32,
    pub frame_skip: u32,
    pub motion_count: u64,
    pub motion_now: bool,
    pub unknown_count: u64,
    pub prev_gray: Option<Vec<u8>>,
    pub last_motion_save: u64,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            latest_jpeg: None,
            recording: false,
            record_frames: Vec::new(),
            record_start: None,
            motion_detect: false,
            motion_gate: true,
            auto_capture: false,
            auto_interval: 10,
            resolution: (0, 0),
            sensitivity: crate::MOTION_SENS,
            min_area: crate::MOTION_MIN_AREA,
            frame_skip: crate::FRAME_SKIP,
            motion_count: 0,
            motion_now: false,
            unknown_count: 0,
            prev_gray: None,
            last_motion_save: 0,
        }
    }
}

// ============================================================
//  邮件配置
// ============================================================
#[derive(Clone, Debug, Default)]
pub struct EmailConfig {
    pub enabled: bool,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub from: String,
    pub password: String,
    pub to: String,
    pub cooldown: u64,
    pub on_motion: bool,
    pub on_unknown: bool,
    pub last_sent: u64,
}

impl EmailConfig {
    pub fn default_smtp() -> Self {
        Self {
            smtp_host: "smtp.gmail.com".into(),
            smtp_port: 465,
            cooldown: 60,
            on_motion: true,
            on_unknown: true,
            ..Default::default()
        }
    }
}

// ============================================================
//  全局应用状态（Clone-cheap：内部字段皆为 Arc）
// ============================================================
#[derive(Clone)]
pub struct AppState {
    pub camera: Arc<Mutex<CameraState>>,
    pub email_cfg: Arc<Mutex<EmailConfig>>,
    /// 广播 JPEG 帧给所有 MJPEG 流客户端
    pub frame_tx: broadcast::Sender<Arc<Vec<u8>>>,
    /// Cookie 签名密钥（axum-extra PrivateCookieJar 需要）
    pub cookie_key: Key,
}

impl AppState {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(4);
        Self {
            camera: Arc::new(Mutex::new(CameraState::default())),
            email_cfg: Arc::new(Mutex::new(EmailConfig::default_smtp())),
            frame_tx: tx,
            cookie_key: Key::generate(),
        }
    }
}

/// 让 axum-extra 的 PrivateCookieJar 能从 AppState 自动提取 Key
/// FromRef<AppState> for Key（AppState 是本地类型，无孤儿规则冲突）
impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}
