use serde::{Deserialize, Serialize};

// ============================================================
//  运行时状态持久化（runtime_state.json）
//
//  与 config.toml（启动参数）区分：
//    config.toml       — 主机/端口/摄像头索引/密码等启动配置，手工编辑
//    runtime_state.json — 运行期通过 UI 修改的一切配置，自动读写
// ============================================================

const RUNTIME_STATE_PATH: &str = "runtime_state.json";

/// 将当前运行时状态序列化为 JSON 并写入磁盘
pub fn save_runtime_state(state: &crate::state::AppState) {
    let snapshot = serde_json::json!({
        "notify":         *state.notify_cfg.lock(),
        "onedrive":       *state.onedrive_cfg.lock(),
        "gdrive":         *state.gdrive_cfg.lock(),
        "ftp":            *state.ftp_cfg.lock(),
        "schedule":       *state.schedule_rules.lock(),
        "alert_rule":     *state.alert_time_rule.lock(),
        "motion_zones":   *state.motion_zones.read(),
        "image_settings": *state.image_settings.read(),
        "watermark":      *state.watermark_cfg.lock(),
        "privacy_masks":  *state.privacy_masks.lock(),
        "rtsp_cameras":   *state.rtsp_cameras.lock(),
        "security":       *state.security.lock(),
        "record_limits":  *state.record_limits.lock(),
        "timelapse":      *state.timelapse_cfg.lock(),
        "email":          *state.email_cfg.lock(),
        "api_cfg":        *state.api_cfg.lock(),
    });
    match serde_json::to_string_pretty(&snapshot) {
        Ok(json) => {
            // 先写入临时文件再原子替换，避免写一半时崩溃损坏状态
            let tmp = format!("{}.tmp", RUNTIME_STATE_PATH);
            if std::fs::write(&tmp, &json).is_ok() {
                if let Err(e) = std::fs::rename(&tmp, RUNTIME_STATE_PATH) {
                    tracing::warn!("运行时状态持久化失败（rename）: {}", e);
                }
            }
        }
        Err(e) => tracing::warn!("运行时状态序列化失败: {}", e),
    }
}

/// 从磁盘加载运行时状态并应用到 AppState
pub fn load_runtime_state(state: &crate::state::AppState) {
    let content = match std::fs::read_to_string(RUNTIME_STATE_PATH) {
        Ok(s) => s,
        Err(_) => {
            tracing::info!("未找到 {} — 使用默认运行时配置", RUNTIME_STATE_PATH);
            return;
        }
    };
    let data: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("运行时状态文件解析失败: {} — 使用默认配置", e);
            return;
        }
    };

    macro_rules! apply {
        ($key:expr, $field:expr) => {
            if let Some(v) = data.get($key) {
                if let Ok(c) = serde_json::from_value(v.clone()) {
                    *$field = c;
                }
            }
        };
    }

    apply!("notify",         *state.notify_cfg.lock());
    apply!("onedrive",       *state.onedrive_cfg.lock());
    apply!("gdrive",         *state.gdrive_cfg.lock());
    apply!("ftp",            *state.ftp_cfg.lock());
    apply!("schedule",       *state.schedule_rules.lock());
    apply!("alert_rule",     *state.alert_time_rule.lock());
    apply!("motion_zones",   *state.motion_zones.write());
    apply!("image_settings", *state.image_settings.write());
    apply!("watermark",      *state.watermark_cfg.lock());
    apply!("privacy_masks",  *state.privacy_masks.lock());
    apply!("rtsp_cameras",   *state.rtsp_cameras.lock());
    apply!("security",       *state.security.lock());
    apply!("record_limits",  *state.record_limits.lock());
    apply!("timelapse",      *state.timelapse_cfg.lock());
    apply!("email",          *state.email_cfg.lock());
    apply!("api_cfg",        *state.api_cfg.lock());

    tracing::info!("运行时状态已从 {} 恢复", RUNTIME_STATE_PATH);
}

/// 启动一个后台任务，每 60 秒自动持久化一次运行时状态
pub async fn auto_persist_loop(state: crate::state::AppState) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        save_runtime_state(&state);
        tracing::debug!("运行时状态已自动持久化");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub camera: CameraConfig,
    pub security: SecurityConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraConfig {
    pub index: usize,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub password: String,
    pub max_login_attempts: u32,
    pub lockout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub save_dir: String,
    pub max_size_mb: u64,
    pub auto_cleanup: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server: ServerConfig { host: "0.0.0.0".into(), port: 5000 },
            camera: CameraConfig { index: 0, width: 1920, height: 1080 },
            security: SecurityConfig {
                password: "admin".into(),
                max_login_attempts: 5,
                lockout_secs: 900,
            },
            storage: StorageConfig {
                save_dir: "captures".into(),
                max_size_mb: 2048,
                auto_cleanup: true,
            },
        }
    }
}

impl Config {
    pub fn load() -> Self {
        match std::fs::read_to_string("config.toml") {
            Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
                tracing::warn!("配置文件解析失败: {} — 使用默认值", e);
                Config::default()
            }),
            Err(_) => {
                let cfg = Config::default();
                if let Ok(s) = toml::to_string_pretty(&cfg) {
                    std::fs::write("config.toml", s).ok();
                    tracing::info!("已生成默认配置文件 config.toml");
                }
                cfg
            }
        }
    }
}
