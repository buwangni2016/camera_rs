use axum::{
    extract::{State, Query},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
};
use axum::extract::ws::{WebSocket, WebSocketUpgrade, Message};
use axum_extra::extract::cookie::PrivateCookieJar;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::state::AppState;
use super::is_authed;

// ============================================================
//  MJPEG 视频流
// ============================================================

#[derive(Deserialize)]
pub struct StreamQuery { pub cam: Option<usize> }

pub async fn video_stream(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<StreamQuery>,
) -> impl IntoResponse {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    // 支持 ?cam=N 选择特定摄像头频道；默认使用主广播
    let rx = if let Some(cam_idx) = q.cam {
        let txs = state.frame_txs.lock();
        match txs.get(&cam_idx) {
            Some(tx) => tx.subscribe(),
            None     => state.frame_tx.subscribe(),
        }
    } else {
        state.frame_tx.subscribe()
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
//  多摄像头分屏页
// ============================================================

pub async fn multiview_page(_: State<AppState>, jar: PrivateCookieJar) -> impl IntoResponse {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return axum::response::Redirect::to("/login").into_response();
    }
    Html(crate::html::MULTIVIEW_HTML).into_response()
}
