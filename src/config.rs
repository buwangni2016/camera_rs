use serde::{Deserialize, Serialize};

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
