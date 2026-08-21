use axum::response::Html;
use axum::{
    extract::{ConnectInfo, State},
    response::IntoResponse,
};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar};
use serde::Deserialize;
use std::net::SocketAddr;

use super::{is_authed, now_secs, OkResp};
use crate::html::{LOGIN_HTML, MAIN_HTML};
use crate::state::AppState;

// ============================================================
//  登录 / 登出（含失败锁定 + Argon2id 自动升级）
// ============================================================

pub async fn login_page(_: State<AppState>, jar: PrivateCookieJar) -> impl IntoResponse {
    if is_authed(&jar) {
        return axum::response::Redirect::to("/").into_response();
    }
    Html(LOGIN_HTML).into_response()
}

#[derive(Deserialize)]
pub struct LoginForm {
    password: String,
}

pub async fn login_post(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    axum::Form(form): axum::Form<LoginForm>,
) -> impl IntoResponse {
    let ip = addr.ip().to_string();
    let now = now_secs();

    // 从 security 配置读取登录参数（支持运行时通过 /security_config 修改）
    let (cfg_password, max_attempts, lockout_secs) = {
        let sec = state.security.lock();
        (
            sec.password.clone(),
            sec.max_login_attempts,
            sec.lockout_secs,
        )
    };

    {
        let attempts = state.login_attempts.lock();
        if let Some(&(cnt, lockout_until)) = attempts.get(&ip) {
            if cnt >= max_attempts && now < lockout_until {
                let remaining = lockout_until - now;
                return axum::response::Redirect::to(&format!(
                    "/login?error=locked&secs={}",
                    remaining
                ))
                .into_response();
            }
        }
    }

    // 验证密码：先匹配用户列表（Argon2id），再回退到 security.password
    let authed = {
        let mut users = state.users.lock();
        let matched = users
            .iter_mut()
            .find(|u| u.enabled && crate::auth::verify_password(&form.password, &u.password_hash));
        if let Some(user) = matched {
            // 旧 SHA-256 哈希自动升级为 Argon2id
            if crate::auth::needs_upgrade(&user.password_hash) {
                user.password_hash = crate::auth::hash_password(&form.password);
            }
            true
        } else {
            cfg_password.is_empty() || form.password == cfg_password
        }
    };

    if authed {
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

pub async fn logout(_: State<AppState>, jar: PrivateCookieJar) -> impl IntoResponse {
    (
        jar.remove(Cookie::from("session")),
        axum::response::Redirect::to("/login"),
    )
        .into_response()
}

pub async fn index(_: State<AppState>, jar: PrivateCookieJar) -> impl IntoResponse {
    if !is_authed(&jar) && !crate::PASSWORD.is_empty() {
        return axum::response::Redirect::to("/login").into_response();
    }
    Html(MAIN_HTML).into_response()
}
