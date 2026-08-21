pub const LOGIN_HTML: &str = r#"<!DOCTYPE html><html lang="zh"><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>监控系统 - 登录</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{background:linear-gradient(135deg,#0f0c29 0%,#1a1a2e 50%,#24243e 100%);display:flex;align-items:center;justify-content:center;height:100vh;font-family:'Segoe UI',system-ui,sans-serif}
.box{background:rgba(22,33,62,.75);backdrop-filter:blur(12px);border:1px solid rgba(15,52,96,.8);border-radius:18px;padding:44px 36px;width:360px;text-align:center;box-shadow:0 20px 60px rgba(0,0,0,.5),0 0 40px rgba(233,69,96,.06)}
.logo{width:56px;height:56px;margin:0 auto 18px;border-radius:16px;background:linear-gradient(135deg,#e94560,#c2314d);display:flex;align-items:center;justify-content:center;font-size:28px;box-shadow:0 8px 24px rgba(233,69,96,.35)}
h2{color:#eee;margin-bottom:6px;font-size:20px;font-weight:600}
.sub{color:#6a7ba2;font-size:12px;margin-bottom:26px}
input{width:100%;padding:12px 16px;border-radius:10px;border:1px solid #0f3460;background:rgba(26,26,46,.8);color:#eee;font-size:15px;margin-bottom:14px;outline:none;transition:border-color .25s,box-shadow .25s}
input:focus{border-color:#e94560;box-shadow:0 0 0 3px rgba(233,69,96,.15)}
button{width:100%;padding:12px;background:linear-gradient(135deg,#e94560,#d63850);color:#fff;border:none;border-radius:10px;font-size:15px;font-weight:600;cursor:pointer;transition:transform .15s,box-shadow .25s}
button:hover{transform:translateY(-1px);box-shadow:0 6px 20px rgba(233,69,96,.4)}
button:active{transform:translateY(0)}
.err{color:#ff6b81;font-size:13px;margin-bottom:12px}
</style></head><body>
<div class="box">
  <div class="logo">📷</div>
  <h2>摄像头监控系统</h2>
  <div class="sub">Rust · 安全访问</div>
  <div id="err-msg" class="err" style="display:none"></div>
  <form method="post" action="/login">
    <input type="password" name="password" placeholder="输入访问密码" autofocus>
    <button type="submit">进入</button>
  </form>
</div>
<script>
const p=new URLSearchParams(location.search),el=document.getElementById('err-msg');
if(p.get('error')==='locked'){el.textContent='登录失败次数过多，请'+p.get('secs')+'秒后重试';el.style.display='block';}
else if(p.get('error')){el.textContent='密码错误';el.style.display='block';}
</script></body></html>"#;

pub const MAIN_HTML: &str = r#"<!DOCTYPE html><html lang="zh"><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>摄像头监控系统</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{background:#1a1a2e;color:#eee;font-family:'Segoe UI',sans-serif}
header{background:#16213e;padding:12px 18px;display:flex;align-items:center;justify-content:space-between;border-bottom:1px solid #0f3460}
header h1{font-size:17px;color:#e94560}
.header-right{display:flex;align-items:center;gap:8px;flex-wrap:wrap}
#clock{font-size:12px;color:#aaa}
.nav-link{font-size:13px;color:#7af;cursor:pointer;padding:5px 10px;border-radius:6px;border:1px solid #0f3460}
.nav-link:hover,.nav-link.active{background:#0f3460}
.main{display:flex;gap:12px;padding:12px;flex-wrap:wrap}
.video-box{flex:1;min-width:480px;background:#000;border-radius:10px;overflow:hidden;position:relative}
.video-box img{width:100%;display:block}
.badge{position:absolute;top:10px;left:10px;background:rgba(233,69,96,.9);color:#fff;padding:3px 10px;border-radius:12px;font-size:13px;display:none}
.badge.show{display:inline-block}
#motion-alert{position:absolute;bottom:10px;left:10px;background:rgba(233,69,96,.9);color:#fff;padding:4px 12px;border-radius:8px;font-size:13px;display:none}
.rust-badge{position:absolute;top:10px;right:10px;background:rgba(180,80,0,.85);color:#fff;padding:3px 10px;border-radius:12px;font-size:12px;font-weight:bold}
.panel{width:270px;display:flex;flex-direction:column;gap:10px}
.card{background:#16213e;border-radius:10px;padding:12px;border:1px solid #0f3460}
.card h3{font-size:11px;color:#aaa;margin-bottom:9px;text-transform:uppercase;letter-spacing:1px}
.btn{width:100%;padding:8px;border:none;border-radius:7px;cursor:pointer;font-size:13px;font-weight:600;margin-bottom:6px;transition:opacity .2s}
.btn:hover{opacity:.85}.btn:last-child{margin-bottom:0}
.btn-blue{background:#0f3460;color:#fff}.btn-green{background:#0d7377;color:#fff}
.btn-red{background:#e94560;color:#fff}.btn-orange{background:#c86020;color:#fff}
.btn-row{display:flex;gap:5px;margin-bottom:6px}
.btn-row .btn{margin-bottom:0}
.toggle-row{display:flex;align-items:center;justify-content:space-between;margin-bottom:8px}
.toggle-label{font-size:13px}
.switch{position:relative;width:42px;height:22px}
.switch input{display:none}
.slider{position:absolute;inset:0;background:#555;border-radius:22px;cursor:pointer;transition:.3s}
.slider:before{content:'';position:absolute;width:16px;height:16px;left:3px;top:3px;background:#fff;border-radius:50%;transition:.3s}
input:checked+.slider{background:#0d7377}
input:checked+.slider:before{transform:translateX(20px)}
.stat-row{display:flex;justify-content:space-between;font-size:12px;padding:3px 0;border-bottom:1px solid #0f3460}
.stat-row:last-child{border-bottom:none}
.stat-val{color:#e94560;font-weight:600}
input[type=range]{width:100%;accent-color:#e94560;margin:3px 0}
.range-row{display:flex;justify-content:space-between;font-size:12px;color:#aaa;margin-bottom:2px}
select.form-sel{width:100%;padding:8px;border-radius:6px;border:1px solid #0f3460;background:#1a1a2e;color:#eee;font-size:13px;cursor:pointer;outline:none}
#toast{position:fixed;bottom:18px;right:18px;background:#0d7377;color:#fff;padding:8px 16px;border-radius:8px;font-size:13px;opacity:0;transition:opacity .3s;pointer-events:none;z-index:999}
#toast.show{opacity:1}
/* 文件管理 */
.gallery-tabs{display:flex;gap:8px;margin-bottom:12px;flex-wrap:wrap}
.tab-btn{padding:6px 14px;border-radius:7px;border:1px solid #0f3460;background:#16213e;color:#eee;cursor:pointer;font-size:13px}
.tab-btn.active{background:#e94560;border-color:#e94560}
.gallery-grid{display:flex;flex-wrap:wrap;gap:10px}
.gallery-item{background:#16213e;border:1px solid #0f3460;border-radius:8px;overflow:hidden;width:175px}
.gallery-item img,.gallery-item video{width:100%;display:block;cursor:pointer}
.item-name{font-size:11px;color:#aaa;padding:4px 6px;word-break:break-all}
.item-actions{display:flex;gap:4px;padding:0 6px 6px}
.item-btn{flex:1;padding:4px;border:none;border-radius:5px;cursor:pointer;font-size:12px}
.item-btn-dl{background:#0f3460;color:#fff}.item-btn-del{background:#e94560;color:#fff}
.no-files{color:#666;font-size:14px;padding:20px}
/* 设置页 */
.settings-grid{display:flex;gap:12px;flex-wrap:wrap;padding:12px}
.settings-col{flex:1;min-width:300px;display:flex;flex-direction:column;gap:12px}
.form-row{margin-bottom:9px}
.form-row label{font-size:12px;color:#aaa;display:block;margin-bottom:3px}
.form-input{width:100%;padding:7px;border-radius:6px;border:1px solid #0f3460;background:#1a1a2e;color:#eee;font-size:13px}
.form-input:focus{outline:none;border-color:#e94560}
.section-title{font-size:14px;color:#7af;font-weight:600;margin:4px 0 10px;padding-bottom:6px;border-bottom:1px solid #0f3460}
.status-ok{color:#0d7377;font-size:12px;margin-top:6px}
.status-err{color:#e94560;font-size:12px;margin-top:6px}
@media(max-width:700px){.panel{width:100%}.video-box{min-width:unset}}
</style></head><body>
<header>
  <h1>摄像头监控系统 <span style="font-size:11px;color:#c86020">Rust</span></h1>
  <div class="header-right">
    <span id="clock"></span>
    <span class="nav-link active" id="nav-monitor" onclick="showPage('monitor')">实时监控</span>
    <span class="nav-link" id="nav-gallery" onclick="showPage('gallery');loadGallery('photos')">文件管理</span>
    <span class="nav-link" id="nav-settings" onclick="showPage('settings');loadSettings()">设置</span>
    <a href="/logout" class="nav-link">退出</a>
  </div>
</header>

<!-- ===== 实时监控 ===== -->
<div id="page-monitor">
<div class="main">
  <div class="video-box">
    <img src="/video" id="stream" alt="视频流">
    <span class="badge" id="rec-badge">● REC</span>
    <span class="rust-badge">Rust</span>
    <span id="motion-alert">⚠ 移动侦测!</span>
  </div>
  <div class="panel">
    <div class="card">
      <h3>摄像头选择</h3>
      <select class="form-sel" id="camera-select" onchange="switchCamera(this.value)"><option>检测中...</option></select>
    </div>
    <div class="card">
      <h3>控制</h3>
      <button class="btn btn-blue" onclick="takePhoto()">拍照保存</button>
      <button class="btn btn-green" id="rec-btn" onclick="toggleRecord()">开始录像</button>
    </div>
    <div class="card">
      <h3>画面调节</h3>
      <div class="range-row"><span>亮度</span><span id="bright-val">0</span></div>
      <input type="range" min="-100" max="100" value="0" oninput="updateImg('brightness',this.value,'bright-val')">
      <div class="range-row"><span>对比度</span><span id="contrast-val">0</span></div>
      <input type="range" min="-100" max="100" value="0" oninput="updateImg('contrast',this.value,'contrast-val')">
      <div class="range-row"><span>饱和度</span><span id="sat-val">0</span></div>
      <input type="range" min="-100" max="100" value="0" oninput="updateImg('saturation',this.value,'sat-val')">
      <div class="btn-row" style="margin-top:8px">
        <button class="btn btn-blue" onclick="api('/set_flip?h=1')">⟺ 水平</button>
        <button class="btn btn-blue" onclick="api('/set_flip?v=1')">↕ 垂直</button>
      </div>
      <div class="btn-row">
        <button class="btn btn-blue" onclick="api('/set_rotation?deg=90')">↻90°</button>
        <button class="btn btn-blue" onclick="api('/set_rotation?deg=180')">↺180°</button>
        <button class="btn btn-blue" onclick="api('/set_rotation?deg=270')">↻270°</button>
        <button class="btn btn-blue" onclick="api('/set_rotation?deg=0')">⟳复位</button>
      </div>
    </div>
    <div class="card">
      <h3>功能开关</h3>
      <div class="toggle-row"><span class="toggle-label">移动侦测</span><label class="switch"><input type="checkbox" id="t-motion" onchange="toggle('motion',this.checked)"><span class="slider"></span></label></div>
      <div class="toggle-row"><span class="toggle-label">运动门控</span><label class="switch"><input type="checkbox" id="t-gate" onchange="toggle('gate',this.checked)" checked><span class="slider"></span></label></div>
      <div class="toggle-row"><span class="toggle-label">定时截图</span><label class="switch"><input type="checkbox" id="t-auto" onchange="toggle('auto',this.checked)"><span class="slider"></span></label></div>
      <div style="margin-top:8px">
        <div class="range-row"><span>截图间隔</span><span id="interval-val">10s</span></div>
        <input type="range" min="5" max="120" value="10" step="5" oninput="document.getElementById('interval-val').textContent=this.value+'s';api('/set_interval?val='+this.value)">
      </div>
    </div>
    <div class="card">
      <h3>侦测参数</h3>
      <div class="range-row"><span>差分阈值</span><span id="sens-val">30</span></div>
      <input type="range" min="5" max="80" value="30" step="5" oninput="document.getElementById('sens-val').textContent=this.value;api('/set_sensitivity?val='+this.value)">
      <div style="margin-top:6px">
        <div class="range-row"><span>最小面积 px²</span><span id="area-val">1500</span></div>
        <input type="range" min="200" max="10000" value="1500" step="100" oninput="document.getElementById('area-val').textContent=this.value;api('/set_min_area?val='+this.value)">
      </div>
    </div>
    <div class="card">
      <h3>统计</h3>
      <div class="stat-row"><span>分辨率</span><span class="stat-val" id="st-res">-</span></div>
      <div class="stat-row"><span>当前摄像头</span><span class="stat-val" id="st-cam">0</span></div>
      <div class="stat-row"><span>移动触发</span><span class="stat-val" id="st-motion">0</span></div>
      <div class="stat-row"><span>拍照张数</span><span class="stat-val" id="st-photo">0</span></div>
      <div class="stat-row"><span>录像时长</span><span class="stat-val" id="st-rec">00:00</span></div>
    </div>
  </div>
</div>
</div>

<!-- ===== 文件管理 ===== -->
<div id="page-gallery" style="display:none;padding:12px">
  <div class="gallery-tabs">
    <button class="tab-btn active" id="tab-photos" onclick="loadGallery('photos')">照片</button>
    <button class="tab-btn" id="tab-videos" onclick="loadGallery('videos')">录像</button>
    <button class="tab-btn" id="tab-motion" onclick="loadGallery('motion')">移动触发</button>
    <button class="tab-btn" id="tab-auto" onclick="loadGallery('auto')">定时截图</button>
    <button class="tab-btn" id="tab-alerts" onclick="loadGallery('alerts')">告警截图</button>
  </div>
  <div class="gallery-grid" id="gallery-grid"><span class="no-files">请选择分类</span></div>
</div>

<!-- ===== 设置页 ===== -->
<div id="page-settings" style="display:none">
<div class="settings-grid">

  <!-- 左列 -->
  <div class="settings-col">

    <!-- OneDrive -->
    <div class="card">
      <div class="section-title">☁️ OneDrive 云存储</div>
      <div class="form-row"><label>Maton API Key</label><input type="password" id="od-key" class="form-input" placeholder="在 maton.ai/settings 获取"></div>
      <div class="form-row"><label>目标文件夹</label><input type="text" id="od-folder" class="form-input" value="camera_rs"></div>
      <div class="toggle-row"><span class="toggle-label">启用 OneDrive</span><label class="switch"><input type="checkbox" id="od-enabled"><span class="slider"></span></label></div>
      <div class="toggle-row"><span class="toggle-label">上传运动截图</span><label class="switch"><input type="checkbox" id="od-motion" checked><span class="slider"></span></label></div>
      <div class="toggle-row"><span class="toggle-label">上传手动截图</span><label class="switch"><input type="checkbox" id="od-photos" checked><span class="slider"></span></label></div>
      <div class="toggle-row"><span class="toggle-label">上传录像</span><label class="switch"><input type="checkbox" id="od-videos"><span class="slider"></span></label></div>
      <div class="toggle-row"><span class="toggle-label">生成分享链接</span><label class="switch"><input type="checkbox" id="od-share" checked><span class="slider"></span></label></div>
      <div style="display:flex;gap:6px;margin-top:8px">
        <button class="btn btn-green" style="flex:1" onclick="saveOneDrive()">保存</button>
        <button class="btn btn-blue" style="flex:1" onclick="createShare()">生成文件夹链接</button>
        <button class="btn btn-blue" style="flex:1" onclick="api('/upload_now').then(d=>toast(d.ok?'已上传当前帧':'上传失败'))">上传当前帧</button>
      </div>
      <div class="form-row" style="margin-top:8px"><label>文件夹分享链接（供 Vercel Viewer 使用）</label><input type="text" id="od-share-url" class="form-input" readonly placeholder="点击"生成文件夹链接"后自动填入"></div>
      <p id="od-status" class="status-ok" style="display:none"></p>
    </div>

    <!-- 邮件 -->
    <div class="card">
      <div class="section-title">📧 邮件告警</div>
      <div class="toggle-row"><span class="toggle-label">启用邮件告警</span><label class="switch"><input type="checkbox" id="cfg-email-on"><span class="slider"></span></label></div>
      <div class="form-row"><label>SMTP 服务器</label><input type="text" id="cfg-smtp-host" class="form-input" value="smtp.gmail.com"></div>
      <div class="form-row"><label>SMTP 端口</label><input type="number" id="cfg-smtp-port" class="form-input" value="465"></div>
      <div class="form-row"><label>发件人邮箱</label><input type="email" id="cfg-email-from" class="form-input"></div>
      <div class="form-row"><label>邮箱密码/应用密码</label><input type="password" id="cfg-email-pass" class="form-input" placeholder="留空不修改"></div>
      <div class="form-row"><label>收件人邮箱</label><input type="email" id="cfg-email-to" class="form-input"></div>
      <div class="form-row"><label>冷却时间（秒）</label><input type="number" id="cfg-cooldown" class="form-input" value="60"></div>
      <div style="display:flex;gap:6px">
        <button class="btn btn-green" style="flex:1" onclick="saveEmail()">保存邮件配置</button>
        <button class="btn btn-blue" style="flex:1" onclick="testEmail()">发送测试</button>
      </div>
    </div>

  </div>

  <!-- 右列 -->
  <div class="settings-col">

    <!-- Telegram -->
    <div class="card">
      <div class="section-title">✈️ Telegram</div>
      <div class="toggle-row"><span class="toggle-label">启用</span><label class="switch"><input type="checkbox" id="tg-enabled"><span class="slider"></span></label></div>
      <div class="form-row"><label>Bot Token</label><input type="text" id="tg-token" class="form-input" placeholder="从 @BotFather 获取"></div>
      <div class="form-row"><label>Chat ID</label><input type="text" id="tg-chat" class="form-input" placeholder="用户/群组 ID"></div>
      <div class="toggle-row"><span class="toggle-label">发送图片</span><label class="switch"><input type="checkbox" id="tg-photo" checked><span class="slider"></span></label></div>
    </div>

    <!-- 钉钉 -->
    <div class="card">
      <div class="section-title">🔔 钉钉机器人</div>
      <div class="toggle-row"><span class="toggle-label">启用</span><label class="switch"><input type="checkbox" id="dd-enabled"><span class="slider"></span></label></div>
      <div class="form-row"><label>Webhook URL</label><input type="text" id="dd-url" class="form-input" placeholder="https://oapi.dingtalk.com/robot/send?..."></div>
      <div class="form-row"><label>加签密钥（可选）</label><input type="text" id="dd-secret" class="form-input" placeholder="SEC..."></div>
    </div>

    <!-- 企业微信 -->
    <div class="card">
      <div class="section-title">💬 企业微信机器人</div>
      <div class="toggle-row"><span class="toggle-label">启用</span><label class="switch"><input type="checkbox" id="wc-enabled"><span class="slider"></span></label></div>
      <div class="form-row"><label>Webhook URL</label><input type="text" id="wc-url" class="form-input" placeholder="https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=..."></div>
    </div>

    <!-- Server酱 -->
    <div class="card">
      <div class="section-title">📱 Server酱（微信推送）</div>
      <div class="toggle-row"><span class="toggle-label">启用</span><label class="switch"><input type="checkbox" id="sc-enabled"><span class="slider"></span></label></div>
      <div class="form-row"><label>SendKey</label><input type="text" id="sc-key" class="form-input" placeholder="在 sct.ftqq.com 获取"></div>
    </div>

    <!-- Bark -->
    <div class="card">
      <div class="section-title">🍎 Bark（iOS 推送）</div>
      <div class="toggle-row"><span class="toggle-label">启用</span><label class="switch"><input type="checkbox" id="bk-enabled"><span class="slider"></span></label></div>
      <div class="form-row"><label>Server URL</label><input type="text" id="bk-url" class="form-input" placeholder="https://api.day.app/你的Key"></div>
      <div class="form-row"><label>通知音效</label><input type="text" id="bk-sound" class="form-input" placeholder="alarm（留空=默认）"></div>
      <div class="form-row"><label>通知分组</label><input type="text" id="bk-group" class="form-input" placeholder="摄像头告警"></div>
    </div>

    <!-- PushPlus -->
    <div class="card">
      <div class="section-title">🔔 PushPlus</div>
      <div class="toggle-row"><span class="toggle-label">启用</span><label class="switch"><input type="checkbox" id="pp-enabled"><span class="slider"></span></label></div>
      <div class="form-row"><label>Token</label><input type="text" id="pp-token" class="form-input" placeholder="在 pushplus.plus 获取"></div>
      <div class="form-row"><label>群组 Topic（可选）</label><input type="text" id="pp-topic" class="form-input"></div>
    </div>

    <!-- Webhook -->
    <div class="card">
      <div class="section-title">🔗 通用 Webhook</div>
      <div class="toggle-row"><span class="toggle-label">启用</span><label class="switch"><input type="checkbox" id="wh-enabled"><span class="slider"></span></label></div>
      <div class="form-row"><label>URL</label><input type="text" id="wh-url" class="form-input" placeholder="https://your-server.com/hook"></div>
      <div class="form-row"><label>Body 模板（JSON，留空=默认）</label><input type="text" id="wh-body" class="form-input" placeholder='{"event":"{event}","count":"{count}","image":"{image_url}"}'></div>
    </div>

    <!-- 通知操作 -->
    <div class="card">
      <button class="btn btn-green" onclick="saveNotify()">保存所有通知配置</button>
      <button class="btn btn-blue" onclick="testNotify()" style="margin-top:6px">发送测试通知（所有渠道）</button>
      <p id="notify-status" class="status-ok" style="display:none"></p>
    </div>

    <!-- 安全设置 -->
    <div class="card">
      <div class="section-title">🔒 安全设置</div>
      <div class="form-row"><label>新密码（留空 = 不修改）</label><input type="password" id="sec-password" class="form-input" placeholder="不回传旧密码，更安全"></div>
      <div class="form-row"><label>IP 白名单（逗号分隔，支持 192.168.1.* 通配，留空 = 不限制）</label><input type="text" id="sec-whitelist" class="form-input" placeholder="192.168.31.*,127.0.0.1"></div>
      <div class="range-row"><span>最大登录失败次数</span></div>
      <input type="number" id="sec-max-attempts" class="form-input" min="3" max="20" value="5">
      <div class="range-row" style="margin-top:6px"><span>锁定时长（秒）</span></div>
      <input type="number" id="sec-lockout" class="form-input" min="60" max="86400" value="900">
      <button class="btn btn-green" style="margin-top:10px" onclick="saveSecurity()">保存安全设置</button>
      <p id="sec-status" class="status-ok" style="display:none"></p>
    </div>

    <!-- 录像限制 -->
    <div class="card">
      <div class="section-title">⏺️ 录像限制（防内存溢出，自动分段保存）</div>
      <div class="toggle-row"><span class="toggle-label">到达上限后自动分段</span><label class="switch"><input type="checkbox" id="rl-split" checked><span class="slider"></span></label></div>
      <div class="range-row"><span>最长录像（分钟，0=不限）</span></div>
      <input type="number" id="rl-duration" class="form-input" min="0" max="720" value="30">
      <div class="range-row" style="margin-top:6px"><span>最大体积（MB，0=不限）</span></div>
      <input type="number" id="rl-size" class="form-input" min="0" max="10240" value="500">
      <button class="btn btn-green" style="margin-top:10px" onclick="saveRecordLimits()">保存录像限制</button>
      <p id="rl-status" class="status-ok" style="display:none"></p>
    </div>

    <!-- 系统信息 -->
    <div class="card">
      <div class="section-title">ℹ️ 系统信息</div>
      <div class="stat-row"><span>引擎</span><span class="stat-val">Rust + nokhwa</span></div>
      <div class="stat-row"><span>通知渠道</span><span class="stat-val">7 种</span></div>
      <div class="stat-row"><span>云存储</span><span class="stat-val">OneDrive</span></div>
      <div class="stat-row"><span>版本</span><span class="stat-val">3.1.0</span></div>
    </div>

  </div>
</div>
</div>

<div id="toast"></div>
<script>
let recording=false,recStart=null,recTimer=null,photoCount=0,imgDebounce={};

function showPage(n){
  ['monitor','gallery','settings'].forEach(p=>{
    document.getElementById('page-'+p).style.display=p===n?'':'none';
    document.getElementById('nav-'+p)?.classList.toggle('active',p===n);
  });
}
function toast(msg,color='#0d7377'){
  const el=document.getElementById('toast');
  el.style.background=color;el.textContent=msg;
  el.classList.add('show');setTimeout(()=>el.classList.remove('show'),2800);
}
function api(url){return fetch(url).then(r=>r.json());}

// 摄像头
function loadCameras(){
  fetch('/cameras').then(r=>r.json()).then(cams=>{
    const sel=document.getElementById('camera-select');
    if(!cams.length){sel.innerHTML='<option value="0">摄像头 0</option><option value="1">摄像头 1</option>';return;}
    sel.innerHTML=cams.map(c=>`<option value="${c.index}">${c.index}: ${c.name}</option>`).join('');
  }).catch(()=>{document.getElementById('camera-select').innerHTML='<option value="0">摄像头 0</option><option value="1">摄像头 1</option>';});
}
function switchCamera(i){if(i==='')return;api('/switch_camera?index='+i).then(d=>d.ok&&toast('已切换到摄像头 '+i));}

// 图像调节
function updateImg(p,v,lid){
  document.getElementById(lid).textContent=v;
  clearTimeout(imgDebounce[p]);
  imgDebounce[p]=setTimeout(()=>api('/set_image?'+p+'='+v),300);
}

// 拍照/录像
function takePhoto(){api('/photo').then(d=>{if(d.ok){photoCount++;document.getElementById('st-photo').textContent=photoCount;toast('截图已保存');}else toast('截图失败','#e94560');});}
function toggleRecord(){
  api('/record').then(d=>{
    recording=d.recording;
    const btn=document.getElementById('rec-btn'),badge=document.getElementById('rec-badge');
    if(recording){btn.textContent='停止录像';btn.className='btn btn-red';badge.classList.add('show');recStart=Date.now();recTimer=setInterval(updateRecTime,1000);toast('开始录像');}
    else{btn.textContent='开始录像';btn.className='btn btn-green';badge.classList.remove('show');clearInterval(recTimer);document.getElementById('st-rec').textContent='00:00';toast('录像已保存');}
  });
}
function updateRecTime(){const s=Math.floor((Date.now()-recStart)/1000);document.getElementById('st-rec').textContent=String(Math.floor(s/60)).padStart(2,'0')+':'+String(s%60).padStart(2,'0');}
function toggle(name,on){api('/toggle?name='+name+'&on='+(on?1:0)).then(()=>toast(name+(on?' 已开启':' 已关闭')));}

// 统计
setInterval(()=>{
  fetch('/stats').then(r=>r.json()).then(d=>{
    document.getElementById('st-res').textContent=d.resolution;
    document.getElementById('st-motion').textContent=d.motion_count;
    document.getElementById('st-cam').textContent=d.camera_idx;
    if(d.motion_now){const ma=document.getElementById('motion-alert');ma.style.display='block';setTimeout(()=>ma.style.display='none',1500);}
    const sel=document.getElementById('camera-select');
    if(sel.options.length>1&&sel.value!==String(d.camera_idx))sel.value=String(d.camera_idx);
  });
},1000);
setInterval(()=>document.getElementById('clock').textContent=new Date().toLocaleString('zh-CN'),1000);

// WebSocket
function connectWs(){
  const ws=new WebSocket('ws://'+location.host+'/ws');
  ws.onmessage=e=>{try{const d=JSON.parse(e.data);if(d.event==='motion')toast('⚠️ 检测到移动！','#e94560');if(d.event==='camera_switched')toast('摄像头已切换到 '+d.index);}catch(_){}};
  ws.onclose=()=>setTimeout(connectWs,3000);
}

// 文件管理
const TYPES=['photos','videos','motion','auto','alerts'];
function loadGallery(type){
  TYPES.forEach(t=>document.getElementById('tab-'+t)?.classList.toggle('active',t===type));
  const grid=document.getElementById('gallery-grid');
  grid.innerHTML='<span class="no-files">加载中...</span>';
  fetch('/files?type='+type).then(r=>r.json()).then(d=>{
    if(!d.files.length){grid.innerHTML='<span class="no-files">暂无文件</span>';return;}
    grid.innerHTML='';
    d.files.forEach(f=>{
      const isVid=f.endsWith('.avi')||f.endsWith('.mp4');
      const div=document.createElement('div');div.className='gallery-item';
      div.innerHTML=(isVid?`<video controls><source src="/file/${type}/${f}"></video>`:`<img src="/file/${type}/${f}" onclick="window.open(this.src)">`)
        +`<div class="item-name">${f}</div><div class="item-actions"><a href="/file/${type}/${f}" download class="item-btn item-btn-dl">下载</a><button class="item-btn item-btn-del" onclick="delFile('${type}','${f}',this.closest('.gallery-item'))">删除</button></div>`;
      grid.appendChild(div);
    });
  });
}
function delFile(type,name,el){
  if(!confirm('确认删除 '+name+'?'))return;
  fetch('/delete?type='+type+'&name='+encodeURIComponent(name),{method:'POST'}).then(r=>r.json()).then(d=>{if(d.ok){el.remove();toast('已删除');}else toast('删除失败','#e94560');});
}

// ===== 设置页 =====
function loadSettings(){
  // 加载通知配置
  fetch('/notify_config').then(r=>r.json()).then(c=>{
    setChk('tg-enabled',c.telegram?.enabled);setVal('tg-token',c.telegram?.bot_token);setVal('tg-chat',c.telegram?.chat_id);setChk('tg-photo',c.telegram?.send_photo??true);
    setChk('dd-enabled',c.dingtalk?.enabled);setVal('dd-url',c.dingtalk?.webhook_url);setVal('dd-secret',c.dingtalk?.secret);
    setChk('wc-enabled',c.wecom?.enabled);setVal('wc-url',c.wecom?.webhook_url);
    setChk('sc-enabled',c.serverchan?.enabled);setVal('sc-key',c.serverchan?.send_key);
    setChk('bk-enabled',c.bark?.enabled);setVal('bk-url',c.bark?.server_url);setVal('bk-sound',c.bark?.sound);setVal('bk-group',c.bark?.group);
    setChk('pp-enabled',c.pushplus?.enabled);setVal('pp-token',c.pushplus?.token);setVal('pp-topic',c.pushplus?.topic);
    setChk('wh-enabled',c.webhook?.enabled);setVal('wh-url',c.webhook?.url);setVal('wh-body',c.webhook?.body_template);
  });
  // 加载 OneDrive 配置
  fetch('/onedrive_config').then(r=>r.json()).then(c=>{
    setVal('od-key',c.maton_api_key);setVal('od-folder',c.folder||'camera_rs');
    setChk('od-enabled',c.enabled);setChk('od-motion',c.upload_motion??true);
    setChk('od-photos',c.upload_photos??true);setChk('od-videos',c.upload_videos);
    setChk('od-share',c.create_share_links??true);setVal('od-share-url',c.share_folder_url);
  });
  loadSecurity();
  loadRecordLimits();
}
function setVal(id,v){const el=document.getElementById(id);if(el&&v!=null)el.value=v;}
function setChk(id,v){const el=document.getElementById(id);if(el&&v!=null)el.checked=!!v;}
function getChk(id){return document.getElementById(id)?.checked??false;}
function getVal(id){return document.getElementById(id)?.value??'';}

function saveNotify(){
  const cfg={
    telegram:{enabled:getChk('tg-enabled'),bot_token:getVal('tg-token'),chat_id:getVal('tg-chat'),send_photo:getChk('tg-photo')},
    dingtalk:{enabled:getChk('dd-enabled'),webhook_url:getVal('dd-url'),secret:getVal('dd-secret')},
    wecom:{enabled:getChk('wc-enabled'),webhook_url:getVal('wc-url')},
    serverchan:{enabled:getChk('sc-enabled'),send_key:getVal('sc-key')},
    bark:{enabled:getChk('bk-enabled'),server_url:getVal('bk-url'),sound:getVal('bk-sound'),group:getVal('bk-group'),icon:''},
    pushplus:{enabled:getChk('pp-enabled'),token:getVal('pp-token'),topic:getVal('pp-topic')},
    webhook:{enabled:getChk('wh-enabled'),url:getVal('wh-url'),body_template:getVal('wh-body')}
  };
  fetch('/notify_config',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(cfg)})
    .then(r=>r.json()).then(d=>{
      const el=document.getElementById('notify-status');
      el.textContent=d.ok?'✅ 已保存':'❌ 保存失败';
      el.className=d.ok?'status-ok':'status-err';
      el.style.display='block';
      if(d.ok)toast('通知配置已保存');
    });
}

function testNotify(){
  fetch('/test_notify').then(r=>r.json()).then(d=>toast(d.ok?'测试通知已发送到所有已启用渠道':'发送失败'));
}

function saveOneDrive(){
  const cfg={
    enabled:getChk('od-enabled'),maton_api_key:getVal('od-key'),folder:getVal('od-folder'),
    upload_photos:getChk('od-photos'),upload_motion:getChk('od-motion'),upload_videos:getChk('od-videos'),
    create_share_links:getChk('od-share'),share_folder_url:getVal('od-share-url')
  };
  fetch('/onedrive_config',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(cfg)})
    .then(r=>r.json()).then(d=>{
      const el=document.getElementById('od-status');
      el.textContent=d.ok?'✅ 已保存':'❌ 保存失败';
      el.className=d.ok?'status-ok':'status-err';
      el.style.display='block';
      if(d.ok)toast('OneDrive 配置已保存');
    });
}

function createShare(){
  toast('正在生成分享链接...');
  fetch('/onedrive_share').then(r=>r.json()).then(d=>{
    if(d.ok){setVal('od-share-url',d.url);toast('文件夹分享链接已生成');}
    else toast(d.error||'生成失败，请检查配置','#e94560');
  });
}

function saveEmail(){
  fetch('/save_config',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({
    email_enabled:getChk('cfg-email-on'),smtp_host:getVal('cfg-smtp-host'),
    smtp_port:parseInt(getVal('cfg-smtp-port')),email_from:getVal('cfg-email-from'),
    email_password:getVal('cfg-email-pass'),email_to:getVal('cfg-email-to'),
    cooldown:parseInt(getVal('cfg-cooldown')||'60'),on_motion:true,on_unknown:false
  })}).then(r=>r.json()).then(d=>{if(d.ok)toast('邮件配置已保存');});
}

function testEmail(){
  fetch('/test_email').then(r=>r.json()).then(d=>{
    if(d.ok)toast('测试邮件已发送');else toast('发送失败: '+d.error,'#e94560');
  });
}

// ===== 安全设置 =====
function loadSecurity(){
  fetch('/security_config').then(r=>r.json()).then(c=>{
    setVal('sec-whitelist',(c.ip_whitelist||[]).join(','));
    setVal('sec-max-attempts',c.max_login_attempts??5);
    setVal('sec-lockout',c.lockout_secs??900);
    const pw=document.getElementById('sec-password');
    if(pw)pw.placeholder=c.has_password?'已设置密码，留空不修改':'未设置密码';
  });
}
function saveSecurity(){
  const body={
    ip_whitelist:getVal('sec-whitelist').split(',').map(s=>s.trim()).filter(Boolean),
    https_enabled:false,cert_path:'cert.pem',key_path:'key.pem',
    password:getVal('sec-password'),   // 空字符串 = 不修改
    max_login_attempts:parseInt(getVal('sec-max-attempts')||'5'),
    lockout_secs:parseInt(getVal('sec-lockout')||'900')
  };
  fetch('/security_config',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)})
    .then(r=>r.json()).then(d=>{
      const el=document.getElementById('sec-status');
      el.textContent=d.ok?'✅ 已保存，下次登录生效':'❌ 保存失败';
      el.className=d.ok?'status-ok':'status-err';el.style.display='block';
      if(d.ok){document.getElementById('sec-password').value='';toast('安全设置已保存');loadSecurity();}
    });
}

// ===== 录像限制 =====
function loadRecordLimits(){
  fetch('/record_limits').then(r=>r.json()).then(c=>{
    setChk('rl-split',c.auto_split??true);
    setVal('rl-duration',Math.round((c.max_duration_secs||0)/60));
    setVal('rl-size',c.max_size_mb||0);
  });
}
function saveRecordLimits(){
  const body={
    max_duration_secs:parseInt(getVal('rl-duration')||'0')*60,
    max_size_mb:parseInt(getVal('rl-size')||'0'),
    auto_split:getChk('rl-split')
  };
  fetch('/record_limits',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)})
    .then(r=>r.json()).then(d=>{
      const el=document.getElementById('rl-status');
      el.textContent=d.ok?'✅ 已保存':'❌ 保存失败';
      el.className=d.ok?'status-ok':'status-err';el.style.display='block';
      if(d.ok)toast('录像限制已保存');
    });
}

loadCameras();
connectWs();
</script></body></html>"#;

pub const MULTIVIEW_HTML: &str = r#"<!DOCTYPE html><html lang="zh"><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>多摄像头分屏 - 监控系统</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{background:#111;color:#eee;font-family:'Segoe UI',sans-serif}
header{background:#16213e;padding:10px 16px;display:flex;align-items:center;justify-content:space-between;border-bottom:1px solid #0f3460}
header h1{font-size:16px;color:#e94560}
.controls{display:flex;gap:8px;align-items:center}
.btn{padding:6px 12px;border:none;border-radius:6px;cursor:pointer;font-size:13px;font-weight:600}
.btn-blue{background:#0f3460;color:#fff}.btn-red{background:#e94560;color:#fff}
.grid{display:grid;gap:4px;padding:4px;height:calc(100vh - 52px)}
.grid.g1{grid-template-columns:1fr}
.grid.g2{grid-template-columns:1fr 1fr}
.grid.g4{grid-template-columns:1fr 1fr;grid-template-rows:1fr 1fr}
.cam-box{background:#000;position:relative;overflow:hidden;border-radius:6px}
.cam-box img{width:100%;height:100%;object-fit:contain;display:block}
.cam-label{position:absolute;top:6px;left:8px;background:rgba(0,0,0,.7);color:#fff;padding:2px 8px;border-radius:8px;font-size:12px}
.cam-fps{position:absolute;top:6px;right:8px;background:rgba(0,0,0,.6);color:#0d7377;padding:2px 8px;border-radius:8px;font-size:11px}
.motion-dot{position:absolute;bottom:8px;left:8px;width:10px;height:10px;border-radius:50%;background:#0d7377;display:none}
.motion-dot.active{background:#e94560;display:block}
</style>
</head><body>
<header>
  <h1>多摄像头分屏监控</h1>
  <div class="controls">
    <button class="btn btn-blue" onclick="setLayout(1)">单屏</button>
    <button class="btn btn-blue" onclick="setLayout(2)">双屏</button>
    <button class="btn btn-blue" onclick="setLayout(4)">四屏</button>
    <span id="cam-count" style="font-size:12px;color:#aaa;margin-left:8px"></span>
    <a href="/" class="btn btn-blue">返回主页</a>
  </div>
</header>
<div class="grid g2" id="grid"></div>
<script>
let cameras=[], layout=2;

async function init(){
  const r=await fetch('/cameras').then(r=>r.json()).catch(()=>[]);
  cameras=r.length?r:[{index:0,name:'摄像头 0'},{index:1,name:'摄像头 1'}];
  document.getElementById('cam-count').textContent=cameras.length+' 个摄像头';
  setLayout(Math.min(layout,cameras.length)||1);
}

function setLayout(n){
  layout=n;
  const grid=document.getElementById('grid');
  grid.className='grid g'+n;
  grid.innerHTML='';
  const show=cameras.slice(0,n);
  show.forEach(cam=>{
    const box=document.createElement('div');
    box.className='cam-box';
    box.innerHTML=`
      <img src="/video?cam=${cam.index}" onerror="this.src='/video'">
      <div class="cam-label">${cam.index}: ${cam.name}</div>
      <div class="cam-fps" id="fps-${cam.index}">-- fps</div>
      <div class="motion-dot" id="mdot-${cam.index}"></div>`;
    grid.appendChild(box);
  });
}

setInterval(()=>{
  fetch('/stats').then(r=>r.json()).then(d=>{
    const el=document.getElementById('fps-'+d.camera_idx);
    if(el)el.textContent=d.fps?.toFixed(1)+' fps';
    const mdot=document.getElementById('mdot-'+d.camera_idx);
    if(mdot)mdot.classList.toggle('active',d.motion_now);
  });
},1000);

init();
</script></body></html>"#;
