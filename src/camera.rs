/*!
 * 摄像头捕获模块（跨平台：Linux V4L2 / Windows Media Foundation）
 * 支持：摄像头枚举、热切换、亮度/对比度/饱和度/翻转/旋转
 */

use crate::motion;
use crate::state::{AppState, ImageSettings, MotionZone};
use image::{imageops, DynamicImage, ImageBuffer, Rgb};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ============================================================
//  摄像头枚举
// ============================================================

pub fn list_cameras() -> Vec<(u32, String)> {
    use nokhwa::utils::{ApiBackend, CameraIndex};
    match nokhwa::query(ApiBackend::Auto) {
        Ok(cameras) => cameras
            .into_iter()
            .filter_map(|info| {
                if let CameraIndex::Index(idx) = info.index() {
                    Some((*idx, info.human_name().to_string()))
                } else {
                    None
                }
            })
            .collect(),
        Err(e) => {
            tracing::warn!("枚举摄像头失败: {} — 返回空列表", e);
            vec![]
        }
    }
}

// ============================================================
//  JPEG 编解码
// ============================================================

pub fn encode_jpeg(rgb: &[u8], width: u32, height: u32, quality: u8) -> Vec<u8> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width, height, rgb.to_vec()).expect("无效 RGB 缓冲区");
    let mut buf = Vec::new();
    DynamicImage::ImageRgb8(img)
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut buf, quality,
        ))
        .expect("JPEG 编码失败");
    buf
}

