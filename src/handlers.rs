/*!
 * HTTP 请求处理器
 */

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use axum::{
    extract::{Path, Query, State, ConnectInfo},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use axum::extract::ws::{WebSocket, WebSocketUpgrade, Message};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use std::net::SocketAddr;

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

// ============================================================
//  登录 / 登出（含失败锁定）
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
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    axum::Form(form): axum::Form<LoginForm>,
) -> impl IntoResponse {
    let ip = addr.ip().to_string();
    let now = now_secs();
    let (password, max_attempts, lockout_secs) = {
        let sec = state.security.lock();
        (sec.password.clone(), sec.max_login_attempts, sec.lockout_secs)
    };

    {
        let attempts = state.login_attempts.lock();
        if let Some(&(cnt, lockout_until)) = attempts.get(&ip) {
            if cnt >= max_attempts && now < lockout_until {
                let remaining = lockout_until - now;
                return axum::response::Redirect::to(
                    &format!("/login?error=locked&secs={}", remaining)
                ).into_response();
            }
        }
    }

    if password.is_empty() || hash_password(&password) == hash_password(&form.password) {
        state.login_attempts.lock().remove(&ip);
        let mut c = Cookie::new("session", "ok");
        c.set_path("/");
        return (jar.add(c), axum::response::Redirect::to("/")).into_response();
    }

    {
        let mut attempts = state.login_attempts.lock();
        let entry = attempts.entry(ip).or_insert((0, 0));
        entry.0 += 1;
        if entry.0 >= max_attempts {
            entry.1 = now + lockout_secs;
        }
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

#[derive(Deserialize, Default)]
pub struct CamQuery { cam: Option<usize> }

pub async fn video_stream(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<CamQuery>,
) -> impl IntoResponse {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let cam_idx = q.cam.unwrap_or_else(|| state.camera_idx.load(Ordering::Relaxed));
    let rx = {
        let txs = state.frame_txs.lock();
        if let Some(tx) = txs.get(&cam_idx) {
            tx.subscribe()
        } else {
            state.frame_tx.subscribe()
        }
    };
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
//  WebSocket 事件推送
// ============================================================

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> impl IntoResponse {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    ws.on_upgrade(|socket| ws_client(socket, state)).into_response()
}

async fn ws_client(mut socket: WebSocket, state: AppState) {
    let mut rx = state.ws_tx.subscribe();
    loop {
        tokio::select! {
            Ok(msg) = rx.recv() => {
                if socket.send(Message::Text(msg)).await.is_err() { break; }
            }
            Some(Ok(_)) = socket.recv() => {}
            else => break,
        }
    }
}

// ============================================================
//  摄像头管理
// ============================================================

#[derive(Serialize)]
pub struct CameraInfo { pub index: u32, pub name: String }

pub async fn cameras_list(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> Json<Vec<CameraInfo>> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(vec![]); }
    let cams = state.available_cameras.lock();
    Json(cams.iter().map(|(idx, name)| CameraInfo { index: *idx, name: name.clone() }).collect())
}

#[derive(Deserialize)]
pub struct SwitchQuery { index: u32 }

pub async fn switch_camera(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<SwitchQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    state.camera_idx.store(q.index as usize, Ordering::Relaxed);
    state.camera.lock().prev_gray = None;
    let msg = serde_json::json!({"event":"camera_switched","index":q.index}).to_string();
    state.ws_tx.send(msg).ok();
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  图像调节
// ============================================================

#[derive(Deserialize)]
pub struct ImageQuery {
    brightness: Option<i32>,
    contrast: Option<i32>,
    saturation: Option<i32>,
}

pub async fn set_image(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<ImageQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    let mut s = state.image_settings.lock();
    if let Some(v) = q.brightness { s.brightness = v.max(-100).min(100); }
    if let Some(v) = q.contrast   { s.contrast   = v.max(-100).min(100); }
    if let Some(v) = q.saturation { s.saturation = v.max(-100).min(100); }
    Json(OkResp { ok: true, error: None })
}

#[derive(Deserialize)]
pub struct FlipQuery { h: Option<u8>, v: Option<u8> }

pub async fn set_flip(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<FlipQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    let mut s = state.image_settings.lock();
    if q.h.is_some() { s.flip_h = !s.flip_h; }
    if q.v.is_some() { s.flip_v = !s.flip_v; }
    Json(OkResp { ok: true, error: None })
}

#[derive(Deserialize)]
pub struct RotQuery { deg: u32 }

pub async fn set_rotation(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<RotQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    state.image_settings.lock().rotation = match q.deg { 0|90|180|270 => q.deg, _ => 0 };
    Json(OkResp { ok: true, error: None })
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
    match state.camera.lock().latest_jpeg.clone() {
        Some(j) => {
            std::fs::write(format!("{}/photos/{}.jpg", crate::SAVE_DIR, ts_str()), &*j).ok();
            crate::storage::cleanup_old_files(crate::SAVE_DIR, crate::MAX_STORAGE_MB);
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
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(RecordResp::default()); }
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
        tokio::task::spawn_blocking(move || { save_mjpeg_avi(&f, crate::RECORD_FPS, &path).ok(); });
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
            cam.motion_detect = on; cam.prev_gray = None;
            Json(serde_json::json!({"motion": on}))
        }
        "gate" => { state.camera.lock().motion_gate = on; Json(serde_json::json!({"gate": on})) }
        "auto"  => {
            state.camera.lock().auto_capture = on;
            if on { let s = state.clone(); tokio::spawn(async move { auto_capture_task(s).await }); }
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

pub async fn set_interval(State(s): State<AppState>, jar: PrivateCookieJar, Query(q): Query<ValQuery>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    s.camera.lock().auto_interval = q.val.and_then(|v| v.parse().ok()).unwrap_or(10u64).max(1);
    Json(OkResp { ok: true, error: None })
}

pub async fn set_sensitivity(State(s): State<AppState>, jar: PrivateCookieJar, Query(q): Query<ValQuery>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    s.camera.lock().sensitivity = q.val.and_then(|v| v.parse().ok()).unwrap_or(30);
    Json(OkResp { ok: true, error: None })
}

pub async fn set_min_area(State(s): State<AppState>, jar: PrivateCookieJar, Query(q): Query<ValQuery>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    s.camera.lock().min_area = q.val.and_then(|v| v.parse().ok()).unwrap_or(1500);
    Json(OkResp { ok: true, error: None })
}

pub async fn set_frame_skip(State(s): State<AppState>, jar: PrivateCookieJar, Query(q): Query<ValQuery>) -> Json<OkResp> {
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
    camera_idx: usize,
    fps: f32,
}

pub async fn get_stats(State(state): State<AppState>, _jar: PrivateCookieJar) -> Json<StatsResp> {
    let cam = state.camera.lock();
    Json(StatsResp {
        resolution: format!("{}x{}", cam.resolution.0, cam.resolution.1),
        motion_count: cam.motion_count,
        motion_now: cam.motion_now,
        unknown_count: cam.unknown_count,
        unknown_face: false,
        camera_idx: state.camera_idx.load(Ordering::Relaxed),
        fps: cam.fps_current,
    })
}

// ============================================================
//  文件管理
// ============================================================

const ALLOWED_TYPES: &[&str] = &["photos", "videos", "motion", "auto", "alerts"];

#[derive(Deserialize)]
pub struct TypeQuery { #[serde(rename = "type")] ftype: Option<String> }

#[derive(Serialize)]
pub struct FilesResp { files: Vec<String> }

pub async fn list_files(_: State<AppState>, jar: PrivateCookieJar, Query(q): Query<TypeQuery>) -> Json<FilesResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(FilesResp { files: vec![] }); }
    let ftype = q.ftype.unwrap_or_else(|| "photos".into());
    if !ALLOWED_TYPES.contains(&ftype.as_str()) { return Json(FilesResp { files: vec![] }); }
    let files = std::fs::read_dir(format!("{}/{}", crate::SAVE_DIR, ftype)).map(|rd| {
        let mut v: Vec<String> = rd.filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| !n.starts_with('.')).collect();
        v.sort_by(|a, b| b.cmp(a)); v
    }).unwrap_or_default();
    Json(FilesResp { files })
}

pub async fn serve_file(_: State<AppState>, jar: PrivateCookieJar, Path((ftype, filename)): Path<(String, String)>) -> impl IntoResponse {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    if !ALLOWED_TYPES.contains(&ftype.as_str()) {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    }
    let safe = std::path::Path::new(&filename).file_name()
        .map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    match std::fs::read(format!("{}/{}/{}", crate::SAVE_DIR, ftype, safe)) {
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
pub struct DeleteQuery { #[serde(rename = "type")] ftype: Option<String>, name: Option<String> }

pub async fn delete_file(_: State<AppState>, jar: PrivateCookieJar, Query(q): Query<DeleteQuery>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    let ftype = q.ftype.unwrap_or_default();
    let name  = q.name.unwrap_or_default();
    if !ALLOWED_TYPES.contains(&ftype.as_str()) || name.is_empty() {
        return Json(OkResp { ok: false, error: Some("invalid params".into()) });
    }
    let safe = std::path::Path::new(&name).file_name()
        .map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
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
    smtp_host: Option<String>, smtp_port: Option<u16>,
    email_from: Option<String>, email_password: Option<String>, email_to: Option<String>,
    cooldown: Option<u64>, on_motion: Option<bool>, on_unknown: Option<bool>,
}

pub async fn save_config(State(state): State<AppState>, jar: PrivateCookieJar, Json(p): Json<ConfigPayload>) -> Json<OkResp> {
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

pub async fn test_email_route(State(state): State<AppState>, jar: PrivateCookieJar) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp { ok: false, error: Some("Unauthorized".into()) });
    }
    match crate::email::send_test(&state) {
        Ok(_)  => Json(OkResp { ok: true, error: None }),
        Err(e) => Json(OkResp { ok: false, error: Some(e) }),
    }
}

// ============================================================
//  通知渠道配置
// ============================================================

pub async fn save_notify_config(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Json(cfg): Json<crate::notify::NotifyConfig>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp { ok: false, error: Some("Unauthorized".into()) });
    }
    *state.notify_cfg.lock() = cfg;
    Json(OkResp { ok: true, error: None })
}

pub async fn get_notify_config(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> Json<crate::notify::NotifyConfig> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(crate::notify::NotifyConfig::default());
    }
    Json(state.notify_cfg.lock().clone())
}

pub async fn test_notify(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp { ok: false, error: Some("Unauthorized".into()) });
    }
    let cfg = state.notify_cfg.lock().clone();
    tokio::spawn(async move {
        crate::notify::send_all(&cfg, crate::notify::NotifyEvent::Custom {
            title: "🔔 通知测试",
            body:  "摄像头监控系统通知渠道测试成功！",
        }).await;
    });
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  OneDrive 配置
// ============================================================

