/*!
 * 多渠道通知模块
 * 支持：Telegram / 钉钉 / 企业微信 / Server酱 / Bark / PushPlus / 通用 Webhook
 */

use serde::{Deserialize, Serialize};

// ============================================================
//  各渠道配置结构体
// ============================================================

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NotifyConfig {
    pub telegram: TelegramCfg,
    pub dingtalk: DingTalkCfg,
    pub wecom: WeComCfg,
    pub serverchan: ServerChanCfg,
    pub bark: BarkCfg,
    pub pushplus: PushPlusCfg,
    pub webhook: WebhookCfg,
    pub discord: DiscordCfg,
    pub slack: SlackCfg,
    pub twilio: TwilioCfg,
    pub ntfy: NtfyCfg,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TelegramCfg {
    pub enabled: bool,
    pub bot_token: String,
    pub chat_id: String,
    pub send_photo: bool, // true=发图片，false=仅文字
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DingTalkCfg {
    pub enabled: bool,
    pub webhook_url: String,
    // 安全模式：加签密钥（空=不加签）
    pub secret: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WeComCfg {
    pub enabled: bool,
    pub webhook_url: String, // 企业微信机器人 Webhook URL
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ServerChanCfg {
    pub enabled: bool,
    pub send_key: String, // Server酱 SendKey
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BarkCfg {
    pub enabled: bool,
    pub server_url: String, // https://api.day.app/{key} 或自建服务器
    pub sound: String,      // 通知音效，留空=默认
    pub group: String,      // 通知分组
    pub icon: String,       // 自定义图标 URL
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PushPlusCfg {
    pub enabled: bool,
    pub token: String,
    pub topic: String, // 群组推送 topic，留空=仅推给自己
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WebhookCfg {
    pub enabled: bool,
    pub url: String,
    // POST body 模板，支持占位符：{event} {count} {image_url} {timestamp}
    pub body_template: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiscordCfg {
    pub enabled: bool,
    pub webhook_url: String, // Discord Webhook URL
    pub username: String,    // 机器人显示名称（留空用默认）
    pub avatar_url: String,  // 机器人头像 URL（留空用默认）
    pub send_image: bool,    // 是否上传图片附件
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SlackCfg {
    pub enabled: bool,
    pub webhook_url: String, // Slack Incoming Webhook URL
    pub channel: String,     // 频道（留空用 Webhook 默认）
    pub username: String,    // 机器人名称（留空用默认）
    pub icon_emoji: String,  // 图标 emoji，如 :camera:
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TwilioCfg {
    pub enabled: bool,
    pub account_sid: String,
    pub auth_token: String,
    pub from_number: String, // +1234567890
    pub to_numbers: String,  // 逗号分隔，支持多个号码
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NtfyCfg {
    pub enabled: bool,
    pub server_url: String, // https://ntfy.sh 或自建
    pub topic: String,      // 推送话题
    pub priority: String,   // default / high / urgent
    pub tags: String,       // 逗号分隔标签，如 warning,camera
}

// ============================================================
//  事件类型
// ============================================================

pub enum NotifyEvent<'a> {
    Motion {
        count: u64,
        image: &'a [u8],
        image_url: Option<&'a str>,
    },
    Recording {
        started: bool,
    },
    CameraOnline {
        index: usize,
    },
    CameraOffline {
        index: usize,
    },
    Custom {
        title: &'a str,
        body: &'a str,
    },
}

// ============================================================
//  统一发送入口
// ============================================================

pub async fn send_all(cfg: &NotifyConfig, event: NotifyEvent<'_>) {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(_) => return,
    };

    let (title, body, image, image_url) = match &event {
        NotifyEvent::Motion {
            count,
            image,
            image_url,
        } => (
            format!("⚠️ 运动侦测告警"),
            format!(
                "检测到移动，累计触发 {} 次{}",
                count,
                image_url
                    .map(|u| format!("\n📷 [查看截图]({})", u))
                    .unwrap_or_default()
            ),
            *image,
            *image_url,
        ),
        NotifyEvent::Recording { started } => (
            if *started {
                "🎥 开始录像".into()
            } else {
                "💾 录像已保存".into()
            },
            if *started {
                "摄像头开始录像".into()
            } else {
                "录像文件已保存".into()
            },
            &[] as &[u8],
            None,
        ),
        NotifyEvent::CameraOnline { index } => (
            format!("📷 摄像头上线"),
            format!("摄像头 {} 已连接就绪", index),
            &[] as &[u8],
            None,
        ),
        NotifyEvent::CameraOffline { index } => (
            format!("❌ 摄像头掉线"),
            format!("摄像头 {} 断开连接", index),
            &[] as &[u8],
            None,
        ),
        NotifyEvent::Custom { title, body } => {
            (title.to_string(), body.to_string(), &[] as &[u8], None)
        }
    };

    let mut handles = Vec::new();

    if cfg.telegram.enabled && !cfg.telegram.bot_token.is_empty() {
        let c = client.clone();
        let cfg = cfg.telegram.clone();
        let t = title.clone();
        let b = body.clone();
        let img = image.to_vec();
        handles.push(tokio::spawn(async move {
            if let Err(e) = send_telegram(&c, &cfg, &img, &t, &b).await {
                tracing::warn!("Telegram 通知失败: {}", e);
            }
        }));
    }

    if cfg.dingtalk.enabled && !cfg.dingtalk.webhook_url.is_empty() {
        let c = client.clone();
        let cfg = cfg.dingtalk.clone();
        let t = title.clone();
        let b = body.clone();
        let iu = image_url.map(String::from);
        handles.push(tokio::spawn(async move {
            if let Err(e) = send_dingtalk(&c, &cfg, &t, &b, iu.as_deref()).await {
                tracing::warn!("钉钉通知失败: {}", e);
            }
        }));
    }

    if cfg.wecom.enabled && !cfg.wecom.webhook_url.is_empty() {
        let c = client.clone();
        let cfg = cfg.wecom.clone();
        let t = title.clone();
        let b = body.clone();
        let iu = image_url.map(String::from);
        handles.push(tokio::spawn(async move {
            if let Err(e) = send_wecom(&c, &cfg, &t, &b, iu.as_deref()).await {
                tracing::warn!("企业微信通知失败: {}", e);
            }
        }));
    }

    if cfg.serverchan.enabled && !cfg.serverchan.send_key.is_empty() {
        let c = client.clone();
        let cfg = cfg.serverchan.clone();
        let t = title.clone();
        let b = body.clone();
        handles.push(tokio::spawn(async move {
            if let Err(e) = send_serverchan(&c, &cfg, &t, &b).await {
                tracing::warn!("Server酱通知失败: {}", e);
            }
        }));
    }

    if cfg.bark.enabled && !cfg.bark.server_url.is_empty() {
        let c = client.clone();
        let cfg = cfg.bark.clone();
        let t = title.clone();
        let b = body.clone();
        handles.push(tokio::spawn(async move {
            if let Err(e) = send_bark(&c, &cfg, &t, &b).await {
                tracing::warn!("Bark 通知失败: {}", e);
            }
        }));
    }

    if cfg.pushplus.enabled && !cfg.pushplus.token.is_empty() {
        let c = client.clone();
        let cfg = cfg.pushplus.clone();
        let t = title.clone();
        let b = body.clone();
        let iu = image_url.map(String::from);
        handles.push(tokio::spawn(async move {
            if let Err(e) = send_pushplus(&c, &cfg, &t, &b, iu.as_deref()).await {
                tracing::warn!("PushPlus 通知失败: {}", e);
            }
        }));
    }

    if cfg.webhook.enabled && !cfg.webhook.url.is_empty() {
        let c = client.clone();
        let cfg = cfg.webhook.clone();
        let t = title.clone();
        let b = body.clone();
        let iu = image_url.map(String::from);
        handles.push(tokio::spawn(async move {
            if let Err(e) = send_webhook(&c, &cfg, &t, &b, iu.as_deref()).await {
                tracing::warn!("Webhook 通知失败: {}", e);
            }
        }));
    }

    if cfg.discord.enabled && !cfg.discord.webhook_url.is_empty() {
        let c = client.clone();
        let cfg = cfg.discord.clone();
        let t = title.clone();
        let b = body.clone();
        let img = image.to_vec();
        handles.push(tokio::spawn(async move {
            if let Err(e) = send_discord(&c, &cfg, &img, &t, &b).await {
                tracing::warn!("Discord 通知失败: {}", e);
            }
        }));
    }

    if cfg.slack.enabled && !cfg.slack.webhook_url.is_empty() {
        let c = client.clone();
        let cfg = cfg.slack.clone();
        let t = title.clone();
        let b = body.clone();
        let iu = image_url.map(String::from);
        handles.push(tokio::spawn(async move {
            if let Err(e) = send_slack(&c, &cfg, &t, &b, iu.as_deref()).await {
                tracing::warn!("Slack 通知失败: {}", e);
            }
        }));
    }

    if cfg.twilio.enabled && !cfg.twilio.account_sid.is_empty() {
        let c = client.clone();
        let cfg = cfg.twilio.clone();
        let t = title.clone();
        let b = body.clone();
        handles.push(tokio::spawn(async move {
            if let Err(e) = send_twilio_sms(&c, &cfg, &t, &b).await {
                tracing::warn!("Twilio SMS 通知失败: {}", e);
            }
        }));
    }

    if cfg.ntfy.enabled && !cfg.ntfy.topic.is_empty() {
        let c = client.clone();
        let cfg = cfg.ntfy.clone();
        let t = title.clone();
        let b = body.clone();
        let img = image.to_vec();
        handles.push(tokio::spawn(async move {
            if let Err(e) = send_ntfy(&c, &cfg, &img, &t, &b).await {
                tracing::warn!("ntfy 通知失败: {}", e);
            }
        }));
    }

    for h in handles {
        h.await.ok();
    }
}

// ============================================================
//  Telegram
// ============================================================

async fn send_telegram(
    client: &reqwest::Client,
    cfg: &TelegramCfg,
    image: &[u8],
    title: &str,
    body: &str,
) -> anyhow::Result<()> {
    let caption = format!("{}\n{}", title, body);
    if cfg.send_photo && !image.is_empty() {
        let form = reqwest::multipart::Form::new()
            .text("chat_id", cfg.chat_id.clone())
            .text("caption", caption)
            .text("parse_mode", "Markdown")
            .part(
                "photo",
                reqwest::multipart::Part::bytes(image.to_vec())
                    .file_name("capture.jpg")
                    .mime_str("image/jpeg")?,
            );
        client
            .post(format!(
                "https://api.telegram.org/bot{}/sendPhoto",
                cfg.bot_token
            ))
            .multipart(form)
            .send()
            .await?;
    } else {
        client
            .post(format!(
                "https://api.telegram.org/bot{}/sendMessage",
                cfg.bot_token
            ))
            .json(&serde_json::json!({
                "chat_id": cfg.chat_id,
                "text": caption,
                "parse_mode": "Markdown"
            }))
            .send()
            .await?;
    }
    Ok(())
}

// ============================================================
//  钉钉
// ============================================================

async fn send_dingtalk(
    client: &reqwest::Client,
    cfg: &DingTalkCfg,
    title: &str,
    body: &str,
    image_url: Option<&str>,
) -> anyhow::Result<()> {
    let text = if let Some(url) = image_url {
        format!("## {}\n{}\n\n![截图]({})", title, body, url)
    } else {
        format!("## {}\n{}", title, body)
    };

    let mut url = cfg.webhook_url.clone();

    // 加签（如果配置了密钥）
    if !cfg.secret.is_empty() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let sign = dingtalk_sign(ts, &cfg.secret);
        url = format!("{}&timestamp={}&sign={}", url, ts, sign);
    }

    let msg_type = if image_url.is_some() {
        "markdown"
    } else {
        "text"
    };
    let body = if msg_type == "markdown" {
        serde_json::json!({"msgtype":"markdown","markdown":{"title":title,"text":text}})
    } else {
        serde_json::json!({"msgtype":"text","text":{"content":format!("{}\n{}",title,body)}})
    };

    client.post(&url).json(&body).send().await?;
    Ok(())
}

fn dingtalk_sign(timestamp: u64, secret: &str) -> String {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let str_to_sign = format!("{}\n{}", timestamp, secret);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC error");
    mac.update(str_to_sign.as_bytes());
    let result = mac.finalize().into_bytes();
    let encoded = base64::engine::general_purpose::STANDARD.encode(result);
    urlencoding::encode(&encoded).to_string()
}

// ============================================================
//  企业微信
// ============================================================

async fn send_wecom(
    client: &reqwest::Client,
    cfg: &WeComCfg,
    title: &str,
    body: &str,
    image_url: Option<&str>,
) -> anyhow::Result<()> {
    let content = if let Some(url) = image_url {
        format!("**{}**\n>{}\n[查看截图]({})", title, body, url)
    } else {
        format!("**{}**\n>{}", title, body)
    };

    client
        .post(&cfg.webhook_url)
        .json(&serde_json::json!({
            "msgtype": "markdown",
            "markdown": { "content": content }
        }))
        .send()
        .await?;
    Ok(())
}

// ============================================================
//  Server酱
// ============================================================

async fn send_serverchan(
    client: &reqwest::Client,
    cfg: &ServerChanCfg,
    title: &str,
    body: &str,
) -> anyhow::Result<()> {
    client
        .post(format!("https://sctapi.ftqq.com/{}.send", cfg.send_key))
        .form(&[("title", title), ("desp", body)])
        .send()
        .await?;
    Ok(())
}

// ============================================================
//  Bark (iOS)
// ============================================================

async fn send_bark(
    client: &reqwest::Client,
    cfg: &BarkCfg,
    title: &str,
    body: &str,
) -> anyhow::Result<()> {
    let mut payload = serde_json::json!({
        "title": title,
        "body": body,
        "url": cfg.server_url,
    });
    if !cfg.sound.is_empty() {
        payload["sound"] = cfg.sound.clone().into();
    }
    if !cfg.group.is_empty() {
        payload["group"] = cfg.group.clone().into();
    }
    if !cfg.icon.is_empty() {
        payload["icon"] = cfg.icon.clone().into();
    }

    // Bark server URL 格式: https://api.day.app/{key}
    let url = format!("{}/push", cfg.server_url.trim_end_matches('/'));
    client.post(&url).json(&payload).send().await?;
    Ok(())
}

// ============================================================
//  PushPlus
// ============================================================

async fn send_pushplus(
    client: &reqwest::Client,
    cfg: &PushPlusCfg,
    title: &str,
    body: &str,
    image_url: Option<&str>,
) -> anyhow::Result<()> {
    let content = if let Some(url) = image_url {
        format!("{}<br><img src='{}'>", body, url)
    } else {
        body.to_string()
    };

    let mut payload = serde_json::json!({
        "token":   cfg.token,
        "title":   title,
        "content": content,
        "template":"html",
    });
    if !cfg.topic.is_empty() {
        payload["topic"] = cfg.topic.clone().into();
    }

    client
        .post("https://www.pushplus.plus/send")
        .json(&payload)
        .send()
        .await?;
    Ok(())
}

// ============================================================
//  通用 Webhook
// ============================================================

async fn send_webhook(
    client: &reqwest::Client,
    cfg: &WebhookCfg,
    title: &str,
    body: &str,
    image_url: Option<&str>,
) -> anyhow::Result<()> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();

    let payload = if cfg.body_template.is_empty() {
        serde_json::json!({
            "event":     title,
            "message":   body,
            "image_url": image_url.unwrap_or(""),
            "timestamp": ts,
        })
    } else {
        let rendered = cfg
            .body_template
            .replace("{event}", title)
            .replace("{message}", body)
            .replace("{image_url}", image_url.unwrap_or(""))
            .replace("{timestamp}", &ts);
        serde_json::from_str(&rendered).unwrap_or_else(|_| serde_json::json!({"raw": rendered}))
    };

    client.post(&cfg.url).json(&payload).send().await?;
    Ok(())
}

// ============================================================
//  Discord
// ============================================================

async fn send_discord(
    client: &reqwest::Client,
    cfg: &DiscordCfg,
    image: &[u8],
    title: &str,
    body: &str,
) -> anyhow::Result<()> {
    if cfg.send_image && !image.is_empty() {
        // 上传图片附件 + embed
        let embed = serde_json::json!({
            "title": title,
            "description": body,
            "color": 15158332,
            "image": {"url": "attachment://capture.jpg"},
        });
        let payload_json = serde_json::json!({
            "username": if cfg.username.is_empty() { "Camera RS" } else { &cfg.username },
            "embeds": [embed],
        });
        let form = reqwest::multipart::Form::new()
            .text("payload_json", payload_json.to_string())
            .part(
                "files[0]",
                reqwest::multipart::Part::bytes(image.to_vec())
                    .file_name("capture.jpg")
                    .mime_str("image/jpeg")?,
            );
        client.post(&cfg.webhook_url).multipart(form).send().await?;
    } else {
        let mut payload = serde_json::json!({
            "embeds": [{
                "title": title,
                "description": body,
                "color": 15158332,
            }]
        });
        if !cfg.username.is_empty() {
            payload["username"] = cfg.username.clone().into();
        }
        if !cfg.avatar_url.is_empty() {
            payload["avatar_url"] = cfg.avatar_url.clone().into();
        }
        client.post(&cfg.webhook_url).json(&payload).send().await?;
    }
    Ok(())
}

// ============================================================
//  Slack
// ============================================================

async fn send_slack(
    client: &reqwest::Client,
    cfg: &SlackCfg,
    title: &str,
    body: &str,
    image_url: Option<&str>,
) -> anyhow::Result<()> {
    let text = format!("*{}*\n{}", title, body);
    let mut blocks =
        vec![serde_json::json!({"type":"section","text":{"type":"mrkdwn","text":text}})];
    if let Some(url) = image_url {
        blocks.push(serde_json::json!({
            "type": "image",
            "image_url": url,
            "alt_text": title,
        }));
    }

    let mut payload = serde_json::json!({"blocks": blocks});
    if !cfg.channel.is_empty() {
        payload["channel"] = cfg.channel.clone().into();
    }
    if !cfg.username.is_empty() {
        payload["username"] = cfg.username.clone().into();
    }
    if !cfg.icon_emoji.is_empty() {
        payload["icon_emoji"] = cfg.icon_emoji.clone().into();
    }

    client.post(&cfg.webhook_url).json(&payload).send().await?;
    Ok(())
}

// ============================================================
//  Twilio SMS
// ============================================================

async fn send_twilio_sms(
    client: &reqwest::Client,
    cfg: &TwilioCfg,
    title: &str,
    body: &str,
) -> anyhow::Result<()> {
    let message = format!("{}: {}", title, body);
    let url = format!(
        "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
        cfg.account_sid
    );
    for number in cfg
        .to_numbers
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        client
            .post(&url)
            .basic_auth(&cfg.account_sid, Some(&cfg.auth_token))
            .form(&[
                ("From", cfg.from_number.as_str()),
                ("To", number),
                ("Body", message.as_str()),
            ])
            .send()
            .await?;
    }
    Ok(())
}

// ============================================================
//  ntfy (self-hosted / ntfy.sh)
// ============================================================

async fn send_ntfy(
    client: &reqwest::Client,
    cfg: &NtfyCfg,
    image: &[u8],
    title: &str,
    body: &str,
) -> anyhow::Result<()> {
    let base = cfg.server_url.trim_end_matches('/');
    let url = format!("{}/{}", base, cfg.topic);

    let mut req = client
        .post(&url)
        .header("Title", title)
        .header("Message", body);

    if !cfg.priority.is_empty() {
        req = req.header("Priority", &cfg.priority);
    }
    if !cfg.tags.is_empty() {
        req = req.header("Tags", &cfg.tags);
    }

    if !image.is_empty() {
        req = req
            .header("Content-Type", "image/jpeg")
            .body(image.to_vec());
    }

    req.send().await?;
    Ok(())
}