pub fn decode_jpeg(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::load_from_memory_with_format(data, image::ImageFormat::Jpeg).ok()?;
    let rgb = img.to_rgb8();
    let (w, h) = (rgb.width(), rgb.height());
    Some((w, h, rgb.into_raw()))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn ts_str() -> String {
    chrono::Local::now().format("%Y%m%d_%H%M%S").to_string()
}

// ============================================================
//  图像处理
// ============================================================

fn apply_brightness(rgb: &mut [u8], brightness: i32) {
    if brightness == 0 {
        return;
    }
    for p in rgb.iter_mut() {
        *p = (*p as i32 + brightness).max(0).min(255) as u8;
    }
}

fn apply_contrast(rgb: &mut [u8], contrast: i32) {
    if contrast == 0 {
        return;
    }
    let factor = (259.0 * (contrast as f32 + 255.0)) / (255.0 * (259.0 - contrast as f32));
    for p in rgb.iter_mut() {
        *p = (factor * (*p as f32 - 128.0) + 128.0).max(0.0).min(255.0) as u8;
    }
}

fn apply_saturation(rgb: &mut [u8], saturation: i32) {
    if saturation == 0 {
        return;
    }
    let factor = 1.0 + saturation as f32 / 100.0;
    for chunk in rgb.chunks_exact_mut(3) {
        let r = chunk[0] as f32;
        let g = chunk[1] as f32;
        let b = chunk[2] as f32;
        let gray = 0.299 * r + 0.587 * g + 0.114 * b;
        chunk[0] = (gray + factor * (r - gray)).max(0.0).min(255.0) as u8;
        chunk[1] = (gray + factor * (g - gray)).max(0.0).min(255.0) as u8;
        chunk[2] = (gray + factor * (b - gray)).max(0.0).min(255.0) as u8;
    }
}

fn apply_flip_h(rgb: &mut [u8], width: u32, height: u32) {
    let (w, h) = (width as usize, height as usize);
    for y in 0..h {
        for x in 0..w / 2 {
            let a = (y * w + x) * 3;
            let b = (y * w + (w - 1 - x)) * 3;
            for c in 0..3usize {
                rgb.swap(a + c, b + c);
            }
        }
    }
}

fn apply_flip_v(rgb: &mut [u8], width: u32, height: u32) {
    let (w, h) = (width as usize, height as usize);
    for y in 0..h / 2 {
        for x in 0..w {
            let a = (y * w + x) * 3;
            let b = ((h - 1 - y) * w + x) * 3;
            for c in 0..3usize {
                rgb.swap(a + c, b + c);
            }
        }
    }
}

fn apply_rotation(rgb: Vec<u8>, width: u32, height: u32, degrees: u32) -> (Vec<u8>, u32, u32) {
    if degrees == 0 {
        return (rgb, width, height);
    }
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = match ImageBuffer::from_raw(width, height, rgb) {
        Some(i) => i,
        None => return (vec![], width, height),
    };
    let rotated = match degrees {
        90 => imageops::rotate90(&img),
        180 => imageops::rotate180(&img),
        270 => imageops::rotate270(&img),
        _ => return (img.into_raw(), width, height),
    };
    let (nw, nh) = (rotated.width(), rotated.height());
    (rotated.into_raw(), nw, nh)
}

/// 将 RGB 帧缩放到不超过 max_w x max_h，保持宽高比
fn scale_down_rgb(rgb: &mut Vec<u8>, w: &mut u32, h: &mut u32, max_w: u32, max_h: u32) {
    if *w <= max_w && *h <= max_h {
        return;
    }
    let scale = (max_w as f32 / *w as f32).min(max_h as f32 / *h as f32);
    let nw = ((*w as f32 * scale) as u32).max(1);
    let nh = ((*h as f32 * scale) as u32).max(1);
    if let Some(img) = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(*w, *h, std::mem::take(rgb)) {
        let resized = imageops::resize(&img, nw, nh, imageops::FilterType::Triangle);
        *rgb = resized.into_raw();
        *w = nw;
        *h = nh;
    }
}

pub fn apply_image_settings(
    rgb: &mut Vec<u8>,
    width: &mut u32,
    height: &mut u32,
    s: &ImageSettings,
) {
    apply_brightness(rgb, s.brightness);
    apply_contrast(rgb, s.contrast);
    apply_saturation(rgb, s.saturation);
    if s.flip_h {
        apply_flip_h(rgb, *width, *height);
    }
    if s.flip_v {
        apply_flip_v(rgb, *width, *height);
    }
    if s.rotation != 0 {
        let (new_rgb, nw, nh) = apply_rotation(std::mem::take(rgb), *width, *height, s.rotation);
        *rgb = new_rgb;
        *width = nw;
        *height = nh;
    }
}

// ============================================================
//  时间戳水印
// ============================================================

pub fn overlay_timestamp(rgb: &mut [u8], width: u32, height: u32) {
    let text = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let font = tiny_font();
    let (cw, ch) = (6usize, 8usize);
    let (x0, y0) = (10usize, (height as usize).saturating_sub(ch + 8));
    let (w, h) = (width as usize, height as usize);
    for (ci, ch_val) in text.chars().enumerate() {
        let Some(glyph) = font.get(&ch_val) else {
            continue;
        };
        let cx = x0 + ci * cw;
        for row in 0..ch {
            for col in 0..cw {
                if glyph[row] & (1 << (5 - col)) != 0 {
                    let (px, py) = (cx + col, y0 + row);
                    if px < w && py < h {
                        let b = (py * w + px) * 3;
                        rgb[b] = 255;
                        rgb[b + 1] = 255;
                        rgb[b + 2] = 255;
                    }
                }
            }
        }
    }
}

fn tiny_font() -> std::collections::HashMap<char, [u8; 8]> {
    let mut m = std::collections::HashMap::new();
    m.insert(
        '0',
        [
            0b011110u8, 0b110011, 0b110011, 0b110011, 0b110011, 0b110011, 0b011110, 0,
        ],
    );
    m.insert(
        '1',
        [
            0b001100, 0b011100, 0b001100, 0b001100, 0b001100, 0b001100, 0b111111, 0,
        ],
    );
    m.insert(
        '2',
        [
            0b011110, 0b110011, 0b000011, 0b000110, 0b011100, 0b110000, 0b111111, 0,
        ],
    );
    m.insert(
        '3',
        [
            0b011110, 0b110011, 0b000011, 0b001110, 0b000011, 0b110011, 0b011110, 0,
        ],
    );
    m.insert(
        '4',
        [
            0b000110, 0b001110, 0b011110, 0b110110, 0b111111, 0b000110, 0b000110, 0,
        ],
    );
    m.insert(
        '5',
        [
            0b111111, 0b110000, 0b111110, 0b000011, 0b000011, 0b110011, 0b011110, 0,
        ],
    );
    m.insert(
        '6',
        [
            0b011110, 0b110011, 0b110000, 0b111110, 0b110011, 0b110011, 0b011110, 0,
        ],
    );
    m.insert(
        '7',
        [
            0b111111, 0b000011, 0b000110, 0b001100, 0b001100, 0b001100, 0b001100, 0,
        ],
    );
    m.insert(
        '8',
        [
            0b011110, 0b110011, 0b110011, 0b011110, 0b110011, 0b110011, 0b011110, 0,
        ],
    );
    m.insert(
        '9',
        [
            0b011110, 0b110011, 0b110011, 0b011111, 0b000011, 0b110011, 0b011110, 0,
        ],
    );
    m.insert('-', [0, 0, 0, 0b111111, 0, 0, 0, 0]);
    m.insert(' ', [0; 8]);
    m.insert(':', [0, 0b001100, 0b001100, 0, 0b001100, 0b001100, 0, 0]);
    m
}

// ============================================================
//  运动区域过滤
// ============================================================

/// 将 RGB 帧中区域外的像素置灰，只保留检测区域内的变化
pub fn apply_motion_zones(rgb: &mut [u8], width: u32, height: u32, zones: &[MotionZone]) {
    if zones.is_empty() {
        return;
    }
    let (w, h) = (width as usize, height as usize);
    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 / w as f32;
            let fy = y as f32 / h as f32;
            let in_zone = zones
                .iter()
                .any(|z| fx >= z.x1 && fx <= z.x2 && fy >= z.y1 && fy <= z.y2);
            if !in_zone {
                let b = (y * w + x) * 3;
                // 置为灰色，消除区域外的运动
                let avg = ((rgb[b] as u32 + rgb[b + 1] as u32 + rgb[b + 2] as u32) / 3) as u8;
                rgb[b] = avg;
                rgb[b + 1] = avg;
                rgb[b + 2] = avg;
            }
        }
    }
}

