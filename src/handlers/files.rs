use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use serde::{Deserialize, Serialize};

use super::{is_authed, OkResp};
use crate::state::AppState;

const ALLOWED_TYPES: &[&str] = &["photos", "videos", "motion", "auto", "alerts"];

#[derive(Deserialize)]
pub struct TypeQuery {
    #[serde(rename = "type")]
    ftype: Option<String>,
}

#[derive(Serialize)]
pub struct FilesResp {
    pub files: Vec<String>,
}

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
    let files = std::fs::read_dir(format!("{}/{}", crate::SAVE_DIR, ftype))
        .map(|rd| {
            let mut v: Vec<String> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| !n.starts_with('.'))
                .collect();
            v.sort_by(|a, b| b.cmp(a));
            v
        })
        .unwrap_or_default();
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
    // 防路径穿越：只取文件名部分
    let safe = std::path::Path::new(&filename)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    match std::fs::read(format!("{}/{}/{}", crate::SAVE_DIR, ftype, safe)) {
        Ok(data) => {
            let ct = if safe.ends_with(".avi") {
                "video/avi"
            } else if safe.ends_with(".mp4") {
                "video/mp4"
            } else {
                "image/jpeg"
            };
            ([(header::CONTENT_TYPE, ct)], data).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

#[derive(Deserialize)]
pub struct DeleteQuery {
    #[serde(rename = "type")]
    ftype: Option<String>,
    name: Option<String>,
}

pub async fn delete_file(
    _: State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<DeleteQuery>,
) -> Json<OkResp> {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return Json(OkResp::default());
    }
    let ftype = q.ftype.unwrap_or_default();
    let name = q.name.unwrap_or_default();
    if !ALLOWED_TYPES.contains(&ftype.as_str()) || name.is_empty() {
        return Json(OkResp {
            ok: false,
            error: Some("invalid params".into()),
        });
    }
    let safe = std::path::Path::new(&name)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    match std::fs::remove_file(format!("{}/{}/{}", crate::SAVE_DIR, ftype, safe)) {
        Ok(_) => Json(OkResp {
            ok: true,
            error: None,
        }),
        Err(e) => Json(OkResp {
            ok: false,
            error: Some(e.to_string()),
        }),
    }
}
