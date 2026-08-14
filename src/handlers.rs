/*!
 * HTTP 请求处理器（axum handlers）
 *
 * 状态：State<AppState>（AppState: Clone，内部 Arc 共享数据）
 * Cookie：PrivateCookieJar（从 AppState::cookie_key 自动提取 Key）
 */

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::state::AppState;
use crate::html::{LOGIN_HTML, MAIN_HTML};
use crate::camera::save_mjpeg_avi;

// ============================================================
//  认证辅助
// ============================================================

fn hash_password(pw: &str) -> String {
    let mut h = Sha256::new();
    h.update(pw.as_bytes());
    hex::encode(h.finalize())
}

fn is_authed(jar: &PrivateCookieJar) -> bool {
    jar.get("session").map(|c| c.value() == "ok").unwrap_or(false)
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn ts_str() -> String {
    chrono::Local::now().format("%Y%m%d_%H%M%S").to_string()
}

macro_rules! require_auth {
    ($jar:expr) => {
        if !is_authed(&$jar) && !crate::PASSWORD.is_empty() {
            return axum::response::Redirect::to("/login").into_response();
        }
    };
}

macro_rules! require_auth_json {
    ($jar:expr, $t:ty) => {
        if !is_authed(&$jar) && !crate::PASSWORD.is_empty() {
            return Json::<$t>(Default::default());
        }
    };
}

// ============================================================
//  登录 / 登出
// ============================================================

pub async fn login_page(
    _: State<AppState>,
    jar: PrivateCookieJar,
) -> impl IntoResponse {
    if is_authed(&jar) {
        return axum::response::Redirect::to("/").into_response();
    }
    Html(LOGIN_HTML).into_response()
}

#[derive(Deserialize)]
pub struct LoginForm { password: String }

pub async fn login_post(
    _: State<AppState>,
    jar: PrivateCookieJar,
    axum::Form(form): axum::Form<LoginForm>,
) -> impl IntoResponse {
    if crate::PASSWORD.is_empty()
        || hash_password(crate::PASSWORD) == hash_password(&form.password)
    {
        let mut c = Cookie::new("session", "ok");
        c.set_path("/");
        return (jar.add(c), axum::response::Redirect::to("/")).into_response();
    }
    axum::response::Redirect::to("/login?error=1").into_response()
}

pub async fn logout(
    _: State<AppState>,
    jar: PrivateCookieJar,
) -> impl IntoResponse {
    (jar.remove(Cookie::from("session")), axum::response::Redirect::to("/login")).into_response()
}

// ============================================================
//  主页
// ============================================================

pub async fn index(
    _: State<AppState>,
    jar: PrivateCookieJar,
) -> impl IntoResponse {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return axum::response::Redirect::to("/login").into_response();
    }
    Html(MAIN_HTML).into_response()
}

// ============================================================
//  MJPEG 视频流
// ============================================================

pub async fn video_stream(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> impl IntoResponse {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let rx = state.frame_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        result.ok().map(|jpeg| {
            let mut chunk = Vec::new();
            chunk.extend_from_slice(b"--frame\r\nContent-Type: image/jpeg\r\n\r\n");
            chunk.extend_from_slice(&jpeg);
            chunk.extend_from_slice(b"\r\n");
            Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(chunk))
        })
    });

    Response::builder()
        .header(header::CONTENT_TYPE, "multipart/x-mixed-replace; boundary=frame")
        .header(header::CACHE_CONTROL, "no-cache, no-store")
        .body(axum::body::Body::from_stream(stream))
        .unwrap()
        .into_response()
}

// ============================================================
//  控制 API（拍照、录像、开关等）
// ============================================================

#[derive(Serialize, Default)]
pub struct OkResp {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn take_photo(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp { ok: false, error: Some("Unauthorized".into()) });
    }
    let jpeg = state.camera.lock().latest_jpeg.clone();
    match jpeg {
        Some(j) => {
            std::fs::write(format!("{}/photos/{}.jpg", crate::SAVE_DIR, ts_str()), &*j).ok();
            Json(OkResp { ok: true, error: None })
        }
        None => Json(OkResp { ok: false, error: Some("无帧数据".into()) }),
    }
}

