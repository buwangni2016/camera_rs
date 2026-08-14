/*!
 * 云存储上传模块
 * 支持：OneDrive / Google Drive / FTP
 * OneDrive 与 Google Drive 均通过 Maton API Gateway 调用，无需手动管理 OAuth
 */

use serde::{Deserialize, Serialize};

const MATON_BASE: &str = "https://api.maton.ai";

// ============================================================
//  OneDrive 配置
// ============================================================
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OneDriveConfig {
    pub enabled:            bool,
    pub maton_api_key:      String,
    pub folder:             String,
    pub upload_photos:      bool,
    pub upload_motion:      bool,
    pub upload_videos:      bool,
    pub create_share_links: bool,
    pub share_folder_url:   String,
}
impl Default for OneDriveConfig {
    fn default() -> Self {
        Self { enabled: false, maton_api_key: String::new(), folder: "camera_rs".into(),
               upload_photos: true, upload_motion: true, upload_videos: false,
               create_share_links: true, share_folder_url: String::new() }
    }
}

// ============================================================
//  Google Drive 配置
// ============================================================
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GoogleDriveConfig {
    pub enabled:       bool,
    pub maton_api_key: String,
    pub folder_name:   String,   // Google Drive 文件夹名，不存在则创建
    pub folder_id:     String,   // 缓存已找到/创建的 folder ID
    pub upload_photos: bool,
    pub upload_motion: bool,
    pub upload_videos: bool,
}

// ============================================================
//  FTP/SFTP 配置
// ============================================================
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FtpConfig {
    pub enabled:       bool,
    pub host:          String,
    pub port:          u16,
    pub username:      String,
    pub password:      String,
    pub remote_dir:    String,   // 远端目录，如 /uploads/camera_rs
    pub passive_mode:  bool,
    pub upload_photos: bool,
    pub upload_motion: bool,
    pub upload_videos: bool,
}

// ============================================================
//  上传调度：根据文件类型分发到各存储
// ============================================================

pub async fn upload_all(
    od:       &OneDriveConfig,
    gd:       &GoogleDriveConfig,
    ftp:      &FtpConfig,
    filename: &str,
    data:     &[u8],
    kind:     UploadKind,
) -> Option<String> {
    let ct = if filename.ends_with(".avi") { "video/avi" }
             else if filename.ends_with(".mp4") { "video/mp4" }
             else { "image/jpeg" };

    let should_upload = |photos: bool, motion: bool, videos: bool| match kind {
        UploadKind::Photo  => photos,
        UploadKind::Motion => motion,
        UploadKind::Video  => videos,
    };

    let mut share_url = None;

    if od.enabled && should_upload(od.upload_photos, od.upload_motion, od.upload_videos) {
        share_url = upload_onedrive(od, filename, data, ct).await;
    }

    if gd.enabled && should_upload(gd.upload_photos, gd.upload_motion, gd.upload_videos) {
        upload_google_drive(gd, filename, data, ct).await;
    }

    if ftp.enabled && should_upload(ftp.upload_photos, ftp.upload_motion, ftp.upload_videos) {
        upload_ftp(ftp, filename, data).await;
    }

    share_url
}

#[derive(Clone, Copy)]
pub enum UploadKind { Photo, Motion, Video }

// ============================================================
//  OneDrive
// ============================================================

pub async fn upload_onedrive(
    cfg: &OneDriveConfig, filename: &str, data: &[u8], ct: &str,
) -> Option<String> {
    if cfg.maton_api_key.is_empty() { return None; }
    let folder = if cfg.folder.is_empty() { "camera_rs" } else { &cfg.folder };
    let path   = format!("{}/{}", folder, filename);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60)).build().ok()?;

    let resp = client
        .put(format!("{}/one-drive/v1.0/me/drive/root:/{}:/content", MATON_BASE, path))
        .header("Authorization", format!("Bearer {}", cfg.maton_api_key))
        .header("Content-Type", ct)
        .body(data.to_vec()).send().await.ok()?
        .json::<serde_json::Value>().await.ok()?;

    tracing::info!("OneDrive 上传: {}", filename);

    if !cfg.create_share_links { return None; }
    let item_id = resp["id"].as_str()?;
    let share = client
        .post(format!("{}/one-drive/v1.0/me/drive/items/{}/createLink", MATON_BASE, item_id))
        .header("Authorization", format!("Bearer {}", cfg.maton_api_key))
        .json(&serde_json::json!({"type":"view","scope":"anonymous"}))
        .send().await.ok()?.json::<serde_json::Value>().await.ok()?;

    share["link"]["webUrl"].as_str().map(String::from)
}