// ============================================================
//  捕获循环（支持摄像头热切换 + 多摄像头独立流）
// ============================================================

/// 单摄像头捕获循环（每个摄像头各启动一个）
pub async fn capture_loop(state: AppState) {
    tokio::task::spawn_blocking(move || {
        run_nokhwa_loop(state);
    })
    .await
    .ok();
}

/// 为指定索引的摄像头单独启动捕获（多摄同时预览）
pub async fn capture_loop_for(state: AppState, idx: usize) {
    let s = state.clone();
    tokio::task::spawn_blocking(move || run_nokhwa_loop_idx(s, idx))
        .await
        .ok();
}

fn run_nokhwa_loop(state: AppState) {
    run_nokhwa_loop_idx(state, usize::MAX);
}

fn run_nokhwa_loop_idx(state: AppState, fixed_idx: usize) {
    use nokhwa::pixel_format::RgbFormat;
    use nokhwa::{
        utils::{CameraIndex, RequestedFormat, RequestedFormatType},
        Camera,
    };

    loop {
        // fixed_idx: 独立流模式固定不变；主流模式跟随 camera_idx
        let target_idx = if fixed_idx == usize::MAX {
            state.camera_idx.load(Ordering::Relaxed)
        } else {
            fixed_idx
        };
        let is_primary =
            fixed_idx == usize::MAX || fixed_idx == state.camera_idx.load(Ordering::Relaxed);
        let index = CameraIndex::Index(target_idx as u32);
        let requested =
            RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);

        let mut camera = match Camera::new(index, requested) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("无法打开摄像头 {}: {} — 使用占位帧", target_idx, e);
                run_placeholder_briefly(&state, target_idx);
                continue;
            }
        };

        if let Err(e) = camera.open_stream() {
            tracing::warn!("无法打开流 {}: {} — 使用占位帧", target_idx, e);
            run_placeholder_briefly(&state, target_idx);
            continue;
        }

        let fmt = camera.camera_format();
        let (cam_w, cam_h) = (fmt.width(), fmt.height());
        if is_primary {
            state.camera.lock().resolution = (cam_w, cam_h);
        }
        tracing::info!("摄像头 {} 就绪 {}x{}", target_idx, cam_w, cam_h);

        // 注册帧通道
        {
            let mut txs = state.frame_txs.lock();
            if !txs.contains_key(&target_idx) {
                let (tx, _) = tokio::sync::broadcast::channel(4);
                txs.insert(target_idx, tx);
            }
        }

        loop {
            // 主流模式：检测切换请求
            if fixed_idx == usize::MAX && state.camera_idx.load(Ordering::Relaxed) != target_idx {
                let _ = camera.stop_stream();
                tracing::info!("检测到摄像头切换请求，重新初始化...");
                break;
            }

            let frame = match camera.frame() {
                Ok(f) => f,
                Err(e) => {
                    tracing::warn!("读帧失败: {}", e);
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
            };

            let rgb_image = match frame.decode_image::<RgbFormat>() {
                Ok(img) => img,
                Err(_) => continue,
            };

            let (mut w, mut h) = (rgb_image.width(), rgb_image.height());
            let mut rgb = rgb_image.into_raw();

            // 降分辨率上限 1280x720，减少编码耗时和流量
            scale_down_rgb(&mut rgb, &mut w, &mut h, 1280, 720);

            let img_settings = state.image_settings.read().clone();
            apply_image_settings(&mut rgb, &mut w, &mut h, &img_settings);

            if is_primary {
                // 主流：运动检测 + 录像 + 广播（内部一次编码，同时发往 frame_tx 和 frame_txs）
                process_and_broadcast(&state, &mut rgb, w, h, target_idx);
            } else {
                // 非主摄像头：只编码并发往对应分流通道
                overlay_timestamp(&mut rgb, w, h);
                let jpeg = encode_jpeg(&rgb, w, h, 75);
                let arc = Arc::new(jpeg);
                let txs = state.frame_txs.lock();
                if let Some(tx) = txs.get(&target_idx) {
                    let _ = tx.send(arc);
                }
            }

            // 更新 FPS
            {
                let mut cam = state.camera.lock();
                cam.fps_frame_count += 1;
                let elapsed = cam.fps_last_ts.elapsed().as_secs_f32();
                if elapsed >= 1.0 {
                    cam.fps_current = cam.fps_frame_count as f32 / elapsed;
                    cam.fps_frame_count = 0;
                    cam.fps_last_ts = std::time::Instant::now();
                }
            }
        }
    }
}

