# 部署指南

## 方式 1：直接运行（Windows/Linux）

```bash
# 编译
cargo build --release

# 运行
./target/release/camera_rs        # Linux
target\release\camera_rs.exe      # Windows
```

访问 http://localhost:5000，默认密码 `admin`

---

## 方式 2：systemd 服务（Linux 长期运行）

```bash
# 1. 修改 service 文件中的用户名和路径
nano deploy/camera_rs.service

# 2. 安装服务
sudo cp deploy/camera_rs.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now camera_rs

# 3. 查看状态
sudo systemctl status camera_rs
sudo journalctl -u camera_rs -f
```

---

## 方式 3：Docker

```bash
# 构建并启动
docker-compose up -d

# 查看日志
docker-compose logs -f

# 停止
docker-compose down
```

**注意**：`docker-compose.yml` 默认挂载 `/dev/video0` 和 `/dev/video1`，根据实际情况修改。

---

## Cloudflare Tunnel（公网访问，无需公网 IP）

```bash
# 安装 cloudflared
# Linux: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/
curl -L https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64 -o cloudflared
chmod +x cloudflared

# 一键穿透（临时，重启后地址变化）
./cloudflared tunnel --url http://localhost:5000

# 永久固定域名（需要 Cloudflare 账号）
./cloudflared tunnel login
./cloudflared tunnel create camera-rs
./cloudflared tunnel route dns camera-rs your-domain.com
./cloudflared tunnel run camera-rs
```

---

## Vercel 查看器（云端远程浏览历史截图）

1. 在 camera_rs 设置页配置 OneDrive/Google Drive
2. 点击「生成文件夹链接」
3. 将 `vercel-viewer/` 目录部署到 Vercel
4. 粘贴分享链接即可远程查看

---

## 配置文件（config.toml）

首次运行自动生成，修改后重启生效：

```toml
[server]
host = "0.0.0.0"
port = 5000

[camera]
index = 0      # 默认摄像头索引
width = 1920
height = 1080

[security]
password = "admin"          # 登录密码
max_login_attempts = 5
lockout_secs = 900

[storage]
save_dir = "captures"
max_size_mb = 2048
auto_cleanup = true
```
