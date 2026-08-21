use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;

use crate::state::AppState;
use super::{is_authed, OkResp};

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
