mod auth;
mod camera;
mod config;
mod email;
mod events;
mod handlers;
mod heatmap;
mod html;
mod motion;
mod notify;
mod report;
mod schedule;
mod state;
mod storage;
mod upload;

use std::net::SocketAddr;
use axum::{Router, routing::{get, post}, middleware, extract::ConnectInfo};
use tower_http::cors::CorsLayer;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use state::AppState;
use handlers::*;

pub const SAVE_DIR:          &str = "captures";
pub const FACE_DIR:          &str = "faces";
pub const RECORD_FPS:         f64 = 20.0;
pub const MOTION_SENS:         u8 = 30;
pub const MOTION_MIN_AREA:    u32 = 8000;
pub const FRAME_SKIP:         u32 = 10;
pub const MAX_LOGIN_ATTEMPTS: u32 = 5;
pub const LOCKOUT_SECS:       u64 = 900;
pub const MAX_STORAGE_MB:     u64 = 2048;
pub const PASSWORD:          &str = "admin";

/// IP 白名单中间件：若 security.ip_whitelist 非空，只允许列表中的 IP 访问
async fn ip_whitelist_middleware(
    axum::extract::State(state): axum::extract::State<AppState>,
    ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let whitelist = state.security.lock().ip_whitelist.clone();
    if !whitelist.is_empty() {
        let ip = addr.ip().to_string();
        if !whitelist.iter().any(|w| ip_matches(&ip, w)) {
            return axum::response::Response::builder()
                .status(axum::http::StatusCode::FORBIDDEN)
                .body(axum::body::Body::from("IP not allowed"))
                .unwrap();
        }
    }
    next.run(request).await
}

fn ip_matches(ip: &str, pattern: &str) -> bool {
    // 只有以 '*' 结尾的模式才使用前缀匹配（如 "192.168.1.*"）
    // 其余情况必须精确匹配，防止 192.168.1.1 意外匹配 192.168.1.10
    if pattern.ends_with('*') {
        ip.starts_with(pattern.trim_end_matches('*'))
    } else {
        ip == pattern
    }
}

/// 带监督的后台任务：panic 时记录日志并通过 CancellationToken 通知系统。
macro_rules! spawn_supervised {
    ($tracker:expr, $token:expr, $name:expr, $fut:expr) => {{
        let token = $token.clone();
        let name = $name;
        $tracker.spawn(async move {
            let result = tokio::spawn($fut).await;
            match result {
                Ok(_) => tracing::info!("后台任务 '{}' 正常退出", name),
                Err(e) if e.is_panic() => {
                    tracing::error!("后台任务 '{}' 发生 panic: {:?}", name, e);
                    token.cancel();   // 通知系统进入关闭流程
                }
                Err(e) => tracing::warn!("后台任务 '{}' 被取消: {:?}", name, e),
            }
        });
    }};
}

#[tokio::main]
async fn main() {
    // 结构化日志：支持 RUST_LOG 环境变量过滤
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(true)
        .with_thread_ids(false)
        .init();

    let cfg = config::Config::load();

    for sub in &["photos", "videos", "motion", "auto", "alerts", "timelapse"] {
        std::fs::create_dir_all(format!("{}/{}", SAVE_DIR, sub)).expect("无法创建存储目录");
    }
    std::fs::create_dir_all(FACE_DIR).ok();

    let state = AppState::new(cfg.camera.index, &cfg.security);

    // 从磁盘恢复上次的运行时配置（通知渠道、云存储、排程等）
    config::load_runtime_state(&state);

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

    // 任务生命周期管理
    let token = CancellationToken::new();
    let tracker = TaskTracker::new();

    // 启动主摄像头捕获
    {
        let s = state.clone();
        spawn_supervised!(tracker, token, "capture_loop", async move {
            camera::capture_loop(s).await
        });
    }

    // 为所有非主摄像头各启动一个独立预览流（多屏分屏用）
    {
        let primary = cfg.camera.index;
        let cameras = state.available_cameras.lock().clone();
        for (idx, _) in cameras {
            let idx = idx as usize;
            if idx != primary {
                let s = state.clone();
                tokio::spawn(async move { camera::capture_loop_for(s, idx).await });
            }
        }
    }

    // 启动定时任务
    {
        let s = state.clone();
        spawn_supervised!(tracker, token, "schedule_loop", async move {
            schedule::schedule_loop(s).await
        });
    }

    // 运行时状态自动持久化（每 60 秒）
    {
        let s = state.clone();
        spawn_supervised!(tracker, token, "auto_persist", async move {
            config::auto_persist_loop(s).await
        });
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
        // 水印
        .route("/watermark",      get(get_watermark).post(save_watermark))
        // 隐私遮罩
        .route("/privacy_masks",  get(get_privacy_masks).post(save_privacy_masks))
        // 运动热力图
        .route("/heatmap",        get(get_heatmap_json))
        .route("/heatmap/image",  get(get_heatmap_image))
        .route("/heatmap/clear",  post(clear_heatmap_handler))
        // 每日报告
        .route("/report/send",    post(trigger_daily_report))
        // RTSP 摄像头
        .route("/rtsp_cameras",   get(get_rtsp_cameras).post(save_rtsp_cameras))
        // 安全配置
        .route("/security_config",get(get_security_config).post(save_security_config))
        // 录像限制
        .route("/record_limits",  get(get_record_limits).post(save_record_limits))
        // 系统
        .route("/health",         get(health_check))
        .route("/sysinfo",        get(sys_info))
        .route("/qrcode",         get(qr_code))
        .route("/manifest.json",  get(pwa_manifest))
        .route("/config/export",  get(export_config))
        .route("/config/import",  post(import_config))
        .layer(middleware::from_fn_with_state(state.clone(), ip_whitelist_middleware))
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let addr: SocketAddr = format!("{}:{}", cfg.server.host, cfg.server.port).parse().unwrap();
    tracing::info!("启动中... http://localhost:{}", cfg.server.port);
    tracing::info!("默认密码: {} | 修改: config.toml", cfg.security.password);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    // 优雅关闭：监听 Ctrl-C 或任务 panic（CancellationToken 被取消）
    let shutdown_state = state.clone();
    let shutdown = async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("收到 Ctrl-C，开始优雅关闭...");
            }
            _ = token.cancelled() => {
                tracing::error!("后台任务 panic，触发系统关闭");
            }
        }
        // 关闭前最后一次持久化运行时状态
        config::save_runtime_state(&shutdown_state);
        tracing::info!("运行时状态已在关闭前持久化");
        tracker.close();
        tracker.wait().await;
    };

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown)
        .await
        .unwrap();

    tracing::info!("服务器已关闭");
}
