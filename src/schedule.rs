/*!
 * 定时任务模块
 * 支持：定时录像、定时运动侦测开关、定时截图
 * 时间规则：星期 + 时间范围
 */

use chrono::{Datelike, Local, Timelike, Weekday};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleRule {
    pub enabled:    bool,
    pub name:       String,
    /// 星期位掩码：bit0=周一 bit1=周二 ... bit6=周日，0xFF=每天
    pub weekdays:   u8,
    pub start_hhmm: u16,  // 格式：900 = 09:00
    pub end_hhmm:   u16,  // 格式：1800 = 18:00
    pub action:     ScheduleAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleAction {
    ArmMotion,      // 开启运动侦测
    DisarmMotion,   // 关闭运动侦测
    StartRecord,    // 开始录像
    StopRecord,     // 停止录像
    EnableNotify,   // 启用通知
    DisableNotify,  // 禁用通知（勿扰模式）
    DailyReport,    // 发送每日统计报告
    ClearHeatmap,   // 清空运动热力图
    AutoUpload,     // 立即触发云存储上传
}

impl ScheduleRule {
    /// 检查当前时间是否在本规则范围内
    pub fn is_active_now(&self) -> bool {
        if !self.enabled { return false; }
        let now = Local::now();
        let wd_bit = weekday_bit(now.weekday());
        if self.weekdays != 0xFF && (self.weekdays & wd_bit) == 0 { return false; }
        let current = now.hour() as u16 * 100 + now.minute() as u16;
        if self.start_hhmm <= self.end_hhmm {
            current >= self.start_hhmm && current < self.end_hhmm
        } else {
            // 跨午夜
            current >= self.start_hhmm || current < self.end_hhmm
        }
    }
}

pub fn weekday_bit_pub(wd: chrono::Weekday) -> u8 { weekday_bit(wd) }

fn weekday_bit(wd: Weekday) -> u8 {
    match wd {
        Weekday::Mon => 0b0000001,
        Weekday::Tue => 0b0000010,
        Weekday::Wed => 0b0000100,
        Weekday::Thu => 0b0001000,
        Weekday::Fri => 0b0010000,
        Weekday::Sat => 0b0100000,
        Weekday::Sun => 0b1000000,
    }
}

/// 后台定时任务循环（每分钟执行一次）
pub async fn schedule_loop(state: crate::state::AppState) {
    loop {
        apply_schedules_async(&state).await;
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}

async fn apply_schedules_async(state: &crate::state::AppState) {
    let rules = state.schedule_rules.lock().clone();
    for rule in &rules {
        if !rule.is_active_now() { continue; }
        match rule.action {
            ScheduleAction::DailyReport => {
                let s = state.clone();
                tokio::spawn(async move { crate::report::send_daily_report(&s).await });
            }
            ScheduleAction::ClearHeatmap => {
                crate::heatmap::clear_heatmap(state);
                tracing::info!("定时任务「{}」: 已清空热力图", rule.name);
            }
            ScheduleAction::AutoUpload => {
                let jpeg = state.camera.lock().latest_jpeg.clone();
                if let Some(j) = jpeg {
                    let od = state.onedrive_cfg.lock().clone();
                    let gd = state.gdrive_cfg.lock().clone();
                    let ft = state.ftp_cfg.lock().clone();
                    let fname = format!("auto/{}.jpg", chrono::Local::now().format("%Y%m%d_%H%M%S"));
                    tokio::spawn(async move {
                        crate::upload::upload_all(&od, &gd, &ft, &fname, &j, crate::upload::UploadKind::Photo).await;
                    });
                }
            }
            _ => apply_sync_rule(state, rule),
        }
    }
}

fn apply_sync_rule(state: &crate::state::AppState, rule: &ScheduleRule) {
    match rule.action {
        ScheduleAction::ArmMotion => {
            let mut cam = state.camera.lock();
            if !cam.motion_detect {
                cam.motion_detect = true;
                cam.prev_gray = None;
                tracing::info!("定时任务「{}」: 已开启运动侦测", rule.name);
            }
        }
        ScheduleAction::DisarmMotion => {
            let mut cam = state.camera.lock();
            if cam.motion_detect {
                cam.motion_detect = false;
                tracing::info!("定时任务「{}」: 已关闭运动侦测", rule.name);
            }
        }
        ScheduleAction::DisableNotify => {
            state.notify_suppressed.store(true, Ordering::Relaxed);
        }
        ScheduleAction::EnableNotify => {
            state.notify_suppressed.store(false, Ordering::Relaxed);
        }
        ScheduleAction::StartRecord => {
            let mut cam = state.camera.lock();
            if !cam.recording {
                cam.recording = true;
                tracing::info!("定时任务「{}」: 已开始录像", rule.name);
            }
        }
        ScheduleAction::StopRecord => {
            let mut cam = state.camera.lock();
            if cam.recording {
                cam.recording = false;
                tracing::info!("定时任务「{}」: 已停止录像", rule.name);
            }
        }
        // Async actions (DailyReport, ClearHeatmap, AutoUpload) handled in apply_schedules_async
        _ => {}
    }
}
