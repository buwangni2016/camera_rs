/*!
 * 全局状态定义（v3.0）
 * 包含：摄像头状态、所有配置、多用户、事件日志、定时规则等
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::collections::HashMap;
use parking_lot::Mutex;
use tokio::sync::broadcast;
use axum_extra::extract::cookie::Key;
use axum::extract::FromRef;
use serde::{Deserialize, Serialize};

use crate::notify::NotifyConfig;
use crate::upload::{OneDriveConfig, GoogleDriveConfig, FtpConfig};
use crate::events::EventLogger;
use crate::schedule::ScheduleRule;

// ============================================================
//  图像调节参数
// ============================================================
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImageSettings {
    pub brightness: i32,
    pub contrast:   i32,
    pub saturation: i32,
    pub flip_h:     bool,
    pub flip_v:     bool,
    pub rotation:   u32,
}

// ============================================================
//  运动检测区域（百分比坐标，0.0-1.0）
// ============================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionZone {
    pub x1: f32, pub y1: f32,
    pub x2: f32, pub y2: f32,
    pub label: String,
}

// ============================================================
//  录像限制配置
// ============================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordLimits {
    pub max_duration_secs: u64,   // 0=不限
    pub max_size_mb:       u64,   // 0=不限
    pub auto_split:        bool,  // 到达限制后自动分段
}
impl Default for RecordLimits {
    fn default() -> Self { Self { max_duration_secs: 0, max_size_mb: 500, auto_split: true } }
}

// ============================================================
//  多用户配置
// ============================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAccount {
    pub username:      String,
    pub password_hash: String,   // SHA-256
    pub role:          UserRole,
    pub enabled:       bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,   // 全部权限
    Viewer,  // 仅查看
}

// ============================================================
//  REST API 配置
// ============================================================
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApiConfig {
    pub enabled: bool,
    pub api_key: String,   // X-API-Key 认证
}

// ============================================================
//  安全配置
// ============================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub ip_whitelist:        Vec<String>,  // 空=不限制
    pub https_enabled:       bool,
    pub cert_path:           String,
    pub key_path:            String,
    /// 登录密码（来自 config.toml，运行时可通过 /security_config 修改）
    pub password:            String,
    pub max_login_attempts:  u32,
    pub lockout_secs:        u64,
}
impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            ip_whitelist: vec![], https_enabled: false,
            cert_path: "cert.pem".into(), key_path: "key.pem".into(),
            password: "admin".into(), max_login_attempts: 5, lockout_secs: 900,
        }
    }
}

// ============================================================
//  告警时间规则（勿扰模式）
// ============================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertTimeRule {
    pub enabled:      bool,
    pub mode:         AlertMode,
    pub start_hhmm:   u16,   // 允许/禁止告警的开始时间
    pub end_hhmm:     u16,
    pub weekdays:     u8,    // 同 ScheduleRule
}
impl Default for AlertTimeRule {
    fn default() -> Self {
        Self { enabled: false, mode: AlertMode::AllowOnly, start_hhmm: 0, end_hhmm: 2359, weekdays: 0xFF }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlertMode {
    AllowOnly,  // 只在指定时间段内告警
    Suppress,   // 在指定时间段内禁止告警
}

// ============================================================
//  延时摄影配置
// ============================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelapseConfig {
    pub enabled:         bool,
    pub fps:             u32,   // 合成视频帧率
    pub max_frames:      u32,   // 最多使用几张
    pub output_format:   String, // "mjpeg" (currently only supported)
}
impl Default for TimelapseConfig {
    fn default() -> Self {
        Self { enabled: false, fps: 10, max_frames: 300, output_format: "mjpeg".into() }
    }
}

// ============================================================
//  摄像头状态
// ============================================================
pub struct CameraState {
    pub latest_jpeg:       Option<Arc<Vec<u8>>>,
    pub recording:         bool,
    pub record_frames:     Vec<Vec<u8>>,
    pub record_start:      Option<u64>,
    pub motion_detect:     bool,
    pub motion_gate:       bool,
    pub auto_capture:      bool,
    pub auto_interval:     u64,
    pub resolution:        (u32, u32),
    pub sensitivity:       u8,
    pub min_area:          u32,
    pub frame_skip:        u32,
    pub motion_count:      u64,
    pub motion_now:        bool,
    pub unknown_count:     u64,
    pub prev_gray:         Option<Vec<u8>>,
    pub last_motion_save:  u64,
    pub fps_current:       f32,
    pub fps_frame_count:   u32,
    pub fps_last_ts:       std::time::Instant,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            latest_jpeg: None, recording: false, record_frames: Vec::new(),
            record_start: None, motion_detect: false, motion_gate: true,
            auto_capture: false, auto_interval: 10, resolution: (0, 0),
            sensitivity: crate::MOTION_SENS, min_area: crate::MOTION_MIN_AREA,
            frame_skip: crate::FRAME_SKIP, motion_count: 0, motion_now: false,
            unknown_count: 0, prev_gray: None, last_motion_save: 0,
            fps_current: 0.0, fps_frame_count: 0,
            fps_last_ts: std::time::Instant::now(),
        }
    }
}

// ============================================================
//  邮件配置
// ============================================================
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EmailConfig {
    pub enabled:   bool,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub from:      String,
    pub password:  String,
    pub to:        String,
    pub cooldown:  u64,
    pub on_motion: bool,
    pub on_unknown: bool,
    pub last_sent: u64,
}
impl EmailConfig {
    pub fn default_smtp() -> Self {
        Self { smtp_host: "smtp.gmail.com".into(), smtp_port: 465,
               cooldown: 60, on_motion: true, on_unknown: true, ..Default::default() }
    }
}

// ============================================================
//  水印配置
// ============================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatermarkConfig {
    pub enabled:    bool,
    pub show_time:  bool,    // 显示时间戳
    pub show_label: bool,    // 显示自定义文字
    pub label:      String,  // 自定义文字内容
    pub position:   String,  // "top_left" | "top_right" | "bottom_left" | "bottom_right"
    pub font_scale: f32,     // 字体缩放比例 (0.5-3.0)
    pub opacity:    u8,      // 透明度 0-255
}
impl Default for WatermarkConfig {
    fn default() -> Self {
        Self {
            enabled: false, show_time: true, show_label: false,
            label: String::new(), position: "bottom_right".into(),
            font_scale: 1.0, opacity: 200,
        }
    }
}

// ============================================================
//  隐私遮罩区域（百分比坐标，0.0-1.0）
// ============================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyMask {
    pub enabled: bool,
    pub x1: f32, pub y1: f32,
    pub x2: f32, pub y2: f32,
    pub label: String,
    pub color: [u8; 3],  // RGB 遮罩颜色，默认黑色
}
impl Default for PrivacyMask {
    fn default() -> Self {
        Self { enabled: true, x1: 0.0, y1: 0.0, x2: 0.0, y2: 0.0,
               label: String::new(), color: [0, 0, 0] }
    }
}

// ============================================================
//  RTSP/IP 摄像头配置
// ============================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtspCamera {
    pub enabled: bool,
    pub url:     String,   // rtsp://user:pass@ip:554/stream
    pub label:   String,
}

// ============================================================
//  全局应用状态（Clone-cheap）
// ============================================================
#[derive(Clone)]
pub struct AppState {
    // 摄像头
    pub camera:             Arc<Mutex<CameraState>>,
    pub camera_idx:         Arc<AtomicUsize>,
    pub available_cameras:  Arc<Mutex<Vec<(u32, String)>>>,
    /// 多摄像头各自的帧广播通道（index -> sender）
    pub frame_txs:          Arc<Mutex<HashMap<usize, broadcast::Sender<Arc<Vec<u8>>>>>>,
    /// 主广播（向后兼容 /video 无参数）
    pub frame_tx:           broadcast::Sender<Arc<Vec<u8>>>,

    // 图像与检测
    pub image_settings:     Arc<Mutex<ImageSettings>>,
    pub motion_zones:       Arc<Mutex<Vec<MotionZone>>>,
    pub record_limits:      Arc<Mutex<RecordLimits>>,

    // 通知
    pub email_cfg:          Arc<Mutex<EmailConfig>>,
    pub notify_cfg:         Arc<Mutex<NotifyConfig>>,
    pub notify_suppressed:  Arc<AtomicBool>,
    pub alert_time_rule:    Arc<Mutex<AlertTimeRule>>,

    // 云存储
    pub onedrive_cfg:       Arc<Mutex<OneDriveConfig>>,
    pub gdrive_cfg:         Arc<Mutex<GoogleDriveConfig>>,
    pub ftp_cfg:            Arc<Mutex<FtpConfig>>,

    // 用户与安全
    pub users:              Arc<Mutex<Vec<UserAccount>>>,
    pub api_cfg:            Arc<Mutex<ApiConfig>>,
    pub security:           Arc<Mutex<SecurityConfig>>,
    pub login_attempts:     Arc<Mutex<HashMap<String, (u32, u64)>>>,

    // 定时任务
    pub schedule_rules:     Arc<Mutex<Vec<ScheduleRule>>>,

    // 延时摄影
    pub timelapse_cfg:      Arc<Mutex<TimelapseConfig>>,

    // RTSP 摄像头
    pub rtsp_cameras:       Arc<Mutex<Vec<RtspCamera>>>,

    // 水印
    pub watermark_cfg:      Arc<Mutex<WatermarkConfig>>,

    // 隐私遮罩
    pub privacy_masks:      Arc<Mutex<Vec<PrivacyMask>>>,

    // 运动热力图（宽x高格子累加计数）
    pub heatmap:            Arc<Mutex<Vec<u32>>>,
    pub heatmap_size:       (u32, u32),   // (cols, rows)

    // 事件日志
    pub event_log:          EventLogger,

    // WebSocket & Cookie
    pub ws_tx:              broadcast::Sender<String>,
    pub cookie_key:         Key,
}

impl AppState {
    pub fn new(camera_idx: usize, sec_cfg: &crate::config::SecurityConfig) -> Self {
        let (frame_tx, _)   = broadcast::channel(8);
        let (ws_tx, _)      = broadcast::channel(32);
        let mut frame_txs   = HashMap::new();
        frame_txs.insert(camera_idx, frame_tx.clone());

        let mut users = Vec::new();
        users.push(UserAccount {
            username: "admin".into(),
            password_hash: crate::auth::hash_password("admin"),
            role: UserRole::Admin,
            enabled: true,
        });

        Self {
            camera:             Arc::new(Mutex::new(CameraState::default())),
            camera_idx:         Arc::new(AtomicUsize::new(camera_idx)),
            available_cameras:  Arc::new(Mutex::new(Vec::new())),
            frame_txs:          Arc::new(Mutex::new(frame_txs)),
            frame_tx,
            image_settings:     Arc::new(Mutex::new(ImageSettings::default())),
            motion_zones:       Arc::new(Mutex::new(Vec::new())),
            record_limits:      Arc::new(Mutex::new(RecordLimits::default())),
            email_cfg:          Arc::new(Mutex::new(EmailConfig::default_smtp())),
            notify_cfg:         Arc::new(Mutex::new(NotifyConfig::default())),
            notify_suppressed:  Arc::new(AtomicBool::new(false)),
            alert_time_rule:    Arc::new(Mutex::new(AlertTimeRule::default())),
            onedrive_cfg:       Arc::new(Mutex::new(OneDriveConfig::default())),
            gdrive_cfg:         Arc::new(Mutex::new(GoogleDriveConfig::default())),
            ftp_cfg:            Arc::new(Mutex::new(FtpConfig::default())),
            users:              Arc::new(Mutex::new(users)),
            api_cfg:            Arc::new(Mutex::new(ApiConfig::default())),
            security:           Arc::new(Mutex::new(SecurityConfig {
                password: sec_cfg.password.clone(),
                max_login_attempts: sec_cfg.max_login_attempts,
                lockout_secs: sec_cfg.lockout_secs,
                ..SecurityConfig::default()
            })),
            login_attempts:     Arc::new(Mutex::new(HashMap::new())),
            schedule_rules:     Arc::new(Mutex::new(Vec::new())),
            timelapse_cfg:      Arc::new(Mutex::new(TimelapseConfig::default())),
            rtsp_cameras:       Arc::new(Mutex::new(Vec::new())),
            watermark_cfg:      Arc::new(Mutex::new(WatermarkConfig::default())),
            privacy_masks:      Arc::new(Mutex::new(Vec::new())),
            heatmap:            Arc::new(Mutex::new(vec![0u32; 64 * 36])),
            heatmap_size:       (64, 36),
            event_log:          EventLogger::new(crate::SAVE_DIR),
            ws_tx,
            cookie_key:         Key::generate(),
        }
    }

    /// 检查当前时间是否允许发送告警
    pub fn alert_allowed(&self) -> bool {
        use std::sync::atomic::Ordering;
        if self.notify_suppressed.load(Ordering::Relaxed) { return false; }
        let rule = self.alert_time_rule.lock();
        if !rule.enabled { return true; }
        use chrono::{Datelike, Timelike};
        let now = chrono::Local::now();
        let wd_bit = crate::schedule::weekday_bit_pub(now.weekday());
        if rule.weekdays != 0xFF && (rule.weekdays & wd_bit) == 0 { return true; }
        let current = now.hour() as u16 * 100 + now.minute() as u16;
        let in_range = if rule.start_hhmm <= rule.end_hhmm {
            current >= rule.start_hhmm && current < rule.end_hhmm
        } else {
            current >= rule.start_hhmm || current < rule.end_hhmm
        };
        match rule.mode {
            crate::state::AlertMode::AllowOnly => in_range,
            crate::state::AlertMode::Suppress  => !in_range,
        }
    }
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self { state.cookie_key.clone() }
}
