/*!
 * USB 摄像头 Web 监控系统 — Rust 重构版
 *
 * 功能：实时 MJPEG 流、拍照、录像、帧差运动侦测、邮件告警
 * 访问：http://localhost:5000
 */

mod camera;
mod motion;
mod email;
mod handlers;
mod html;
mod state;

use std::net::SocketAddr;
use axum::{
    Router,
    routing::{get, post},
};
use tower_http::cors::CorsLayer;
use state::AppState;
use handlers::*;

// ============================================================
//  配置
// ============================================================
pub const CAMERA_INDEX: usize = 0;
pub const RESOLUTION: (u32, u32) = (1920, 1080);
pub const RECORD_FPS: f64 = 20.0;
pub const SAVE_DIR: &str = "captures";
pub const FACE_DIR: &str = "faces";
pub const HOST: &str = "0.0.0.0";
pub const PORT: u16 = 5000;
pub const PASSWORD: &str = "admin"; // 留空则不需要密码
pub const MOTION_SENS: u8 = 30;
pub const MOTION_MIN_AREA: u32 = 1500;
pub const FRAME_SKIP: u32 = 10;

// ============================================================
//  入口
// ============================================================
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .init();

    for sub in &["photos", "videos", "motion", "auto", "alerts"] {
        std::fs::create_dir_all(format!("{}/{}", SAVE_DIR, sub))
            .expect("无法创建存储目录");
    }
    std::fs::create_dir_all(FACE_DIR).ok();

    // AppState::Clone-cheap，内部用 Arc<Mutex<...>> 共享数据
    let state = AppState::new();

    // 启动摄像头后台线程（传入 clone 的 state）
    {
        let s = state.clone();
        tokio::spawn(async move {
            camera::capture_loop(s).await;
        });
    }

    // 路由：state 类型为 AppState（axum 会 clone 给每个 handler）
    let app = Router::new()
        .route("/login",  get(login_page).post(login_post))
        .route("/logout", get(logout))
        .route("/",       get(index))
        .route("/video",  get(video_stream))
        .route("/photo",            get(take_photo))
        .route("/record",           get(toggle_record))
        .route("/toggle",           get(toggle_feature))
        .route("/set_interval",     get(set_interval))
        .route("/set_sensitivity",  get(set_sensitivity))
        .route("/set_min_area",     get(set_min_area))
        .route("/set_frame_skip",   get(set_frame_skip))
        .route("/stats",            get(get_stats))
        .route("/files",            get(list_files))
        .route("/file/:ftype/:filename", get(serve_file))
        .route("/delete",           post(delete_file))
        .route("/save_config",      post(save_config))
        .route("/test_email",       get(test_email_route))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", HOST, PORT).parse().unwrap();
    tracing::info!("启动中... 访问 http://localhost:{}", PORT);
    if !PASSWORD.is_empty() {
        tracing::info!("登录密码: {}", PASSWORD);
    }

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