pub async fn create_folder_share(cfg: &OneDriveConfig) -> Option<String> {
    if cfg.maton_api_key.is_empty() { return None; }
    let folder = if cfg.folder.is_empty() { "camera_rs" } else { &cfg.folder };
    let client = reqwest::Client::new();

    let _ = client.put(format!("{}/one-drive/v1.0/me/drive/root:/{}/.keep:/content", MATON_BASE, folder))
        .header("Authorization", format!("Bearer {}", cfg.maton_api_key))
        .header("Content-Type", "text/plain").body("camera_rs").send().await;

    let item = client.get(format!("{}/one-drive/v1.0/me/drive/root:/{}", MATON_BASE, folder))
        .header("Authorization", format!("Bearer {}", cfg.maton_api_key))
        .send().await.ok()?.json::<serde_json::Value>().await.ok()?;

    let id = item["id"].as_str()?;
    let share = client.post(format!("{}/one-drive/v1.0/me/drive/items/{}/createLink", MATON_BASE, id))
        .header("Authorization", format!("Bearer {}", cfg.maton_api_key))
        .json(&serde_json::json!({"type":"view","scope":"anonymous"}))
        .send().await.ok()?.json::<serde_json::Value>().await.ok()?;

    share["link"]["webUrl"].as_str().map(String::from)
}

// ============================================================
//  Google Drive（通过 Maton）
// ============================================================

pub async fn upload_google_drive(cfg: &GoogleDriveConfig, filename: &str, data: &[u8], ct: &str) {
    if cfg.maton_api_key.is_empty() { return; }
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60)).build() {
        Ok(c) => c, Err(_) => return,
    };

    // 获取或创建目标文件夹 ID
    let folder_id = if !cfg.folder_id.is_empty() {
        cfg.folder_id.clone()
    } else {
        get_or_create_gdrive_folder(&client, &cfg.maton_api_key, &cfg.folder_name).await
            .unwrap_or_else(|| "root".to_string())
    };

    // 上传文件（multipart）
    let meta = serde_json::json!({"name": filename, "parents": [folder_id]});
    let meta_str = meta.to_string();
    let boundary = "boundary_camera_rs";
    let body = format!(
        "--{b}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{b}\r\nContent-Type: {ct}\r\n\r\n",
        b=boundary, meta=meta_str, ct=ct
    );
    let mut full_body = body.into_bytes();
    full_body.extend_from_slice(data);
    full_body.extend_from_slice(format!("\r\n--{}--", boundary).as_bytes());

    let url = format!(
        "{}/google-drive/upload/drive/v3/files?uploadType=multipart&fields=id,name",
        MATON_BASE
    );
    match client.post(&url)
        .header("Authorization", format!("Bearer {}", cfg.maton_api_key))
        .header("Content-Type", format!("multipart/related; boundary={}", boundary))
        .body(full_body).send().await
    {
        Ok(_) => tracing::info!("Google Drive 上传: {}", filename),
        Err(e) => tracing::warn!("Google Drive 上传失败: {}", e),
    }
}

async fn get_or_create_gdrive_folder(client: &reqwest::Client, api_key: &str, name: &str) -> Option<String> {
    if name.is_empty() { return Some("root".into()); }
    let q = format!("name='{}' and mimeType='application/vnd.google-apps.folder' and trashed=false", name);
    let resp = client.get(format!("{}/google-drive/drive/v3/files?q={}&fields=files(id)", MATON_BASE, urlencoding::encode(&q)))
        .header("Authorization", format!("Bearer {}", api_key))
        .send().await.ok()?.json::<serde_json::Value>().await.ok()?;

    if let Some(id) = resp["files"].get(0).and_then(|f| f["id"].as_str()) {
        return Some(id.to_string());
    }

    // 创建文件夹
    let create = client.post(format!("{}/google-drive/drive/v3/files", MATON_BASE))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({"name": name, "mimeType": "application/vnd.google-apps.folder"}))
        .send().await.ok()?.json::<serde_json::Value>().await.ok()?;

    create["id"].as_str().map(String::from)
}

// ============================================================
//  FTP 上传
// ============================================================

pub async fn upload_ftp(cfg: &FtpConfig, filename: &str, data: &[u8]) {
    if cfg.host.is_empty() { return; }
    let host = cfg.host.clone();
    let port = if cfg.port == 0 { 21 } else { cfg.port };
    let user = cfg.username.clone();
    let pass = cfg.password.clone();
    let dir  = cfg.remote_dir.clone();
    let fname = filename.to_string();
    let data_owned = data.to_vec();

    tokio::task::spawn_blocking(move || {
        use suppaftp::FtpStream;
        let addr = format!("{}:{}", host, port);
        let result = (|| -> anyhow::Result<()> {
            let mut ftp = FtpStream::connect(&addr)?;
            ftp.login(&user, &pass)?;
            if !dir.is_empty() {
                ftp.cwd(&dir).or_else(|_| { ftp.mkdir(&dir)?; ftp.cwd(&dir) })?;
            }
            let cursor = std::io::Cursor::new(data_owned);
            ftp.put_file(&fname, &mut std::io::BufReader::new(cursor))?;
            ftp.quit()?;
            Ok(())
        })();
        match result {
            Ok(_)  => tracing::info!("FTP 上传成功: {}", fname),
            Err(e) => tracing::warn!("FTP 上传失败: {}", e),
        }
    }).await.ok();
}

use base64::Engine;