fn run_placeholder_briefly(state: &AppState, expected_idx: usize) {
    let (w, h) = (640u32, 480u32);
    state.camera.lock().resolution = (w, h);
    for _ in 0..25 {
        if state.camera_idx.load(Ordering::Relaxed) != expected_idx {
            return;
        }
        let mut rgb = vec![30u8; (w * h * 3) as usize];
        overlay_timestamp(&mut rgb, w, h);
        let jpeg = encode_jpeg(&rgb, w, h, 70);
        let arc = Arc::new(jpeg);
        state.camera.lock().latest_jpeg = Some(arc.clone());
        let _ = state.frame_tx.send(arc);
        std::thread::sleep(Duration::from_millis(40));
    }
}

fn process_and_broadcast(state: &AppState, rgb: &mut Vec<u8>, w: u32, h: u32, cam_idx: usize) {
    use std::sync::atomic::AtomicU32;
    static SKIP_CTR: AtomicU32 = AtomicU32::new(0);
    let ctr = SKIP_CTR.fetch_add(1, Ordering::Relaxed);

    let (sensitivity, min_area, motion_detect, frame_skip) = {
        let cam = state.camera.lock();
        (
            cam.sensitivity,
            cam.min_area,
            cam.motion_detect,
            cam.frame_skip.max(1),
        )
    };
    let should_detect = motion_detect && ctr % frame_skip == 0;
    let prev_gray = state.camera.lock().prev_gray.clone();

    let (contours, detected, new_gray) = if should_detect {
        let zones = state.motion_zones.read().clone();
        let mut rgb_for_motion = rgb.clone();
        if !zones.is_empty() {
            apply_motion_zones(&mut rgb_for_motion, w, h, &zones);
        }
        motion::detect_motion(&mut rgb_for_motion, w, h, &prev_gray, sensitivity, min_area)
    } else {
        let g = motion::to_grayscale(rgb, w, h);
        (vec![], false, g)
    };

    if detected && !contours.is_empty() {
        let rects: Vec<(u32, u32, u32, u32)> =
            contours.iter().map(|r| (r.x, r.y, r.w, r.h)).collect();
        crate::heatmap::record_motion(state, &rects, w, h);
    }

    overlay_timestamp(rgb, w, h);
    let jpeg = encode_jpeg(rgb, w, h, 75);
    let jpeg_arc = Arc::new(jpeg.clone());

    // 一次编码，同时发往主广播和该摄像头的分流通道
    {
        let txs = state.frame_txs.lock();
        if let Some(tx) = txs.get(&cam_idx) {
            let _ = tx.send(jpeg_arc.clone());
        }
    }

    {
        let mut cam = state.camera.lock();
        cam.latest_jpeg = Some(jpeg_arc.clone());
        cam.prev_gray = Some(new_gray);
        cam.motion_now = motion_detect && detected;

        if motion_detect && detected {
            let t = now_secs();
            if t - cam.last_motion_save > 2 {
                let path = format!("{}/motion/{}.jpg", crate::SAVE_DIR, ts_str());
                std::fs::write(&path, &jpeg).ok();
                cam.motion_count += 1;
                cam.last_motion_save = t;

                let motion_count = cam.motion_count;
                let should_send_email = {
                    let mut ecfg = state.email_cfg.lock();
                    if ecfg.enabled
                        && ecfg.on_motion
                        && now_secs() - ecfg.last_sent >= ecfg.cooldown
                    {
                        ecfg.last_sent = now_secs();
                        true
                    } else {
                        false
                    }
                };
                if should_send_email {
                    let cfg = state.email_cfg.lock().clone();
                    let p = path.clone();
                    std::thread::spawn(move || {
                        crate::email::send_motion_alert_direct(&cfg, motion_count, &p).ok();
                    });
                }

                // WebSocket 事件
                let msg =
                    serde_json::json!({"event":"motion","count":cam.motion_count}).to_string();
                state.ws_tx.send(msg).ok();

                // 记录事件日志
                {
                    let evt = crate::events::Event::new(
                        0,
                        crate::events::EventKind::Motion,
                        cam_idx,
                        format!("检测到移动 #{}", cam.motion_count),
                    )
                    .with_thumb(format!("motion/{}.jpg", ts_str()));
                    state.event_log.log(evt);
                }

                // 异步：上传 + 多渠道通知（检查告警时间规则）
                if state.alert_allowed() {
                    let notify_cfg = state.notify_cfg.lock().clone();
                    let od_cfg = state.onedrive_cfg.lock().clone();
                    let gd_cfg = state.gdrive_cfg.lock().clone();
                    let ftp_cfg = state.ftp_cfg.lock().clone();
                    let jpeg_copy = jpeg.clone();
                    let filename = format!("motion/{}.jpg", ts_str());
                    let count = cam.motion_count;
                    tokio::spawn(async move {
                        let share_url = crate::upload::upload_all(
                            &od_cfg,
                            &gd_cfg,
                            &ftp_cfg,
                            &filename,
                            &jpeg_copy,
                            crate::upload::UploadKind::Motion,
                        )
                        .await;
                        crate::notify::send_all(
                            &notify_cfg,
                            crate::notify::NotifyEvent::Motion {
                                count,
                                image: &jpeg_copy,
                                image_url: share_url.as_deref(),
                            },
                        )
                        .await;
                    });
                }
            }
        }

        if cam.recording {
            cam.record_frames.push(jpeg.clone());
            // 录像保护：达到时长/大小上限后自动分段保存，防止内存无限增长
            let limits = state.record_limits.lock().clone();
            let elapsed = cam
                .record_start
                .map(|t| now_secs().saturating_sub(t))
                .unwrap_or(0);
            let est_bytes: u64 = cam.record_frames.iter().map(|f| f.len() as u64).sum();
            let dur_hit = limits.auto_split
                && limits.max_duration_secs > 0
                && elapsed >= limits.max_duration_secs;
            let size_hit = limits.auto_split
                && limits.max_size_mb > 0
                && est_bytes >= limits.max_size_mb * 1024 * 1024;
            if dur_hit || size_hit {
                let frames = std::mem::take(&mut cam.record_frames);
                cam.record_start = Some(now_secs());
                let path = format!("{}/videos/{}.avi", crate::SAVE_DIR, ts_str());
                tracing::info!(
                    "录像自动分段：{} 帧 / {} MB（原因：{}）",
                    frames.len(),
                    est_bytes / 1024 / 1024,
                    if dur_hit {
                        "时长上限"
                    } else {
                        "大小上限"
                    }
                );
                tokio::task::spawn_blocking(move || {
                    crate::camera::save_mjpeg_avi(&frames, crate::RECORD_FPS, &path).ok();
                });
            }
        }
    }

    let _ = state.frame_tx.send(jpeg_arc);
}

