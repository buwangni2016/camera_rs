/*!
 * HTML 模板（内嵌字符串，与 Python 版 UI 一致）
 */

pub const LOGIN_HTML: &str = r#"
<!DOCTYPE html><html lang="zh"><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>监控系统 - 登录</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{background:#1a1a2e;display:flex;align-items:center;justify-content:center;height:100vh;font-family:'Segoe UI',sans-serif}
.box{background:#16213e;border:1px solid #0f3460;border-radius:14px;padding:40px 32px;width:320px;text-align:center}
h2{color:#e94560;margin-bottom:24px;font-size:20px}
input{width:100%;padding:10px 14px;border-radius:8px;border:1px solid #0f3460;background:#1a1a2e;color:#eee;font-size:15px;margin-bottom:14px;outline:none}
input:focus{border-color:#e94560}
button{width:100%;padding:11px;background:#e94560;color:#fff;border:none;border-radius:8px;font-size:15px;font-weight:600;cursor:pointer}
button:hover{opacity:.85}
.err{color:#e94560;font-size:13px;margin-bottom:12px}
</style></head><body>
<div class="box">
  <h2>摄像头监控系统 (Rust)</h2>
  <div id="err-msg" class="err" style="display:none"></div>
  <form method="post" action="/login">
    <input type="password" name="password" placeholder="输入访问密码" autofocus>
    <button type="submit">进入</button>
  </form>
</div>
<script>
const params = new URLSearchParams(location.search);
if(params.get('error')){
  const el=document.getElementById('err-msg');
  el.textContent='密码错误';el.style.display='block';
}
</script>
</body></html>
"#;

pub const MAIN_HTML: &str = r#"
<!DOCTYPE html><html lang="zh"><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>摄像头监控系统 (Rust)</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{background:#1a1a2e;color:#eee;font-family:'Segoe UI',sans-serif}
a{color:inherit;text-decoration:none}
header{background:#16213e;padding:14px 20px;display:flex;align-items:center;justify-content:space-between;border-bottom:1px solid #0f3460}
header h1{font-size:18px;color:#e94560;letter-spacing:1px}
.header-right{display:flex;align-items:center;gap:10px}
#clock{font-size:13px;color:#aaa}
.nav-link{font-size:13px;color:#7af;cursor:pointer;padding:5px 10px;border-radius:6px;border:1px solid #0f3460}
.nav-link:hover,.nav-link.active{background:#0f3460}
.main{display:flex;gap:14px;padding:14px;flex-wrap:wrap}
.video-box{flex:1;min-width:480px;background:#000;border-radius:10px;overflow:hidden;position:relative}
.video-box img{width:100%;display:block}
.badge{position:absolute;top:10px;left:10px;background:rgba(233,69,96,.9);color:#fff;padding:3px 10px;border-radius:12px;font-size:13px;display:none}
.badge.show{display:inline-block}
#motion-alert{position:absolute;bottom:10px;left:10px;background:rgba(233,69,96,.9);color:#fff;padding:4px 12px;border-radius:8px;font-size:13px;display:none}
.panel{width:270px;display:flex;flex-direction:column;gap:12px}
.card{background:#16213e;border-radius:10px;padding:14px;border:1px solid #0f3460}
.card h3{font-size:12px;color:#aaa;margin-bottom:10px;text-transform:uppercase;letter-spacing:1px}
.btn{width:100%;padding:9px;border:none;border-radius:8px;cursor:pointer;font-size:13px;font-weight:600;margin-bottom:7px;transition:opacity .2s}
.btn:hover{opacity:.85}
.btn:last-child{margin-bottom:0}
.btn-blue{background:#0f3460;color:#fff}
.btn-green{background:#0d7377;color:#fff}
.btn-red{background:#e94560;color:#fff}
.toggle-row{display:flex;align-items:center;justify-content:space-between;margin-bottom:9px}
.toggle-row:last-child{margin-bottom:0}
.toggle-label{font-size:13px}
.switch{position:relative;width:42px;height:22px}
.switch input{display:none}
.slider{position:absolute;inset:0;background:#555;border-radius:22px;cursor:pointer;transition:.3s}
.slider:before{content:'';position:absolute;width:16px;height:16px;left:3px;top:3px;background:#fff;border-radius:50%;transition:.3s}
input:checked+.slider{background:#0d7377}
input:checked+.slider:before{transform:translateX(20px)}
.stat-row{display:flex;justify-content:space-between;font-size:13px;padding:4px 0;border-bottom:1px solid #0f3460}
.stat-row:last-child{border-bottom:none}
.stat-val{color:#e94560;font-weight:600}
input[type=range]{width:100%;accent-color:#e94560}
.range-row{display:flex;justify-content:space-between;font-size:12px;color:#aaa;margin-bottom:4px}
#toast{position:fixed;bottom:20px;right:20px;background:#0d7377;color:#fff;padding:9px 18px;border-radius:8px;font-size:14px;opacity:0;transition:opacity .3s;pointer-events:none;z-index:999}
#toast.show{opacity:1}
.gallery-tabs{display:flex;gap:8px;margin-bottom:14px;flex-wrap:wrap}
.tab-btn{padding:7px 16px;border-radius:7px;border:1px solid #0f3460;background:#16213e;color:#eee;cursor:pointer;font-size:13px}
.tab-btn.active{background:#e94560;border-color:#e94560}
.gallery-grid{display:flex;flex-wrap:wrap;gap:10px}
.gallery-item{background:#16213e;border:1px solid #0f3460;border-radius:8px;overflow:hidden;width:180px}
.gallery-item img,.gallery-item video{width:100%;display:block;cursor:pointer}
.item-name{font-size:11px;color:#aaa;padding:5px 7px;word-break:break-all}
.item-actions{display:flex;gap:4px;padding:0 7px 7px}
.item-btn{flex:1;padding:5px;border:none;border-radius:5px;cursor:pointer;font-size:12px}
.item-btn:hover{opacity:.8}
.item-btn-dl{background:#0f3460;color:#fff}
.item-btn-del{background:#e94560;color:#fff}
.no-files{color:#666;font-size:14px;padding:20px}
.form-row{margin-bottom:10px}
.form-row label{font-size:12px;color:#aaa;display:block;margin-bottom:4px}
.form-input{width:100%;padding:8px;border-radius:6px;border:1px solid #0f3460;background:#1a1a2e;color:#eee;font-size:13px}
.form-input:focus{outline:none;border-color:#e94560}
.rust-badge{position:absolute;top:10px;right:10px;background:rgba(180,80,0,.9);color:#fff;padding:3px 10px;border-radius:12px;font-size:12px;font-weight:bold}
@media(max-width:700px){.panel{width:100%}.video-box{min-width:unset}}
</style>
</head><body>
<header>
  <h1>摄像头监控系统 <span style="font-size:12px;color:#c86020;font-weight:normal;">Powered by Rust</span></h1>
  <div class="header-right">
    <span id="clock"></span>
    <span class="nav-link active" id="nav-monitor" onclick="showPage('monitor')">实时监控</span>
    <span class="nav-link" id="nav-gallery" onclick="showPage('gallery');loadGallery('photos')">文件管理</span>
    <span class="nav-link" id="nav-settings" onclick="showPage('settings')">设置</span>
    <a href="/logout" class="nav-link">退出</a>
  </div>
</header>

<!-- 实时监控页 -->
<div id="page-monitor">
<div class="main">
  <div class="video-box">
    <img src="/video" id="stream" alt="视频流">
    <span class="badge" id="rec-badge">● REC</span>
    <span class="rust-badge">Rust</span>
    <span id="motion-alert">移动侦测!</span>
  </div>
  <div class="panel">
    <div class="card">
      <h3>控制</h3>
      <button class="btn btn-blue" onclick="takePhoto()">拍照保存</button>
      <button class="btn btn-green" id="rec-btn" onclick="toggleRecord()">开始录像</button>
    </div>
    <div class="card">
      <h3>功能开关</h3>
      <div class="toggle-row">
        <span class="toggle-label">移动侦测</span>
        <label class="switch"><input type="checkbox" id="t-motion" onchange="toggle('motion',this.checked)"><span class="slider"></span></label>
      </div>
      <div class="toggle-row">
        <span class="toggle-label">运动门控</span>
        <label class="switch"><input type="checkbox" id="t-gate" onchange="toggle('gate',this.checked)" checked><span class="slider"></span></label>
      </div>
      <div class="toggle-row">
        <span class="toggle-label">定时截图</span>
        <label class="switch"><input type="checkbox" id="t-auto" onchange="toggle('auto',this.checked)"><span class="slider"></span></label>
      </div>
      <div style="margin-top:8px">
        <div class="range-row"><span>截图间隔</span><span id="interval-val">10s</span></div>
        <input type="range" min="5" max="120" value="10" step="5"
          oninput="document.getElementById('interval-val').textContent=this.value+'s';api('/set_interval?val='+this.value)">
      </div>
    </div>
    <div class="card">
      <h3>侦测参数</h3>
      <div class="range-row"><span>差分阈值</span><span id="sens-val">30</span></div>
      <input type="range" min="5" max="80" value="30" step="5"
        oninput="document.getElementById('sens-val').textContent=this.value;api('/set_sensitivity?val='+this.value)">
      <div style="margin-top:8px">
        <div class="range-row"><span>最小触发面积 px²</span><span id="area-val">1500</span></div>
        <input type="range" min="200" max="10000" value="1500" step="100"
          oninput="document.getElementById('area-val').textContent=this.value;api('/set_min_area?val='+this.value)">
      </div>
    </div>
    <div class="card">
      <h3>统计</h3>
      <div class="stat-row"><span>分辨率</span><span class="stat-val" id="st-res">-</span></div>
      <div class="stat-row"><span>移动触发</span><span class="stat-val" id="st-motion">0</span></div>
      <div class="stat-row"><span>拍照张数</span><span class="stat-val" id="st-photo">0</span></div>
      <div class="stat-row"><span>录像时长</span><span class="stat-val" id="st-rec">00:00</span></div>
    </div>
  </div>
</div>
</div>

<!-- 文件管理页 -->
<div id="page-gallery" style="display:none;padding:14px">
  <div class="gallery-tabs">
    <button class="tab-btn active" id="tab-photos" onclick="loadGallery('photos')">照片</button>
    <button class="tab-btn" id="tab-videos" onclick="loadGallery('videos')">录像</button>
    <button class="tab-btn" id="tab-motion" onclick="loadGallery('motion')">移动触发</button>
    <button class="tab-btn" id="tab-auto" onclick="loadGallery('auto')">定时截图</button>
    <button class="tab-btn" id="tab-alerts" onclick="loadGallery('alerts')">告警截图</button>
  </div>
  <div class="gallery-grid" id="gallery-grid"><span class="no-files">请选择分类</span></div>
</div>

<!-- 设置页 -->
<div id="page-settings" style="display:none;padding:14px">
  <div style="display:flex;gap:14px;flex-wrap:wrap">
    <div class="card" style="flex:1;min-width:300px;max-width:500px">
      <h3>邮件告警设置</h3>
      <div class="form-row">
        <label>启用邮件告警</label>
        <label class="switch" style="display:inline-block">
          <input type="checkbox" id="cfg-email-on" onchange="saveCfg()">
          <span class="slider"></span>
        </label>
      </div>
      <div class="form-row"><label>SMTP 服务器</label><input type="text" id="cfg-smtp-host" class="form-input" value="smtp.gmail.com"></div>
      <div class="form-row"><label>SMTP 端口</label><input type="number" id="cfg-smtp-port" class="form-input" value="465"></div>
      <div class="form-row"><label>发件人邮箱</label><input type="email" id="cfg-email-from" class="form-input"></div>
      <div class="form-row"><label>邮箱密码/应用密码</label><input type="password" id="cfg-email-pass" class="form-input" placeholder="留空不修改"></div>
      <div class="form-row"><label>收件人邮箱</label><input type="email" id="cfg-email-to" class="form-input"></div>
      <div class="form-row"><label>告警冷却时间（秒）</label><input type="number" id="cfg-cooldown" class="form-input" value="60"></div>
      <div class="form-row">
        <label>触发条件</label>
        <label style="font-size:13px"><input type="checkbox" id="cfg-on-motion" checked> 移动侦测</label>
      </div>
      <button class="btn btn-green" onclick="saveCfg()">保存设置</button>
      <button class="btn btn-blue" onclick="testEmail()" style="margin-top:0">发送测试邮件</button>
      <p id="cfg-status" style="font-size:12px;color:#aaa;margin-top:8px"></p>
    </div>
    <div class="card" style="flex:1;min-width:300px;max-width:400px">
      <h3>系统信息</h3>
      <div class="stat-row"><span>引擎</span><span class="stat-val">Rust + V4L2</span></div>
      <div class="stat-row"><span>运动侦测</span><span class="stat-val">帧差算法</span></div>
      <div class="stat-row"><span>视频格式</span><span class="stat-val">MJPEG AVI</span></div>
      <div class="stat-row"><span>版本</span><span class="stat-val">1.0.0</span></div>
    </div>
  </div>
</div>

<div id="toast"></div>

<script>
let recording=false,recStart=null,recTimer=null,photoCount=0;

function showPage(name){
  ['monitor','gallery','settings'].forEach(p=>{
    document.getElementById('page-'+p).style.display=p===name?'':'none';
    document.getElementById('nav-'+p)?.classList.toggle('active',p===name);
  });
}

function toast(msg,color='#0d7377'){
  const el=document.getElementById('toast');
  el.style.background=color;el.textContent=msg;
  el.classList.add('show');setTimeout(()=>el.classList.remove('show'),2500);
}

function api(url){return fetch(url).then(r=>r.json());}

function takePhoto(){
  api('/photo').then(d=>{
    if(d.ok){photoCount++;document.getElementById('st-photo').textContent=photoCount;toast('截图已保存');}
    else toast('截图失败','#e94560');
  });
}

function toggleRecord(){
  api('/record').then(d=>{
    recording=d.recording;
    const btn=document.getElementById('rec-btn'),badge=document.getElementById('rec-badge');
    if(recording){
      btn.textContent='停止录像';btn.className='btn btn-red';
      badge.classList.add('show');recStart=Date.now();
      recTimer=setInterval(updateRecTime,1000);toast('开始录像');
    } else {
      btn.textContent='开始录像';btn.className='btn btn-green';
      badge.classList.remove('show');clearInterval(recTimer);
      document.getElementById('st-rec').textContent='00:00';
      toast('录像已保存');
    }
  });
}

function updateRecTime(){
  const s=Math.floor((Date.now()-recStart)/1000);
  document.getElementById('st-rec').textContent=
    String(Math.floor(s/60)).padStart(2,'0')+':'+String(s%60).padStart(2,'0');
}

function toggle(name,on){
  api('/toggle?name='+name+'&on='+(on?1:0)).then(d=>{
    toast(name+' '+(on?'已开启':'已关闭'));
  });
}

// 统计轮询
setInterval(()=>{
  fetch('/stats').then(r=>r.json()).then(d=>{
    document.getElementById('st-res').textContent=d.resolution;
    document.getElementById('st-motion').textContent=d.motion_count;
    const ma=document.getElementById('motion-alert');
    if(d.motion_now){ma.style.display='block';setTimeout(()=>ma.style.display='none',1500);}
  });
},1000);

setInterval(()=>{document.getElementById('clock').textContent=new Date().toLocaleString('zh-CN');},1000);

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
      div.innerHTML=(isVid?`<video controls><source src="/file/${type}/${f}"></video>`
        :`<img src="/file/${type}/${f}" onclick="window.open(this.src)">`)
        +`<div class="item-name">${f}</div>
          <div class="item-actions">
            <a href="/file/${type}/${f}" download class="item-btn item-btn-dl">下载</a>
            <button class="item-btn item-btn-del" onclick="delFile('${type}','${f}',this.closest('.gallery-item'))">删除</button>
          </div>`;
      grid.appendChild(div);
    });
  });
}

function delFile(type,name,el){
  if(!confirm('确认删除 '+name+'?'))return;
  fetch('/delete?type='+type+'&name='+encodeURIComponent(name),{method:'POST'})
    .then(r=>r.json()).then(d=>{if(d.ok){el.remove();toast('已删除');}else toast('删除失败','#e94560');});
}

// 设置
function saveCfg(){
  const data={
    email_enabled:document.getElementById('cfg-email-on').checked,
    smtp_host:document.getElementById('cfg-smtp-host').value,
    smtp_port:parseInt(document.getElementById('cfg-smtp-port').value),
    email_from:document.getElementById('cfg-email-from').value,
    email_password:document.getElementById('cfg-email-pass').value,
    email_to:document.getElementById('cfg-email-to').value,
    cooldown:parseInt(document.getElementById('cfg-cooldown').value),
    on_motion:document.getElementById('cfg-on-motion').checked,
    on_unknown:false
  };
  fetch('/save_config',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(data)})
    .then(r=>r.json()).then(d=>{
      if(d.ok){document.getElementById('cfg-status').textContent='已保存';toast('设置已保存');}
    });
}

function testEmail(){
  fetch('/test_email').then(r=>r.json()).then(d=>{
    if(d.ok)toast('测试邮件已发送');
    else toast('发送失败: '+d.error,'#e94560');
  });
}
</script>
</body></html>
"#;
