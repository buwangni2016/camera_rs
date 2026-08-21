use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;

use super::{is_authed, EventQuery, OkResp};
use crate::state::AppState;

use sysinfo;

// ============================================================
//  事件日志
// ============================================================

pub async fn get_events(
    State(s): State<AppState>,
    Query(q): Query<EventQuery>,
    jar: PrivateCookieJar,
) -> Json<serde_json::Value> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(serde_json::json!([]));
    }
    let limit = q.limit.unwrap_or(100).min(500);
    let events = match q.kind.as_deref() {
        Some(k) => s.event_log.by_kind(k, limit),
        None => s.event_log.recent(limit),
    };
    Json(serde_json::json!({"events": events, "total": s.event_log.count()}))
}

pub async fn clear_events(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp::default());
    }
    s.event_log.clear();
    Json(OkResp {
        ok: true,
        error: None,
    })
}

// ============================================================
//  系统信息、健康检查、QR 码、PWA Manifest
// ============================================================

pub async fn health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
    let cam = state.camera.lock();
    Json(serde_json::json!({
        "status": "ok",
        "camera": cam.resolution != (0, 0),
        "fps": cam.fps_current,
        "uptime": "running",
        "version": "3.0.0",
    }))
}

pub async fn sys_info(State(_): State<AppState>, jar: PrivateCookieJar) -> Json<serde_json::Value> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(serde_json::json!({}));
    }
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();
    let disk_total: u64 = sysinfo::Disks::new_with_refreshed_list()
        .iter()
        .map(|d| d.total_space())
        .sum();
    let disk_used: u64 = sysinfo::Disks::new_with_refreshed_list()
        .iter()
        .map(|d| d.total_space() - d.available_space())
        .sum();
    Json(serde_json::json!({
        "cpu_usage":  sys.global_cpu_usage(),
        "mem_total":  sys.total_memory(),
        "mem_used":   sys.used_memory(),
        "disk_total": disk_total,
        "disk_used":  disk_used,
        "os":         sysinfo::System::os_version().unwrap_or_default(),
    }))
}

pub async fn qr_code(State(_): State<AppState>, jar: PrivateCookieJar) -> impl IntoResponse {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let ip = local_ip().unwrap_or_else(|| "localhost".into());
    let url = format!("http://{}:5000", ip);
    let code = qrcode::QrCode::new(url.as_bytes()).unwrap();
    let svg = code.render::<qrcode::render::svg::Color>().build();
    ([(header::CONTENT_TYPE, "image/svg+xml")], svg).into_response()
}

fn local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

pub async fn pwa_manifest() -> impl IntoResponse {
    let manifest = serde_json::json!({
        "name": "摄像头监控系统",
        "short_name": "Camera RS",
        "start_url": "/",
        "display": "standalone",
        "background_color": "#1a1a2e",
        "theme_color": "#e94560",
        "description": "Rust 摄像头监控系统",
        "icons": [
            {"src": "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>📷</text></svg>", "type": "image/svg+xml", "sizes": "any"}
        ]
    });
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        manifest.to_string(),
    )
        .into_response()
}
