use axum::{
    extract::{Query, State},
    Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;

use super::{is_authed, now_secs, ts_str, OkResp, ValQuery};
use crate::state::AppState;

// ============================================================
//  摄像头枚举与切换
// ============================================================

#[derive(Serialize)]
pub struct CameraInfo {
    pub index: u32,
    pub name: String,
}

pub async fn cameras_list(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> Json<Vec<CameraInfo>> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(vec![]);
    }
    let cams = state.available_cameras.lock();
    Json(
        cams.iter()
            .map(|(idx, name)| CameraInfo {
                index: *idx,
                name: name.clone(),
            })
            .collect(),
    )
}

#[derive(Deserialize)]
pub struct SwitchQuery {
    index: u32,
}

pub async fn switch_camera(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<SwitchQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp::default());
    }
    state.camera_idx.store(q.index as usize, Ordering::Relaxed);
    state.camera.lock().prev_gray = None;
    let msg = serde_json::json!({"event":"camera_switched","index":q.index}).to_string();
    state.ws_tx.send(msg).ok();
    Json(OkResp {
        ok: true,
        error: None,
    })
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
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp::default());
    }
    let mut s = state.image_settings.write();
    if let Some(v) = q.brightness {
        s.brightness = v.max(-100).min(100);
    }
    if let Some(v) = q.contrast {
        s.contrast = v.max(-100).min(100);
    }
    if let Some(v) = q.saturation {
        s.saturation = v.max(-100).min(100);
    }
    Json(OkResp {
        ok: true,
        error: None,
    })
}

#[derive(Deserialize)]
pub struct FlipQuery {
    h: Option<u8>,
    v: Option<u8>,
}

pub async fn set_flip(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<FlipQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp::default());
    }
    let mut s = state.image_settings.write();
    if q.h.is_some() {
        s.flip_h = !s.flip_h;
    }
    if q.v.is_some() {
        s.flip_v = !s.flip_v;
    }
    Json(OkResp {
        ok: true,
        error: None,
    })
}

#[derive(Deserialize)]
pub struct RotQuery {
    deg: u32,
}

pub async fn set_rotation(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<RotQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp::default());
    }
    state.image_settings.write().rotation = match q.deg {
        0 | 90 | 180 | 270 => q.deg,
        _ => 0,
    };
    Json(OkResp {
        ok: true,
        error: None,
    })
}

// ============================================================
//  拍照、录像、开关等
// ============================================================

pub async fn take_photo(State(state): State<AppState>, jar: PrivateCookieJar) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp {
            ok: false,
            error: Some("Unauthorized".into()),
        });
    }
    match state.camera.lock().latest_jpeg.clone() {
        Some(j) => {
            std::fs::write(format!("{}/photos/{}.jpg", crate::SAVE_DIR, ts_str()), &*j).ok();
            crate::storage::cleanup_old_files(crate::SAVE_DIR, crate::MAX_STORAGE_MB);
            Json(OkResp {
                ok: true,
                error: None,
            })
        }
        None => Json(OkResp {
            ok: false,
            error: Some("无帧数据".into()),
        }),
    }
}

#[derive(Serialize, Default)]
pub struct RecordResp {
    recording: bool,
}

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
            crate::camera::save_mjpeg_avi(&f, crate::RECORD_FPS, &path).ok();
        });
    }
    Json(RecordResp { recording })
}

#[derive(Deserialize)]
pub struct ToggleQuery {
    name: String,
    on: Option<u8>,
}

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
    use std::time::Duration;
    loop {
        let (on, interval, jpeg) = {
            let cam = state.camera.lock();
            (cam.auto_capture, cam.auto_interval, cam.latest_jpeg.clone())
        };
        if !on {
            break;
        }
        if let Some(j) = jpeg {
            std::fs::write(format!("{}/auto/{}.jpg", crate::SAVE_DIR, ts_str()), &*j).ok();
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

// ============================================================
//  数值参数设置
// ============================================================

pub async fn set_interval(
    State(s): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<ValQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp::default());
    }
    s.camera.lock().auto_interval = q.val.and_then(|v| v.parse().ok()).unwrap_or(10u64).max(1);
    Json(OkResp {
        ok: true,
        error: None,
    })
}

pub async fn set_sensitivity(
    State(s): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<ValQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp::default());
    }
    s.camera.lock().sensitivity = q.val.and_then(|v| v.parse().ok()).unwrap_or(30);
    Json(OkResp {
        ok: true,
        error: None,
    })
}

pub async fn set_min_area(
    State(s): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<ValQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp::default());
    }
    s.camera.lock().min_area = q.val.and_then(|v| v.parse().ok()).unwrap_or(1500);
    Json(OkResp {
        ok: true,
        error: None,
    })
}

pub async fn set_frame_skip(
    State(s): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<ValQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp::default());
    }
    s.camera.lock().frame_skip = q
        .val
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(10)
        .max(1);
    Json(OkResp {
        ok: true,
        error: None,
    })
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
    })
}

// ============================================================
//  运动检测区域
// ============================================================

pub async fn get_motion_zones(
    State(s): State<AppState>,
    jar: PrivateCookieJar,
) -> Json<Vec<crate::state::MotionZone>> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(vec![]);
    }
    Json(s.motion_zones.read().clone())
}

pub async fn save_motion_zones(
    State(s): State<AppState>,
    jar: PrivateCookieJar,
    Json(zones): Json<Vec<crate::state::MotionZone>>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp::default());
    }
    *s.motion_zones.write() = zones;
    Json(OkResp {
        ok: true,
        error: None,
    })
}