pub async fn save_onedrive_config(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Json(cfg): Json<crate::upload::OneDriveConfig>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp { ok: false, error: Some("Unauthorized".into()) });
    }
    *state.onedrive_cfg.lock() = cfg;
    Json(OkResp { ok: true, error: None })
}

pub async fn get_onedrive_config(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> Json<crate::upload::OneDriveConfig> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(crate::upload::OneDriveConfig::default());
    }
    Json(state.onedrive_cfg.lock().clone())
}

pub async fn create_onedrive_share(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> Json<serde_json::Value> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(serde_json::json!({"ok": false}));
    }
    let cfg = state.onedrive_cfg.lock().clone();
    match crate::upload::create_folder_share(&cfg).await {
        Some(url) => {
            state.onedrive_cfg.lock().share_folder_url = url.clone();
            Json(serde_json::json!({"ok": true, "url": url}))
        }
        None => Json(serde_json::json!({"ok": false, "error": "生成失败，请检查 Maton API Key"})),
    }
}

pub async fn upload_now(
    State(state): State<AppState>, jar: PrivateCookieJar,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp { ok: false, error: Some("Unauthorized".into()) });
    }
    let jpeg = state.camera.lock().latest_jpeg.clone();
    match jpeg {
        Some(j) => {
            let od = state.onedrive_cfg.lock().clone();
            let gd = state.gdrive_cfg.lock().clone();
            let ft = state.ftp_cfg.lock().clone();
            let fname = format!("photos/{}.jpg", ts_str());
            tokio::spawn(async move {
                crate::upload::upload_all(&od, &gd, &ft, &fname, &j, crate::upload::UploadKind::Photo).await;
            });
            Json(OkResp { ok: true, error: None })
        }
        None => Json(OkResp { ok: false, error: Some("无帧数据".into()) }),
    }
}

