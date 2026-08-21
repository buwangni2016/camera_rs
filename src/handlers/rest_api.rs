use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use subtle::ConstantTimeEq;

use crate::state::AppState;
use super::{ts_str, OkResp};

// ============================================================
//  REST API（X-API-Key 认证）
// ============================================================

fn check_api_key(headers: &axum::http::HeaderMap, state: &AppState) -> bool {
    let cfg = state.api_cfg.lock();
    if !cfg.enabled { return false; }
    headers.get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .map(|k| {
            // 常量时间比较，防止时序攻击
            k.as_bytes().ct_eq(cfg.api_key.as_bytes()).into()
        })
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
        Some(j) => ([(axum::http::header::CONTENT_TYPE, "image/jpeg")], j.as_ref().clone()).into_response(),
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
        "photo" => {
            let jpeg = state.camera.lock().latest_jpeg.clone();
            if let Some(j) = jpeg {
                std::fs::write(format!("{}/photos/{}.jpg", crate::SAVE_DIR, ts_str()), &*j).ok();
            }
        }
        "motion_on"  => { state.camera.lock().motion_detect = true; }
        "motion_off" => { state.camera.lock().motion_detect = false; }
        _ => return Json(OkResp { ok: false, error: Some("unknown action".into()) }),
    }
    Json(OkResp { ok: true, error: None })
}