#[derive(Serialize, Default)]
pub struct RecordResp { recording: bool }

pub async fn toggle_record(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> Json<RecordResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(RecordResp::default());
    }
    let (recording, frames) = {
        let mut cam = state.camera.lock();
        if cam.recording {
            cam.recording = false;
            (false, Some(std::mem::take(&mut cam.record_frames)))
        } else {
            cam.recording = true;
            cam.record_start = Some(now_secs());
            cam.record_frames.clear();
            (true, None)
        }
    };
    if let Some(f) = frames {
        let path = format!("{}/videos/{}.avi", crate::SAVE_DIR, ts_str());
        tokio::task::spawn_blocking(move || {
            if let Err(e) = save_mjpeg_avi(&f, crate::RECORD_FPS, &path) {
                tracing::error!("录像保存失败: {}", e);
            }
        });
    }
    Json(RecordResp { recording })
}

#[derive(Deserialize)]
pub struct ToggleQuery { name: String, on: Option<u8> }

pub async fn toggle_feature(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<ToggleQuery>,
) -> Json<serde_json::Value> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(serde_json::json!({"ok": false}));
    }
    let on = q.on.unwrap_or(0) == 1;
    match q.name.as_str() {
        "motion" => {
            let mut cam = state.camera.lock();
            cam.motion_detect = on;
            cam.prev_gray = None;
            Json(serde_json::json!({"motion": on}))
        }
        "gate" => {
            state.camera.lock().motion_gate = on;
            Json(serde_json::json!({"gate": on}))
        }
        "auto" => {
            state.camera.lock().auto_capture = on;
            if on {
                let s = state.clone();
                tokio::spawn(async move { auto_capture_task(s).await });
            }
            Json(serde_json::json!({"auto": on}))
        }
        _ => Json(serde_json::json!({"ok": false})),
    }
}

