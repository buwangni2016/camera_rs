/*!
 * 事件日志模块
 * 持久化记录所有监控事件（运动检测、录像、截图等）
 * 存储格式：JSON Lines，每行一条事件
 */

use chrono::Local;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const MAX_EVENTS_IN_MEMORY: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Motion,
    Photo,
    RecordStart,
    RecordStop,
    CameraSwitch,
    CameraOnline,
    CameraOffline,
    LoginSuccess,
    LoginFailed,
    AlertSent,
    UploadSuccess,
    SystemStart,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    pub timestamp: String, // ISO 8601
    pub kind: EventKind,
    pub camera_idx: usize,
    pub message: String,
    pub thumb_path: Option<String>, // 缩略图路径（相对于 SAVE_DIR）
    pub extra: Option<serde_json::Value>,
}

impl Event {
    pub fn new(id: u64, kind: EventKind, camera_idx: usize, message: impl Into<String>) -> Self {
        Self {
            id,
            timestamp: Local::now().to_rfc3339(),
            kind,
            camera_idx,
            message: message.into(),
            thumb_path: None,
            extra: None,
        }
    }
    pub fn with_thumb(mut self, path: impl Into<String>) -> Self {
        self.thumb_path = Some(path.into());
        self
    }
    pub fn with_extra(mut self, v: serde_json::Value) -> Self {
        self.extra = Some(v);
        self
    }
}

// ============================================================
//  EventLogger
// ============================================================

#[derive(Clone)]
pub struct EventLogger {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    log_path: PathBuf,
    events: Vec<Event>,
    next_id: u64,
}

impl EventLogger {
    pub fn new(save_dir: &str) -> Self {
        let log_path = PathBuf::from(save_dir).join("events.jsonl");
        let (events, next_id) = load_recent(&log_path, MAX_EVENTS_IN_MEMORY);
        Self {
            inner: Arc::new(Mutex::new(Inner {
                log_path,
                events,
                next_id,
            })),
        }
    }

    pub fn log(&self, mut event: Event) {
        let mut g = self.inner.lock().unwrap();
        event.id = g.next_id;
        g.next_id += 1;

        // 持久化到文件
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&g.log_path)
        {
            if let Ok(line) = serde_json::to_string(&event) {
                writeln!(f, "{}", line).ok();
            }
        }

        // 内存缓冲
        g.events.push(event);
        if g.events.len() > MAX_EVENTS_IN_MEMORY {
            g.events.drain(0..50);
        }
    }

    pub fn recent(&self, limit: usize) -> Vec<Event> {
        let g = self.inner.lock().unwrap();
        let start = g.events.len().saturating_sub(limit);
        g.events[start..].iter().rev().cloned().collect()
    }

    pub fn count(&self) -> usize {
        self.inner.lock().unwrap().events.len()
    }

    /// 按类型过滤
    pub fn by_kind(&self, kind_str: &str, limit: usize) -> Vec<Event> {
        let g = self.inner.lock().unwrap();
        g.events
            .iter()
            .rev()
            .filter(|e| format!("{:?}", e.kind).to_lowercase() == kind_str.to_lowercase())
            .take(limit)
            .cloned()
            .collect()
    }

    /// 清空日志文件
    pub fn clear(&self) {
        let mut g = self.inner.lock().unwrap();
        g.events.clear();
        std::fs::write(&g.log_path, "").ok();
    }
}

fn load_recent(path: &PathBuf, limit: usize) -> (Vec<Event>, u64) {
    let mut events = Vec::new();
    let mut max_id = 0u64;

    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines() {
            if let Ok(e) = serde_json::from_str::<Event>(line) {
                if e.id > max_id {
                    max_id = e.id;
                }
                events.push(e);
            }
        }
    }

    // 只保留最近的
    if events.len() > limit {
        let start = events.len() - limit;
        events = events[start..].to_vec();
    }

    (events, max_id + 1)
}
