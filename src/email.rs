/*!
 * 邮件告警模块（lettre 0.11）
 */

use std::time::{SystemTime, UNIX_EPOCH};
use lettre::{
    Message, SmtpTransport, Transport,
    message::{header::ContentType, Attachment, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
};
use crate::state::{AppState, EmailConfig};

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn send_impl(cfg: &EmailConfig, subject: &str, body: &str, image_path: Option<&str>) -> Result<(), String> {
    if !cfg.enabled || cfg.from.is_empty() || cfg.password.is_empty() {
        return Err("邮件未配置".into());
    }
    let email = if let Some(path) = image_path {
        if let Ok(img_data) = std::fs::read(path) {
            Message::builder()
                .from(cfg.from.parse().map_err(|e: lettre::address::AddressError| e.to_string())?)
                .to(cfg.to.parse().map_err(|e: lettre::address::AddressError| e.to_string())?)
                .subject(subject)
                .multipart(
                    MultiPart::mixed()
                        .singlepart(SinglePart::plain(body.to_string()))
                        .singlepart(
                            Attachment::new("alert.jpg".to_string())
                                .body(img_data, ContentType::parse("image/jpeg").unwrap()),
                        ),
                )
                .map_err(|e| e.to_string())?
        } else {
            build_plain(cfg, subject, body)?
        }
    } else {
        build_plain(cfg, subject, body)?
    };

    let creds = Credentials::new(cfg.from.clone(), cfg.password.clone());
    let mailer = SmtpTransport::relay(&cfg.smtp_host)
        .map_err(|e| e.to_string())?
        .credentials(creds)
        .build();

    mailer.send(&email).map_err(|e| e.to_string())?;
    Ok(())
}

fn build_plain(cfg: &EmailConfig, subject: &str, body: &str) -> Result<Message, String> {
    Message::builder()
        .from(cfg.from.parse().map_err(|e: lettre::address::AddressError| e.to_string())?)
        .to(cfg.to.parse().map_err(|e: lettre::address::AddressError| e.to_string())?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| e.to_string())
}

/// 从摄像头线程（同步）调用的运动告警。
/// 调用方负责冷却检测和更新全局 last_sent，本函数直接发送。
pub fn send_motion_alert_direct(cfg: &EmailConfig, count: u64, image_path: &str) -> Result<(), String> {
    let body = format!(
        "时间: {}\n已触发第 {} 次移动侦测",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        count
    );
    send_impl(cfg, "移动侦测告警", &body, Some(image_path))
}

/// 保留旧接口以兼容直接调用（自带冷却检查，但 last_sent 只更新本地副本）
pub fn send_motion_alert_blocking(
    mut cfg: EmailConfig,
    count: u64,
    image_path: &str,
) -> Result<(), String> {
    if now_secs() - cfg.last_sent < cfg.cooldown {
        return Err("冷却中".into());
    }
    cfg.last_sent = now_secs();
    send_motion_alert_direct(&cfg, count, image_path)
}

/// 从 Web 处理器调用（更新全局冷却时间）
pub fn send_motion_alert(state: &AppState, count: u64, image_path: &str) {
    let cfg = state.email_cfg.lock().clone();
    if let Err(e) = send_motion_alert_blocking(cfg, count, image_path) {
        if e != "冷却中" && e != "邮件未配置" {
            tracing::error!("邮件发送失败: {}", e);
        }
    } else {
        state.email_cfg.lock().last_sent = now_secs();
    }
}

/// 测试邮件（重置冷却）
pub fn send_test(state: &AppState) -> Result<(), String> {
    state.email_cfg.lock().last_sent = 0;
    let cfg = state.email_cfg.lock().clone();
    let body = format!(
        "这是一封测试邮件。\n时间: {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    send_impl(&cfg, "测试邮件 - 监控系统", &body, None)
}
