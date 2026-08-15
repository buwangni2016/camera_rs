/*!
 * 定时任务模块
 * 支持：定时录像、定时运动侦测开关、定时截图
 * 时间规则：星期 + 时间范围
 */

use chrono::{Datelike, Local, Timelike, Weekday};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

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
    // 记录上一轮哪些规则处于活跃状态，用于检测"进入窗口"边沿
    let mut prev_active: std::collections::HashSet<usize> = std::collections::HashSet::new();
    loop {
        apply_schedules_async(&state, &mut prev_active).await;
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}

async fn apply_schedules_async(
    state: &crate::state::AppState,
    prev_active: &mut std::collections::HashSet<usize>,
) {
    let rules = state.schedule_rules.lock().clone();
    let mut cur_active = std::collections::HashSet::new();

    for (i, rule) in rules.iter().enumerate() {
        let active = rule.is_active_now();
        if active { cur_active.insert(i); }

        match rule.action {
            // 以下为"一次性触发"动作：只在刚进入激活窗口时执行一次
            ScheduleAction::DailyReport | ScheduleAction::ClearHeatmap | ScheduleAction::AutoUpload => {
                if !active || prev_active.contains(&i) { continue; }
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
                    _ => {}
                }
            }
            // StartRecord：进入窗口时开始，幂等
            ScheduleAction::StartRecord => {
                if !active { continue; }
                let mut cam = state.camera.lock();
                if !cam.recording {
                    cam.recording = true;
                    cam.record_start = Some(now_secs());
                    cam.record_frames.clear();
                    tracing::info!("定时任务「{}」: 已开始录像", rule.name);
                }
            }
            // StopRecord：离开窗口时停止并保存帧
            ScheduleAction::StopRecord => {
                // 当前不在窗口 且 上一轮在窗口 → 刚离开，保存录像
                if active || !prev_active.contains(&i) { continue; }
                let frames = {
                    let mut cam = state.camera.lock();
                    if cam.recording {
                        cam.recording = false;
                        Some(std::mem::take(&mut cam.record_frames))
                    } else { None }
                };
                if let Some(frames) = frames {
                    if !frames.is_empty() {
                        let path = format!("{}/videos/{}.avi", crate::SAVE_DIR,
                            chrono::Local::now().format("%Y%m%d_%H%M%S"));
                        tokio::task::spawn_blocking(move || {
                            crate::camera::save_mjpeg_avi(&frames, crate::RECORD_FPS, &path).ok();
                        });
                        tracing::info!("定时任务「{}」: 已停止录像并保存", rule.name);
                    }
                }
            }
            // 其余状态类动作：幂等，每轮都可执行
            _ => {
                if !active { continue; }
                apply_sync_rule(state, rule);
            }
        }
    }

    *prev_active = cur_active;
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
        // StartRecord/StopRecord/DailyReport/ClearHeatmap/AutoUpload 均在 apply_schedules_async 处理
        _ => {}
    }
}