// ============================================================
//  Google Drive & FTP 配置
// ============================================================

pub async fn get_gdrive_config(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<crate::upload::GoogleDriveConfig> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(Default::default()); }
    Json(s.gdrive_cfg.lock().clone())
}
pub async fn save_gdrive_config(State(s): State<AppState>, jar: PrivateCookieJar, Json(c): Json<crate::upload::GoogleDriveConfig>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    *s.gdrive_cfg.lock() = c;
    Json(OkResp { ok: true, error: None })
}

pub async fn get_ftp_config(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<crate::upload::FtpConfig> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(Default::default()); }
    Json(s.ftp_cfg.lock().clone())
}
pub async fn save_ftp_config(State(s): State<AppState>, jar: PrivateCookieJar, Json(c): Json<crate::upload::FtpConfig>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    *s.ftp_cfg.lock() = c;
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  运动检测区域
// ============================================================

pub async fn get_motion_zones(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<Vec<crate::state::MotionZone>> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(vec![]); }
    Json(s.motion_zones.lock().clone())
}
pub async fn save_motion_zones(State(s): State<AppState>, jar: PrivateCookieJar, Json(zones): Json<Vec<crate::state::MotionZone>>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    *s.motion_zones.lock() = zones;
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  事件日志
// ============================================================

#[derive(Deserialize, Default)]
pub struct EventQuery { limit: Option<usize>, kind: Option<String> }

pub async fn get_events(State(s): State<AppState>, Query(q): Query<EventQuery>, jar: PrivateCookieJar) -> Json<serde_json::Value> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(serde_json::json!([])); }
    let limit = q.limit.unwrap_or(100).min(500);
    let events = match q.kind.as_deref() {
        Some(k) => s.event_log.by_kind(k, limit),
        None    => s.event_log.recent(limit),
    };
    Json(serde_json::json!({"events": events, "total": s.event_log.count()}))
}