async fn auto_capture_task(state: AppState) {
    loop {
        let (on, interval, jpeg) = {
            let cam = state.camera.lock();
            (cam.auto_capture, cam.auto_interval, cam.latest_jpeg.clone())
        };
        if !on { break; }
        if let Some(j) = jpeg {
            std::fs::write(format!("{}/auto/{}.jpg", crate::SAVE_DIR, ts_str()), &*j).ok();
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

#[derive(Deserialize)]
pub struct ValQuery { val: Option<String> }

pub async fn set_interval(
    State(s): State<AppState>, jar: PrivateCookieJar, Query(q): Query<ValQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    s.camera.lock().auto_interval = q.val.and_then(|v| v.parse().ok()).unwrap_or(10u64).max(1);
    Json(OkResp { ok: true, error: None })
}

pub async fn set_sensitivity(
    State(s): State<AppState>, jar: PrivateCookieJar, Query(q): Query<ValQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    s.camera.lock().sensitivity = q.val.and_then(|v| v.parse().ok()).unwrap_or(30);
    Json(OkResp { ok: true, error: None })
}

pub async fn set_min_area(
    State(s): State<AppState>, jar: PrivateCookieJar, Query(q): Query<ValQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    s.camera.lock().min_area = q.val.and_then(|v| v.parse().ok()).unwrap_or(1500);
    Json(OkResp { ok: true, error: None })
}

pub async fn set_frame_skip(
    State(s): State<AppState>, jar: PrivateCookieJar, Query(q): Query<ValQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    s.camera.lock().frame_skip = q.val.and_then(|v| v.parse::<u32>().ok()).unwrap_or(10).max(1);
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  统计
// ============================================================

#[derive(Serialize)]
pub struct StatsResp {
    resolution: String,
    motion_count: u64,
    motion_now: bool,
    unknown_count: u64,
    unknown_face: bool,
}

pub async fn get_stats(
    State(state): State<AppState>,
    _jar: PrivateCookieJar,
) -> Json<StatsResp> {
    let cam = state.camera.lock();
    Json(StatsResp {
        resolution: format!("{}x{}", cam.resolution.0, cam.resolution.1),
        motion_count: cam.motion_count,
        motion_now: cam.motion_now,
        unknown_count: cam.unknown_count,
        unknown_face: false,
    })
}

// ============================================================
//  文件管理
// ============================================================

const ALLOWED_TYPES: &[&str] = &["photos", "videos", "motion", "auto", "alerts"];

#[derive(Deserialize)]
pub struct TypeQuery {
    #[serde(rename = "type")]
    ftype: Option<String>,
}

#[derive(Serialize)]
pub struct FilesResp { files: Vec<String> }

pub async fn list_files(
    _: State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<TypeQuery>,
) -> Json<FilesResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(FilesResp { files: vec![] });
    }
    let ftype = q.ftype.unwrap_or_else(|| "photos".into());
    if !ALLOWED_TYPES.contains(&ftype.as_str()) {
        return Json(FilesResp { files: vec![] });
    }
    let dir = format!("{}/{}", crate::SAVE_DIR, ftype);
    let files = std::fs::read_dir(&dir).map(|rd| {
        let mut v: Vec<String> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| !n.starts_with('.'))
            .collect();
        v.sort_by(|a, b| b.cmp(a));
        v
    }).unwrap_or_default();
    Json(FilesResp { files })
}

pub async fn serve_file(
    _: State<AppState>,
    jar: PrivateCookieJar,
    Path((ftype, filename)): Path<(String, String)>,
) -> impl IntoResponse {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    if !ALLOWED_TYPES.contains(&ftype.as_str()) {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    }
    let safe = std::path::Path::new(&filename)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let path = format!("{}/{}/{}", crate::SAVE_DIR, ftype, safe);
    match std::fs::read(&path) {
        Ok(data) => {
            let ct = if safe.ends_with(".avi") { "video/avi" }
                     else if safe.ends_with(".mp4") { "video/mp4" }
                     else { "image/jpeg" };
            ([(header::CONTENT_TYPE, ct)], data).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

#[derive(Deserialize)]
pub struct DeleteQuery {
    #[serde(rename = "type")] ftype: Option<String>,
    name: Option<String>,
}

pub async fn delete_file(
    _: State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<DeleteQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    let ftype = q.ftype.unwrap_or_default();
    let name  = q.name.unwrap_or_default();
    if !ALLOWED_TYPES.contains(&ftype.as_str()) || name.is_empty() {
        return Json(OkResp { ok: false, error: Some("invalid params".into()) });
    }
    let safe = std::path::Path::new(&name)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    match std::fs::remove_file(format!("{}/{}/{}", crate::SAVE_DIR, ftype, safe)) {
        Ok(_)  => Json(OkResp { ok: true, error: None }),
        Err(e) => Json(OkResp { ok: false, error: Some(e.to_string()) }),
    }
}

// ============================================================
//  邮件设置
// ============================================================

#[derive(Deserialize)]
pub struct ConfigPayload {
    email_enabled: Option<bool>,
    smtp_host: Option<String>,
    smtp_port: Option<u16>,
    email_from: Option<String>,
    email_password: Option<String>,
    email_to: Option<String>,
    cooldown: Option<u64>,
    on_motion: Option<bool>,
    on_unknown: Option<bool>,
}

pub async fn save_config(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Json(p): Json<ConfigPayload>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp { ok: false, error: Some("Unauthorized".into()) });
    }
    let mut cfg = state.email_cfg.lock();
    if let Some(v) = p.email_enabled { cfg.enabled = v; }
    if let Some(v) = p.smtp_host     { cfg.smtp_host = v; }
    if let Some(v) = p.smtp_port     { cfg.smtp_port = v; }
    if let Some(v) = p.email_from    { cfg.from = v; }
    if let Some(v) = p.email_password { if !v.is_empty() { cfg.password = v; } }
    if let Some(v) = p.email_to      { cfg.to = v; }
    if let Some(v) = p.cooldown      { cfg.cooldown = v; }
    if let Some(v) = p.on_motion     { cfg.on_motion = v; }
    if let Some(v) = p.on_unknown    { cfg.on_unknown = v; }
    Json(OkResp { ok: true, error: None })
}

pub async fn test_email_route(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp { ok: false, error: Some("Unauthorized".into()) });
    }
    match crate::email::send_test(&state) {
        Ok(_)  => Json(OkResp { ok: true, error: None }),
        Err(e) => Json(OkResp { ok: false, error: Some(e) }),
    }
}
