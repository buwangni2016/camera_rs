mod auth;
mod camera;
mod config;
mod email;
mod events;
mod handlers;
mod html;
mod motion;
mod notify;
mod schedule;
mod state;
mod storage;
mod upload;

use std::net::SocketAddr;
use axum::{Router, routing::{get, post}};
use tower_http::cors::CorsLayer;
use state::AppState;
use handlers::*;

pub const SAVE_DIR:          &str = "captures";
pub const FACE_DIR:          &str = "faces";
pub const RECORD_FPS:         f64 = 20.0;
pub const MOTION_SENS:         u8 = 30;
pub const MOTION_MIN_AREA:    u32 = 1500;
pub const FRAME_SKIP:         u32 = 10;
pub const MAX_LOGIN_ATTEMPTS: u32 = 5;
pub const LOCKOUT_SECS:       u64 = 900;
pub const MAX_STORAGE_MB:     u64 = 2048;
pub const PASSWORD:          &str = "admin";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).init();

    let cfg = config::Config::load();

    for sub in &["photos", "videos", "motion", "auto", "alerts", "timelapse"] {
        std::fs::create_dir_all(format!("{}/{}", SAVE_DIR, sub)).expect("无法创建存储目录");
    }
    std::fs::create_dir_all(FACE_DIR).ok();

    let state = AppState::new(cfg.camera.index);

    // 枚举摄像头
    {
        let mut cameras = camera::list_cameras();
        if cameras.is_empty() {
            cameras = vec![(0, "摄像头 0".into()), (1, "摄像头 1".into())];
        } else {
            for (i, n) in &cameras { tracing::info!("  [{}] {}", i, n); }
        }
        *state.available_cameras.lock() = cameras;
    }

    // 启动主摄像头捕获（热切换模式）
    {
        let s = state.clone();
        tokio::spawn(async move { camera::capture_loop(s).await });
    }

    // 启动定时任务
    {
        let s = state.clone();
        tokio::spawn(async move { schedule::schedule_loop(s).await });
    }

    // 事件日志：记录启动
    state.event_log.log(events::Event::new(
        0, events::EventKind::SystemStart, 0, "摄像头监控系统启动"
    ));

    let app = Router::new()
        // 认证
        .route("/login",          get(login_page).post(login_post))
        .route("/logout",         get(logout))
        .route("/",               get(index))
        // 视频流（支持 ?cam=N 多摄像头）
        .route("/video",          get(video_stream))
        .route("/ws",             get(ws_handler))
        // 摄像头
        .route("/cameras",        get(cameras_list))
        .route("/switch_camera",  get(switch_camera))
        .route("/multiview",      get(multiview_page))
        // 图像调节
        .route("/set_image",      get(set_image))
        .route("/set_flip",       get(set_flip))
        .route("/set_rotation",   get(set_rotation))
        // 运动区域
        .route("/motion_zones",   get(get_motion_zones).post(save_motion_zones))
        // 控制
        .route("/photo",          get(take_photo))
        .route("/record",         get(toggle_record))
        .route("/toggle",         get(toggle_feature))
        .route("/set_interval",   get(set_interval))
        .route("/set_sensitivity",get(set_sensitivity))
        .route("/set_min_area",   get(set_min_area))
        .route("/set_frame_skip", get(set_frame_skip))
        .route("/stats",          get(get_stats))
        // 文件管理
        .route("/files",          get(list_files))
        .route("/file/:ftype/:filename", get(serve_file))
        .route("/delete",         post(delete_file))
        // 延时摄影
        .route("/timelapse/build",get(build_timelapse))
        .route("/timelapse/config",get(get_timelapse_cfg).post(save_timelapse_cfg))
        // 事件日志
        .route("/events",         get(get_events))
        .route("/events/clear",   post(clear_events))
        // 邮件
        .route("/save_config",    post(save_config))
        .route("/test_email",     get(test_email_route))
        // 通知渠道
        .route("/notify_config",  get(get_notify_config).post(save_notify_config))
        .route("/test_notify",    get(test_notify))
        .route("/alert_rule",     get(get_alert_rule).post(save_alert_rule))
        // 云存储
        .route("/onedrive_config",    get(get_onedrive_config).post(save_onedrive_config))
        .route("/onedrive_share",     get(create_onedrive_share))
        .route("/gdrive_config",      get(get_gdrive_config).post(save_gdrive_config))
        .route("/ftp_config",         get(get_ftp_config).post(save_ftp_config))
        .route("/upload_now",         get(upload_now))
        // 定时任务
        .route("/schedule",       get(get_schedule).post(save_schedule))
        // 用户管理
        .route("/users",          get(list_users).post(save_users))
        // API Key
        .route("/api_config",     get(get_api_config).post(save_api_config))
        // REST API（需 X-API-Key）
        .route("/api/snapshot",   get(api_snapshot))
        .route("/api/stats",      get(api_stats))
        .route("/api/events",     get(api_events))
        .route("/api/trigger",    post(api_trigger))
        // 系统
        .route("/health",         get(health_check))
        .route("/sysinfo",        get(sys_info))
        .route("/qrcode",         get(qr_code))
        .route("/manifest.json",  get(pwa_manifest))
        .route("/config/export",  get(export_config))
        .route("/config/import",  post(import_config))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.server.host, cfg.server.port).parse().unwrap();
    tracing::info!("启动中... http://localhost:{}", cfg.server.port);
    tracing::info!("密码: {} | 修改: config.toml", cfg.security.password);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await.unwrap();
}
