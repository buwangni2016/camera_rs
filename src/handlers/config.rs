use axum::{extract::State, Json, response::IntoResponse, http::{header, StatusCode}};
use axum_extra::extract::cookie::PrivateCookieJar;
use serde::Deserialize;

use crate::state::AppState;
use super::{is_authed, ts_str, OkResp};

// ============================================================
//  邮件设置
// ============================================================

#[derive(Deserialize)]
pub struct ConfigPayload {
    email_enabled:    Option<bool>,
    smtp_host:        Option<String>, smtp_port: Option<u16>,
    email_from:       Option<String>, email_password: Option<String>, email_to: Option<String>,
    cooldown:         Option<u64>,    on_motion: Option<bool>, on_unknown: Option<bool>,
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
    if let Some(v) = p.email_enabled   { cfg.enabled = v; }
    if let Some(v) = p.smtp_host       { cfg.smtp_host = v; }
    if let Some(v) = p.smtp_port       { cfg.smtp_port = v; }
    if let Some(v) = p.email_from      { cfg.from = v; }
    if let Some(v) = p.email_password  { if !v.is_empty() { cfg.password = v; } }
    if let Some(v) = p.email_to        { cfg.to = v; }
    if let Some(v) = p.cooldown        { cfg.cooldown = v; }
    if let Some(v) = p.on_motion       { cfg.on_motion = v; }
    if let Some(v) = p.on_unknown      { cfg.on_unknown = v; }
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
//  告警时间规则
// ============================================================

pub async fn get_alert_rule(State(s): State<AppState>, jar: PrivateCookieJar) -> Json<crate::state::AlertTimeRule> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(Default::default()); }
    Json(s.alert_time_rule.lock().clone())
}

pub async fn save_alert_rule(
    State(s): State<AppState>,
    jar: PrivateCookieJar,
    Json(rule): Json<crate::state::AlertTimeRule>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    *s.alert_time_rule.lock() = rule;
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
    State(state): State<AppState>,
    jar: PrivateCookieJar,
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

pub async fn save_users(
    State(s): State<AppState>,
    jar: PrivateCookieJar,
    Json(forms): Json<Vec<UserForm>>,
) -> Json<OkResp> {
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

pub async fn save_api_config(
    State(s): State<AppState>,
    jar: PrivateCookieJar,
    Json(mut c): Json<crate::state::ApiConfig>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() { return Json(OkResp::default()); }
    if c.api_key.is_empty() { c.api_key = crate::auth::generate_api_key(); }
    *s.api_cfg.lock() = c;
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

// ============================================================
//  配置导入 / 导出
// ============================================================

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
        "motion_zones":   *state.motion_zones.read(),
        "image_settings": *state.image_settings.read(),
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
    if let Some(v) = data.get("notify")        { if let Ok(c) = serde_json::from_value(v.clone()) { *state.notify_cfg.lock() = c; } }
    if let Some(v) = data.get("onedrive")      { if let Ok(c) = serde_json::from_value(v.clone()) { *state.onedrive_cfg.lock() = c; } }
    if let Some(v) = data.get("gdrive")        { if let Ok(c) = serde_json::from_value(v.clone()) { *state.gdrive_cfg.lock() = c; } }
    if let Some(v) = data.get("ftp")           { if let Ok(c) = serde_json::from_value(v.clone()) { *state.ftp_cfg.lock() = c; } }
    if let Some(v) = data.get("schedule")      { if let Ok(r) = serde_json::from_value(v.clone()) { *state.schedule_rules.lock() = r; } }
    if let Some(v) = data.get("alert_rule")    { if let Ok(r) = serde_json::from_value(v.clone()) { *state.alert_time_rule.lock() = r; } }
    if let Some(v) = data.get("motion_zones")  { if let Ok(z) = serde_json::from_value(v.clone()) { *state.motion_zones.write() = z; } }
    if let Some(v) = data.get("watermark")     { if let Ok(c) = serde_json::from_value(v.clone()) { *state.watermark_cfg.lock() = c; } }
    if let Some(v) = data.get("privacy_masks") { if let Ok(m) = serde_json::from_value(v.clone()) { *state.privacy_masks.lock() = m; } }
    if let Some(v) = data.get("rtsp_cameras")  { if let Ok(c) = serde_json::from_value(v.clone()) { *state.rtsp_cameras.lock() = c; } }
    if let Some(v) = data.get("security")      { if let Ok(c) = serde_json::from_value(v.clone()) { *state.security.lock() = c; } }
    if let Some(v) = data.get("record_limits") { if let Ok(c) = serde_json::from_value(v.clone()) { *state.record_limits.lock() = c; } }
    Json(OkResp { ok: true, error: None })
}