pub async fn clear_events(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    s.event_log.clear();
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  定时任务
// ============================================================

pub async fn get_schedule(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<Vec<crate::schedule::ScheduleRule>> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(vec![]); }
    Json(s.schedule_rules.lock().clone())
}
pub async fn save_schedule(State(s): State<AppState>, jar: PrivateCookieJar, Json(rules): Json<Vec<crate::schedule::ScheduleRule>>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    *s.schedule_rules.lock() = rules;
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  告警时间规则
// ============================================================

pub async fn get_alert_rule(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<crate::state::AlertTimeRule> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(Default::default()); }
    Json(s.alert_time_rule.lock().clone())
}
pub async fn save_alert_rule(State(s): State<AppState>, jar: PrivateCookieJar, Json(rule): Json<crate::state::AlertTimeRule>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    *s.alert_time_rule.lock() = rule;
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  用户管理
// ============================================================

pub async fn list_users(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<serde_json::Value> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(serde_json::json!([])); }
    let users: Vec<_> = s.users.lock().iter().map(|u| serde_json::json!({
        "username": u.username, "role": u.role, "enabled": u.enabled
    })).collect();
    Json(serde_json::json!(users))
}

#[derive(Deserialize)]
pub struct UserForm { username: String, password: Option<String>, role: String, enabled: bool }

