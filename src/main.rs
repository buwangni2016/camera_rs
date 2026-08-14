mod camera;
mod config;
mod email;
mod handlers;
mod html;
mod motion;
mod state;
mod storage;

use std::net::SocketAddr;
use axum::{Router, routing::{get, post}};
use tower_http::cors::CorsLayer;
use state::AppState;
use handlers::*;

// ============================================================
//  常量（部分从 config.toml 覆盖）
// ============================================================
pub const SAVE_DIR: &str = "captures";
pub const FACE_DIR: &str = "faces";
pub const RECORD_FPS: f64 = 20.0;
pub const MOTION_SENS: u8 = 30;
pub const MOTION_MIN_AREA: u32 = 1500;
pub const FRAME_SKIP: u32 = 10;
pub const MAX_LOGIN_ATTEMPTS: u32 = 5;
pub const LOCKOUT_SECS: u64 = 900;
pub const MAX_STORAGE_MB: u64 = 2048;
pub const PASSWORD: &str = "admin";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).init();

    let cfg = config::Config::load();

    for sub in &["photos", "videos", "motion", "auto", "alerts"] {
        std::fs::create_dir_all(format!("{}/{}", SAVE_DIR, sub)).expect("无法创建存储目录");
    }
    std::fs::create_dir_all(FACE_DIR).ok();

    let state = AppState::new(cfg.camera.index);

    // 枚举可用摄像头
    {
        let mut cameras = camera::list_cameras();
        if cameras.is_empty() {
            cameras = vec![(0, "摄像头 0".into()), (1, "摄像头 1".into())];
            tracing::info!("未自动检测到摄像头，提供默认选项 0/1");
        } else {
            tracing::info!("检测到 {} 个摄像头:", cameras.len());
            for (idx, name) in &cameras { tracing::info!("  [{}] {}", idx, name); }
        }
        *state.available_cameras.lock() = cameras;
    }

    // 启动摄像头捕获后台任务
    {
        let s = state.clone();
        tokio::spawn(async move { camera::capture_loop(s).await });
    }

    let app = Router::new()
        .route("/login",   get(login_page).post(login_post))
        .route("/logout",  get(logout))
        .route("/",        get(index))
        .route("/video",   get(video_stream))
        .route("/ws",      get(ws_handler))
        .route("/cameras", get(cameras_list))
        .route("/switch_camera",   get(switch_camera))
        .route("/set_image",       get(set_image))
        .route("/set_flip",        get(set_flip))
        .route("/set_rotation",    get(set_rotation))
        .route("/photo",           get(take_photo))
        .route("/record",          get(toggle_record))
        .route("/toggle",          get(toggle_feature))
        .route("/set_interval",    get(set_interval))
        .route("/set_sensitivity", get(set_sensitivity))
        .route("/set_min_area",    get(set_min_area))
        .route("/set_frame_skip",  get(set_frame_skip))
        .route("/stats",           get(get_stats))
        .route("/files",           get(list_files))
        .route("/file/:ftype/:filename", get(serve_file))
        .route("/delete",          post(delete_file))
        .route("/save_config",     post(save_config))
        .route("/test_email",      get(test_email_route))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.server.host, cfg.server.port).parse().unwrap();
    tracing::info!("启动中... 访问 http://localhost:{}", cfg.server.port);
    tracing::info!("密码: {} | 修改密码请编辑 config.toml", cfg.security.password);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await.unwrap();
}
