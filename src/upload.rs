/*!
 * OneDrive 文件上传模块
 * 通过 Maton API Gateway 调用 Microsoft Graph API，无需手动管理 OAuth Token
 *
 * 使用方法：
 * 1. 在 https://maton.ai 连接 OneDrive 账户
 * 2. 在设置页填入 Maton API Key
 * 3. 程序启动后自动上传截图/录像到指定文件夹
 */

use serde::{Deserialize, Serialize};

const GRAPH_BASE: &str = "https://api.maton.ai/one-drive/v1.0/me/drive";

// ============================================================
//  OneDrive 配置
// ============================================================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OneDriveConfig {
    pub enabled:             bool,
    pub maton_api_key:       String,
    pub folder:              String,   // OneDrive 中的目标文件夹，默认 "camera_rs"
    pub upload_photos:       bool,     // 上传手动截图
    pub upload_motion:       bool,     // 上传运动侦测截图
    pub upload_videos:       bool,     // 上传录像
    pub create_share_links:  bool,     // 生成公开分享链接（用于通知中展示图片）
    pub share_folder_url:    String,   // 文件夹公开分享链接（供 Vercel Viewer 使用）
}

impl Default for OneDriveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            maton_api_key: String::new(),
            folder: "camera_rs".into(),
            upload_photos: true,
            upload_motion: true,
            upload_videos: false,
            create_share_links: true,
            share_folder_url: String::new(),
        }
    }
}

// ============================================================
//  上传单个文件，返回公开分享链接（如果配置了）
// ============================================================

pub async fn upload_file(
    cfg: &OneDriveConfig,
    filename: &str,
    data: &[u8],
    content_type: &str,
) -> Option<String> {
    if !cfg.enabled || cfg.maton_api_key.is_empty() { return None; }

    let folder = if cfg.folder.is_empty() { "camera_rs" } else { &cfg.folder };
    let path   = format!("{}/{}", folder, filename);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build().ok()?;

    // 上传文件（使用 OneDrive 简单上传 API，支持最大 4MB）
    let upload_url = format!("{}/root:/{}:/content", GRAPH_BASE, path);
    let resp = client.put(&upload_url)
        .header("Authorization", format!("Bearer {}", cfg.maton_api_key))
        .header("Content-Type", content_type)
        .body(data.to_vec())
        .send().await.ok()?
        .json::<serde_json::Value>().await.ok()?;

    tracing::info!("OneDrive 上传成功: {}", filename);

    // 生成分享链接
    if !cfg.create_share_links { return None; }
    let item_id = resp["id"].as_str()?;
    let share_resp = client.post(
        format!("{}/items/{}/createLink", GRAPH_BASE, item_id))
        .header("Authorization", format!("Bearer {}", cfg.maton_api_key))
        .json(&serde_json::json!({"type": "view", "scope": "anonymous"}))
        .send().await.ok()?
        .json::<serde_json::Value>().await.ok()?;

    let url = share_resp["link"]["webUrl"].as_str().map(String::from)?;

    // 转换为直链（DownloadURL）
    // OneDrive 分享链接转直链格式：将 redir 链接替换
    Some(to_direct_url(&url))
}

/// 尝试将 OneDrive 分享链接转为直接可预览的链接
fn to_direct_url(share_url: &str) -> String {
    // 将 1drv.ms 或 onedrive.live.com 链接转为 embed 格式
    if share_url.contains("1drv.ms") || share_url.contains("onedrive.live.com") {
        let encoded = base64::engine::general_purpose::STANDARD_NO_PAD
            .encode(share_url.as_bytes());
        // 替换非标准字符
        let safe = encoded.replace('+', "-").replace('/', "_");
        format!("https://api.onedrive.com/v1.0/shares/u!{}/driveItem/content", safe)
    } else {
        share_url.to_string()
    }
}

// ============================================================
//  为文件夹创建公开分享链接（供 Vercel Viewer 使用）
// ============================================================

pub async fn create_folder_share(cfg: &OneDriveConfig) -> Option<String> {
    if !cfg.enabled || cfg.maton_api_key.is_empty() { return None; }

    let folder = if cfg.folder.is_empty() { "camera_rs" } else { &cfg.folder };
    let client = reqwest::Client::new();

    // 先确保文件夹存在
    let _ = client.put(format!("{}/root:/{}:/content", GRAPH_BASE, format!("{}/.keep", folder)))
        .header("Authorization", format!("Bearer {}", cfg.maton_api_key))
        .header("Content-Type", "text/plain")
        .body("camera_rs")
        .send().await;

    // 获取文件夹 item
    let item_resp = client.get(format!("{}/root:/{}", GRAPH_BASE, folder))
        .header("Authorization", format!("Bearer {}", cfg.maton_api_key))
        .send().await.ok()?
        .json::<serde_json::Value>().await.ok()?;

    let item_id = item_resp["id"].as_str()?;

    // 创建文件夹分享链接
    let share_resp = client.post(format!("{}/items/{}/createLink", GRAPH_BASE, item_id))
        .header("Authorization", format!("Bearer {}", cfg.maton_api_key))
        .json(&serde_json::json!({"type": "view", "scope": "anonymous"}))
        .send().await.ok()?
        .json::<serde_json::Value>().await.ok()?;

    share_resp["link"]["webUrl"].as_str().map(String::from)
}

use base64::Engine;