pub async fn save_users(State(s): State<AppState>, jar: PrivateCookieJar, Json(forms): Json<Vec<UserForm>>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    let mut users = s.users.lock();
    for form in forms {
        if let Some(u) = users.iter_mut().find(|u| u.username == form.username) {
            u.enabled = form.enabled;
            u.role = if form.role == "admin" { crate::state::UserRole::Admin } else { crate::state::UserRole::Viewer };
            if let Some(pw) = &form.password { if !pw.is_empty() { u.password_hash = crate::auth::hash_password(pw); } }
        } else {
            let pw_hash = form.password.as_deref().map(crate::auth::hash_password).unwrap_or_default();
            users.push(crate::state::UserAccount {
                username: form.username, password_hash: pw_hash,
                role: if form.role == "admin" { crate::state::UserRole::Admin } else { crate::state::UserRole::Viewer },
                enabled: form.enabled,
            });
        }
    }
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  API Key 管理
// ============================================================

pub async fn get_api_config(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<crate::state::ApiConfig> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(Default::default()); }
    Json(s.api_cfg.lock().clone())
}
pub async fn save_api_config(State(s): State<AppState>, jar: PrivateCookieJar, Json(mut c): Json<crate::state::ApiConfig>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    if c.api_key.is_empty() { c.api_key = crate::auth::generate_api_key(); }
    *s.api_cfg.lock() = c;
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  REST API（X-API-Key 认证）
// ============================================================

fn check_api_key(headers: &axum::http::HeaderMap, state: &AppState) -> bool {
    let cfg = state.api_cfg.lock();
    if !cfg.enabled { return false; }
    headers.get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .map(|k| k == cfg.api_key)
        .unwrap_or(false)
}

pub async fn api_snapshot(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if !check_api_key(&headers, &state) {
        return (StatusCode::UNAUTHORIZED, "Invalid API Key").into_response();
    }
    match state.camera.lock().latest_jpeg.clone() {
        Some(j) => ([(header::CONTENT_TYPE, "image/jpeg")], j.as_ref().clone()).into_response(),
        None    => (StatusCode::SERVICE_UNAVAILABLE, "No frame").into_response(),
    }
}

pub async fn api_stats(State(state): State<AppState>, headers: axum::http::HeaderMap) -> impl IntoResponse {
    if !check_api_key(&headers, &state) {
        return (StatusCode::UNAUTHORIZED, "Invalid API Key").into_response();
    }
    let cam = state.camera.lock();
    Json(serde_json::json!({
        "resolution": format!("{}x{}", cam.resolution.0, cam.resolution.1),
        "fps": cam.fps_current,
        "motion_count": cam.motion_count,
        "motion_now": cam.motion_now,
        "recording": cam.recording,
        "camera_idx": state.camera_idx.load(std::sync::atomic::Ordering::Relaxed),
    })).into_response()
}

pub async fn api_events(State(state): State<AppState>, headers: axum::http::HeaderMap) -> impl IntoResponse {
    if !check_api_key(&headers, &state) {
        return (StatusCode::UNAUTHORIZED, "Invalid API Key").into_response();
    }
    Json(state.event_log.recent(50)).into_response()
}

#[derive(Deserialize)]
pub struct TriggerBody { action: String }

pub async fn api_trigger(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<TriggerBody>,
) -> Json<OkResp> {
    if !check_api_key(&headers, &state) {
        return Json(OkResp { ok: false, error: Some("Invalid API Key".into()) });
    }
    match body.action.as_str() {
        "photo"  => { let jpeg = state.camera.lock().latest_jpeg.clone(); if let Some(j) = jpeg { std::fs::write(format!("{}/photos/{}.jpg", crate::SAVE_DIR, ts_str()), &*j).ok(); } }
        "motion_on"  => { state.camera.lock().motion_detect = true; }
        "motion_off" => { state.camera.lock().motion_detect = false; }
        _ => return Json(OkResp { ok: false, error: Some("unknown action".into()) }),
    }
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  系统工具
// ============================================================

pub async fn health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
    let cam = state.camera.lock();
    Json(serde_json::json!({
        "status": "ok",
        "camera": cam.resolution != (0,0),
        "fps": cam.fps_current,
        "uptime": "running",
        "version": "3.0.0",
    }))
}

pub async fn sys_info(State(_): State<AppState>, jar: PrivateCookieJar) -> Json<serde_json::Value> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(serde_json::json!({})); }
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();
    let disk_total: u64 = sysinfo::Disks::new_with_refreshed_list().iter().map(|d| d.total_space()).sum();
    let disk_used: u64  = sysinfo::Disks::new_with_refreshed_list().iter().map(|d| d.total_space() - d.available_space()).sum();
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
    // 获取本机 IP
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
    ([(header::CONTENT_TYPE, "application/manifest+json")], manifest.to_string()).into_response()
}

