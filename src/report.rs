/*!
 * 每日统计报告模块
 * 每天定时生成摄像头统计摘要，通过邮件或通知渠道发送
 */

use chrono::Local;
use crate::state::AppState;
use crate::events::EventKind;

/// 生成每日摘要文本
pub fn build_daily_report(state: &AppState) -> String {
    let now   = Local::now();
    let today = now.format("%Y-%m-%d").to_string();

    let cam         = state.camera.lock();
    let motion_total = cam.motion_count;
    let fps         = cam.fps_current;
    let resolution  = format!("{}x{}", cam.resolution.0, cam.resolution.1);
    drop(cam);

    // 从事件日志统计今日数据
    let events = state.event_log.recent(500);
    let today_motion: usize = events.iter().filter(|e| {
        matches!(e.kind, EventKind::Motion) && e.timestamp.starts_with(&today)
    }).count();
    let today_photos: usize = events.iter().filter(|e| {
        matches!(e.kind, EventKind::Photo) && e.timestamp.starts_with(&today)
    }).count();
    let today_records: usize = events.iter().filter(|e| {
        matches!(e.kind, EventKind::RecordStop) && e.timestamp.starts_with(&today)
    }).count();

    // 存储占用
    let storage_mb = dir_size_mb(crate::SAVE_DIR);

    format!(
        "【每日监控报告】{}\n\
         ━━━━━━━━━━━━━━━━━━━━\n\
         摄像头分辨率: {}\n\
         当前帧率: {:.1} FPS\n\
         ━━━━━━━━━━━━━━━━━━━━\n\
         今日运动触发: {} 次\n\
         今日自动截图: {} 张\n\
         今日录像片段: {} 段\n\
         历史运动总次数: {}\n\
         ━━━━━━━━━━━━━━━━━━━━\n\
         存储已用: {:.1} MB\n\
         报告时间: {}",
        today,
        resolution, fps,
        today_motion, today_photos, today_records,
        motion_total,
        storage_mb,
        now.format("%Y-%m-%d %H:%M:%S"),
    )
}

fn dir_size_mb(dir: &str) -> f64 {
    fn walk(path: &std::path::Path) -> u64 {
        std::fs::read_dir(path).map(|rd| {
            rd.filter_map(|e| e.ok())
              .map(|e| {
                  let p = e.path();
                  if p.is_dir() { walk(&p) } else { p.metadata().map(|m| m.len()).unwrap_or(0) }
              })
              .sum()
        }).unwrap_or(0)
    }
    walk(std::path::Path::new(dir)) as f64 / 1_048_576.0
}

/// 发送每日报告（异步，通过所有已启用渠道）
pub async fn send_daily_report(state: &AppState) {
    let report = build_daily_report(state);
    tracing::info!("发送每日监控报告");

    // 邮件
    {
        let cfg = state.email_cfg.lock().clone();
        if cfg.enabled && !cfg.to.is_empty() {
            let r = report.clone();
            let _ = tokio::task::spawn_blocking(move || {
                send_report_email(&cfg, &r)
            }).await;
        }
    }

    // 多渠道通知
    let notify_cfg = state.notify_cfg.lock().clone();
    crate::notify::send_all(&notify_cfg, crate::notify::NotifyEvent::Custom {
        title: "每日监控报告",
        body:  &report,
    }).await;
}

fn send_report_email(cfg: &crate::state::EmailConfig, body: &str) -> anyhow::Result<()> {
    use lettre::{Message, SmtpTransport, Transport};
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::message::header::ContentType;

    let email = Message::builder()
        .from(cfg.from.parse()?)
        .to(cfg.to.parse()?)
        .subject(format!("每日监控报告 - {}", chrono::Local::now().format("%Y-%m-%d")))
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())?;

    let creds = Credentials::new(cfg.from.clone(), cfg.password.clone());
    let mailer = SmtpTransport::relay(&cfg.smtp_host)?
        .port(cfg.smtp_port)
        .credentials(creds)
        .build();

    mailer.send(&email)?;
    Ok(())
}