// ============================================================
//  MJPEG AVI 录像写入
// ============================================================

pub fn save_mjpeg_avi(frames: &[Vec<u8>], fps: f64, path: &str) -> std::io::Result<()> {
    if frames.is_empty() {
        return Ok(());
    }
    let frame_count = frames.len() as u32;
    let fps_u = fps as u32;
    let us_per_frame = (1_000_000.0 / fps) as u32;
    let (width, height) = frames
        .first()
        .and_then(|f| decode_jpeg(f))
        .map(|(w, h, _)| (w, h))
        .unwrap_or((0, 0));

    let mut movi: Vec<u8> = Vec::new();
    let mut offsets: Vec<(u32, u32)> = Vec::new();
    for f in frames {
        let off = mowi_len(&movi);
        mowi_tag(&mut movi, b"00dc");
        let sz = f.len() as u32;
        mowi_u32(&mut movi, sz);
        offsets.push((off + 8, sz));
        movi.extend_from_slice(f);
        if sz % 2 != 0 {
            movi.push(0);
        }
    }

    let mut buf: Vec<u8> = Vec::with_capacity(4096 + movi.len());

    mowi_tag(&mut buf, b"RIFF");
    let riff_pos = buf.len();
    mowi_u32(&mut buf, 0);
    mowi_tag(&mut buf, b"AVI ");

    mowi_tag(&mut buf, b"LIST");
    let hdrl_pos = buf.len();
    mowi_u32(&mut buf, 0);
    mowi_tag(&mut buf, b"hdrl");

    mowi_tag(&mut buf, b"avih");
    mowi_u32(&mut buf, 56);
    mowi_u32(&mut buf, us_per_frame);
    mowi_u32(&mut buf, 0);
    mowi_u32(&mut buf, 0);
    mowi_u32(&mut buf, 0x0010);
    mowi_u32(&mut buf, frame_count);
    for _ in 0..3 {
        mowi_u32(&mut buf, 0);
    }
    {
        let n = buf.len() - 4;
        buf[n..n + 4].copy_from_slice(&1u32.to_le_bytes());
    }
    for _ in 0..6 {
        mowi_u32(&mut buf, 0);
    }

    mowi_tag(&mut buf, b"LIST");
    let strl_pos = buf.len();
    mowi_u32(&mut buf, 0);
    mowi_tag(&mut buf, b"strl");

    mowi_tag(&mut buf, b"strh");
    mowi_u32(&mut buf, 56);
    mowi_tag(&mut buf, b"vids");
    mowi_tag(&mut buf, b"MJPG");
    mowi_u32(&mut buf, 0);
    mowi_u32(&mut buf, 0);
    mowi_u32(&mut buf, 0);
    mowi_u32(&mut buf, 1);
    mowi_u32(&mut buf, fps_u);
    mowi_u32(&mut buf, 0);
    mowi_u32(&mut buf, frame_count);
    mowi_u32(&mut buf, 0);
    mowi_u32(&mut buf, u32::MAX);
    mowi_u32(&mut buf, 0);
    for _ in 0..4 {
        buf.extend_from_slice(&0u16.to_le_bytes());
    }

    mowi_tag(&mut buf, b"strf");
    mowi_u32(&mut buf, 40);
    mowi_u32(&mut buf, 40);
    mowi_u32(&mut buf, width);
    mowi_u32(&mut buf, height);
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&24u16.to_le_bytes());
    mowi_tag(&mut buf, b"MJPG");
    for _ in 0..5 {
        mowi_u32(&mut buf, 0);
    }

    let strl_sz = (buf.len() - strl_pos - 4) as u32;
    buf[strl_pos..strl_pos + 4].copy_from_slice(&strl_sz.to_le_bytes());
    let hdrl_sz = (buf.len() - hdrl_pos - 4) as u32;
    buf[hdrl_pos..hdrl_pos + 4].copy_from_slice(&hdrl_sz.to_le_bytes());

    mowi_tag(&mut buf, b"LIST");
    mowi_u32(&mut buf, (4 + movi.len()) as u32);
    mowi_tag(&mut buf, b"movi");
    let movi_start = buf.len() as u32;
    buf.extend_from_slice(&movi);

    mowi_tag(&mut buf, b"idx1");
    mowi_u32(&mut buf, frame_count * 16);
    for (off, sz) in &offsets {
        mowi_tag(&mut buf, b"00dc");
        mowi_u32(&mut buf, 0x10);
        mowi_u32(&mut buf, movi_start + off - 8);
        mowi_u32(&mut buf, *sz);
    }

    let riff_sz = (buf.len() - riff_pos - 4) as u32;
    buf[riff_pos..riff_pos + 4].copy_from_slice(&riff_sz.to_le_bytes());

    std::fs::write(path, &buf)
}

#[inline]
fn mowi_len(v: &[u8]) -> u32 {
    v.len() as u32
}
#[inline]
fn mowi_tag(v: &mut Vec<u8>, t: &[u8; 4]) {
    v.extend_from_slice(t);
}
#[inline]
fn mowi_u32(v: &mut Vec<u8>, n: u32) {
    v.extend_from_slice(&n.to_le_bytes());
}