pub async fn export_config(State(state): State<AppState>, jar: PrivateCookieJar) -> impl IntoResponse {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let export = serde_json::json!({
        "notify":         *state.notify_cfg.lock(),
        "onedrive":       *state.onedrive_cfg.lock(),
        "gdrive":         *state.gdrive_cfg.lock(),
        "ftp":            *state.ftp_cfg.lock(),
        "schedule":       *state.schedule_rules.lock(),
        "alert_rule":     *state.alert_time_rule.lock(),
        "motion_zones":   *state.motion_zones.lock(),
        "image_settings": *state.image_settings.lock(),
        "watermark":      *state.watermark_cfg.lock(),
        "privacy_masks":  *state.privacy_masks.lock(),
        "rtsp_cameras":   *state.rtsp_cameras.lock(),
        "security":       *state.security.lock(),
        "record_limits":  *state.record_limits.lock(),
    });
    let json = serde_json::to_string_pretty(&export).unwrap_or_default();
    ([(header::CONTENT_TYPE, "application/json"),
      (header::CONTENT_DISPOSITION, "attachment; filename=camera_rs_config.json")],
     json).into_response()
}

pub async fn import_config(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Json(data): Json<serde_json::Value>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    if let Some(v) = data.get("notify")          { if let Ok(c) = serde_json::from_value(v.clone()) { *state.notify_cfg.lock() = c; } }
    if let Some(v) = data.get("onedrive")        { if let Ok(c) = serde_json::from_value(v.clone()) { *state.onedrive_cfg.lock() = c; } }
    if let Some(v) = data.get("gdrive")          { if let Ok(c) = serde_json::from_value(v.clone()) { *state.gdrive_cfg.lock() = c; } }
    if let Some(v) = data.get("ftp")             { if let Ok(c) = serde_json::from_value(v.clone()) { *state.ftp_cfg.lock() = c; } }
    if let Some(v) = data.get("schedule")        { if let Ok(r) = serde_json::from_value(v.clone()) { *state.schedule_rules.lock() = r; } }
    if let Some(v) = data.get("alert_rule")      { if let Ok(r) = serde_json::from_value(v.clone()) { *state.alert_time_rule.lock() = r; } }
    if let Some(v) = data.get("motion_zones")    { if let Ok(z) = serde_json::from_value(v.clone()) { *state.motion_zones.lock() = z; } }
    if let Some(v) = data.get("image_settings")  { if let Ok(c) = serde_json::from_value(v.clone()) { *state.image_settings.lock() = c; } }
    if let Some(v) = data.get("watermark")       { if let Ok(c) = serde_json::from_value(v.clone()) { *state.watermark_cfg.lock() = c; } }
    if let Some(v) = data.get("privacy_masks")   { if let Ok(m) = serde_json::from_value(v.clone()) { *state.privacy_masks.lock() = m; } }
    if let Some(v) = data.get("rtsp_cameras")    { if let Ok(c) = serde_json::from_value(v.clone()) { *state.rtsp_cameras.lock() = c; } }
    if let Some(v) = data.get("security")        { if let Ok(c) = serde_json::from_value(v.clone()) { *state.security.lock() = c; } }
    if let Some(v) = data.get("record_limits")   { if let Ok(c) = serde_json::from_value(v.clone()) { *state.record_limits.lock() = c; } }
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  延时摄影
// ============================================================

pub async fn get_timelapse_cfg(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<crate::state::TimelapseConfig> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(Default::default()); }
    Json(s.timelapse_cfg.lock().clone())
}
pub async fn save_timelapse_cfg(State(s): State<AppState>, jar: PrivateCookieJar, Json(c): Json<crate::state::TimelapseConfig>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    *s.timelapse_cfg.lock() = c;
    Json(OkResp { ok: true, error: None })
}

pub async fn build_timelapse(State(state): State<AppState>, jar: PrivateCookieJar) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    let cfg = state.timelapse_cfg.lock().clone();
    tokio::task::spawn_blocking(move || {
        build_timelapse_avi(&cfg).unwrap_or_else(|e| tracing::warn!("延时摄影生成失败: {}", e));
    });
    Json(OkResp { ok: true, error: None })
}

fn build_timelapse_avi(cfg: &crate::state::TimelapseConfig) -> anyhow::Result<()> {
    let dir = format!("{}/auto", crate::SAVE_DIR);
    let mut files: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".jpg"))
        .collect();
    files.sort_by_key(|e| e.file_name());
    let max = cfg.max_frames as usize;
    if files.len() > max { files.drain(0..files.len()-max); }

    let frames: Vec<Vec<u8>> = files.iter()
        .filter_map(|e| std::fs::read(e.path()).ok())
        .collect();

    if frames.is_empty() { return Ok(()); }
    let path = format!("{}/timelapse/{}.avi", crate::SAVE_DIR, chrono::Local::now().format("%Y%m%d_%H%M%S"));
    crate::camera::save_mjpeg_avi(&frames, cfg.fps as f64, &path)?;
    tracing::info!("延时摄影生成: {} 帧 -> {}", frames.len(), path);
    Ok(())
}

// ============================================================
//  多摄像头分屏页
// ============================================================

pub async fn multiview_page(_: State<AppState>, jar: PrivateCookieJar) -> impl IntoResponse {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return axum::response::Redirect::to("/login").into_response();
    }
    Html(crate::html::MULTIVIEW_HTML).into_response()
}

use sysinfo;

// ============================================================
//  水印配置
// ============================================================

pub async fn get_watermark(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<crate::state::WatermarkConfig> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(Default::default()); }
    Json(s.watermark_cfg.lock().clone())
}
pub async fn save_watermark(State(s): State<AppState>, jar: PrivateCookieJar, Json(c): Json<crate::state::WatermarkConfig>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    *s.watermark_cfg.lock() = c;
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  隐私遮罩
// ============================================================

pub async fn get_privacy_masks(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<Vec<crate::state::PrivacyMask>> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(vec![]); }
    Json(s.privacy_masks.lock().clone())
}
pub async fn save_privacy_masks(State(s): State<AppState>, jar: PrivateCookieJar, Json(masks): Json<Vec<crate::state::PrivacyMask>>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    *s.privacy_masks.lock() = masks;
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  运动热力图
// ============================================================

pub async fn get_heatmap_json(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<serde_json::Value> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(serde_json::json!({})); }
    Json(crate::heatmap::heatmap_json(&s))
}

pub async fn get_heatmap_image(State(s): State<AppState>, jar: PrivateCookieJar) -> impl IntoResponse {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let png = crate::heatmap::render_heatmap_png(&s);
    ([(axum::http::header::CONTENT_TYPE, "image/png")], png).into_response()
}

pub async fn clear_heatmap_handler(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    crate::heatmap::clear_heatmap(&s);
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  每日报告（手动触发）
// ============================================================

pub async fn trigger_daily_report(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp { ok: false, error: Some("Unauthorized".into()) });
    }
    tokio::spawn(async move { crate::report::send_daily_report(&s).await });
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  RTSP 摄像头管理
// ============================================================

pub async fn get_rtsp_cameras(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<Vec<crate::state::RtspCamera>> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(vec![]); }
    Json(s.rtsp_cameras.lock().clone())
}
pub async fn save_rtsp_cameras(State(s): State<AppState>, jar: PrivateCookieJar, Json(cams): Json<Vec<crate::state::RtspCamera>>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    *s.rtsp_cameras.lock() = cams;
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  安全 / IP 白名单配置
// ============================================================

pub async fn get_security_config(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<crate::state::SecurityConfig> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(Default::default()); }
    Json(s.security.lock().clone())
}
pub async fn save_security_config(State(s): State<AppState>, jar: PrivateCookieJar, Json(c): Json<crate::state::SecurityConfig>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    *s.security.lock() = c;
    Json(OkResp { ok: true, error: None })
}

// ============================================================
//  录像限制配置
// ============================================================

pub async fn get_record_limits(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<crate::state::RecordLimits> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(Default::default()); }
    Json(s.record_limits.lock().clone())
}
pub async fn save_record_limits(State(s): State<AppState>, jar: PrivateCookieJar, Json(c): Json<crate::state::RecordLimits>) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    *s.record_limits.lock() = c;
    Json(OkResp { ok: true, error: None })
}
