/*!
 * 摄像头捕获模块（Linux V4L2）
 */

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use image::{DynamicImage, ImageBuffer, Rgb};
use crate::state::AppState;
use crate::motion;

// ============================================================
//  JPEG 编解码
// ============================================================

pub fn encode_jpeg(rgb: &[u8], width: u32, height: u32, quality: u8) -> Vec<u8> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width, height, rgb.to_vec())
        .expect("无效 RGB 缓冲区");
    let mut buf = Vec::new();
    DynamicImage::ImageRgb8(img)
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality))
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
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub fn ts_str() -> String {
    chrono::Local::now().format("%Y%m%d_%H%M%S").to_string()
}

// ============================================================
//  时间戳水印（极简 bitmap 字体，6×8 像素/字符）
// ============================================================

pub fn overlay_timestamp(rgb: &mut [u8], width: u32, height: u32) {
    let text = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let font = tiny_font();
    let (cw, ch) = (6usize, 8usize);
    let (x0, y0) = (10usize, (height as usize).saturating_sub(ch + 8));
    let (w, h) = (width as usize, height as usize);
    for (ci, ch_val) in text.chars().enumerate() {
        let Some(glyph) = font.get(&ch_val) else { continue };
        let cx = x0 + ci * cw;
        for row in 0..ch {
            for col in 0..cw {
                if glyph[row] & (1 << (5 - col)) != 0 {
                    let (px, py) = (cx + col, y0 + row);
                    if px < w && py < h {
                        let b = (py * w + px) * 3;
                        rgb[b] = 255; rgb[b+1] = 255; rgb[b+2] = 255;
                    }
                }
            }
        }
    }
}

fn tiny_font() -> std::collections::HashMap<char, [u8; 8]> {
    let mut m = std::collections::HashMap::new();
    m.insert('0', [0b011110u8, 0b110011, 0b110011, 0b110011, 0b110011, 0b110011, 0b011110, 0]);
    m.insert('1', [0b001100, 0b011100, 0b001100, 0b001100, 0b001100, 0b001100, 0b111111, 0]);
    m.insert('2', [0b011110, 0b110011, 0b000011, 0b000110, 0b011100, 0b110000, 0b111111, 0]);
    m.insert('3', [0b011110, 0b110011, 0b000011, 0b001110, 0b000011, 0b110011, 0b011110, 0]);
    m.insert('4', [0b000110, 0b001110, 0b011110, 0b110110, 0b111111, 0b000110, 0b000110, 0]);
    m.insert('5', [0b111111, 0b110000, 0b111110, 0b000011, 0b000011, 0b110011, 0b011110, 0]);
    m.insert('6', [0b011110, 0b110011, 0b110000, 0b111110, 0b110011, 0b110011, 0b011110, 0]);
    m.insert('7', [0b111111, 0b000011, 0b000110, 0b001100, 0b001100, 0b001100, 0b001100, 0]);
    m.insert('8', [0b011110, 0b110011, 0b110011, 0b011110, 0b110011, 0b110011, 0b011110, 0]);
    m.insert('9', [0b011110, 0b110011, 0b110011, 0b011111, 0b000011, 0b110011, 0b011110, 0]);
    m.insert('-', [0, 0, 0, 0b111111, 0, 0, 0, 0]);
    m.insert(' ', [0; 8]);
    m.insert(':', [0, 0b001100, 0b001100, 0, 0b001100, 0b001100, 0, 0]);
    m
}

// ============================================================
//  捕获循环入口
// ============================================================

pub async fn capture_loop(state: AppState) {
    tokio::task::spawn_blocking(move || {
        run_nokhwa_loop(state);
    }).await.ok();
}

fn run_nokhwa_loop(state: AppState) {
    use nokhwa::{Camera, utils::{CameraIndex, RequestedFormat, RequestedFormatType}};
    use nokhwa::pixel_format::RgbFormat;

    let index = CameraIndex::Index(crate::CAMERA_INDEX as u32);
    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);

    let mut camera = match Camera::new(index, requested) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("无法打开摄像头: {} — 使用占位帧", e);
            run_placeholder_loop(state);
            return;
        }
    };

    if let Err(e) = camera.open_stream() {
        tracing::warn!("无法打开摄像头流: {} — 使用占位帧", e);
        run_placeholder_loop(state);
        return;
    }

    let fmt = camera.camera_format();
    let (cam_w, cam_h) = (fmt.width(), fmt.height());
    state.camera.lock().resolution = (cam_w, cam_h);
    tracing::info!("摄像头就绪 {}x{}", cam_w, cam_h);

    loop {
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
            Err(e) => {
                tracing::warn!("解码帧失败: {}", e);
                continue;
            }
        };

        let (w, h) = (rgb_image.width(), rgb_image.height());
        let mut rgb = rgb_image.into_raw();
        process_and_broadcast(&state, &mut rgb, w, h);
    }
}

fn process_and_broadcast(state: &AppState, rgb: &mut Vec<u8>, w: u32, h: u32) {
    let (sensitivity, min_area, motion_detect) = {
        let cam = state.camera.lock();
        (cam.sensitivity, cam.min_area, cam.motion_detect)
    };
    let prev_gray = state.camera.lock().prev_gray.clone();

    let (_, detected, new_gray) = if motion_detect {
        motion::detect_motion(rgb, w, h, &prev_gray, sensitivity, min_area)
    } else {
        let g = motion::to_grayscale(rgb, w, h);
        (vec![], false, g)
    };

    overlay_timestamp(rgb, w, h);
    let jpeg = encode_jpeg(rgb, w, h, 80);
    let jpeg_arc = Arc::new(jpeg.clone());

    {
        let mut cam = state.camera.lock();
        cam.latest_jpeg = Some(jpeg_arc.clone());
        cam.prev_gray   = Some(new_gray);
        cam.motion_now  = motion_detect && detected;

        if motion_detect && detected {
            let t = now_secs();
            if t - cam.last_motion_save > 2 {
                let path = format!("{}/motion/{}.jpg", crate::SAVE_DIR, ts_str());
                std::fs::write(&path, &jpeg).ok();
                cam.motion_count += 1;
                cam.last_motion_save = t;

                let (enabled, on_motion) = {
                    let ecfg = state.email_cfg.lock();
                    (ecfg.enabled, ecfg.on_motion)
                };
                if enabled && on_motion {
                    let cfg = state.email_cfg.lock().clone();
                    let count = cam.motion_count;
                    let p = path.clone();
                    std::thread::spawn(move || {
                        crate::email::send_motion_alert_blocking(cfg, count, &p).ok();
                    });
                }
            }
        }

        if cam.recording {
            cam.record_frames.push(jpeg.clone());
        }
    }

    let _ = state.frame_tx.send(jpeg_arc);
}

fn run_placeholder_loop(state: AppState) {
    let (w, h) = (640u32, 480u32);
    state.camera.lock().resolution = (w, h);
    loop {
        let mut rgb = vec![0u8; (w * h * 3) as usize];
        for y in 0..h as usize {
            for x in 0..w as usize {
                let shade: u8 = if (x / 32 + y / 32) % 2 == 0 { 40 } else { 60 };
                let b = (y * w as usize + x) * 3;
                rgb[b] = shade; rgb[b+1] = shade; rgb[b+2] = shade;
            }
        }
        overlay_timestamp(&mut rgb, w, h);
        let jpeg = encode_jpeg(&rgb, w, h, 70);
        let arc = Arc::new(jpeg);
        state.camera.lock().latest_jpeg = Some(arc.clone());
        let _ = state.frame_tx.send(arc);
        std::thread::sleep(Duration::from_millis(40));
    }
}

fn yuyv_to_rgb(yuyv: &[u8], width: u32, height: u32) -> (u32, u32, Vec<u8>) {
    let clamp = |x: f32| x.max(0.0).min(255.0) as u8;
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    for (i, chunk) in yuyv.chunks_exact(4).enumerate() {
        let y0 = chunk[0] as f32;
        let u  = chunk[1] as f32 - 128.0;
        let y1 = chunk[2] as f32;
        let v  = chunk[3] as f32 - 128.0;
        let cvt = |y: f32| (clamp(y + 1.402*v), clamp(y - 0.344*u - 0.714*v), clamp(y + 1.772*u));
        let (r0,g0,b0) = cvt(y0);
        let (r1,g1,b1) = cvt(y1);
        let base = i * 6;
        if base + 5 < rgb.len() {
            rgb[base] = r0; rgb[base+1] = g0; rgb[base+2] = b0;
            rgb[base+3] = r1; rgb[base+4] = g1; rgb[base+5] = b1;
        }
    }
    (width, height, rgb)
}

// ============================================================
//  MJPEG AVI 录像写入
// ============================================================

pub fn save_mjpeg_avi(frames: &[Vec<u8>], fps: f64, path: &str) -> std::io::Result<()> {
    if frames.is_empty() { return Ok(()); }
    let frame_count  = frames.len() as u32;
    let fps_u        = fps as u32;
    let us_per_frame = (1_000_000.0 / fps) as u32;

    // movi 数据
    let mut movi: Vec<u8> = Vec::new();
    let mut offsets: Vec<(u32, u32)> = Vec::new();
    for f in frames {
        let off = mowi_len(&movi);
        mowi_tag(&mut movi, b"00dc");
        let sz = f.len() as u32;
        mowi_u32(&mut movi, sz);
        offsets.push((off + 8, sz));
        movi.extend_from_slice(f);
        if sz % 2 != 0 { movi.push(0); }
    }

    let mut buf: Vec<u8> = Vec::with_capacity(4096 + movi.len());

    mowi_tag(&mut buf, b"RIFF");
    let riff_pos = buf.len(); mowi_u32(&mut buf, 0);
    mowi_tag(&mut buf, b"AVI ");

    // hdrl
    mowi_tag(&mut buf, b"LIST");
    let hdrl_pos = buf.len(); mowi_u32(&mut buf, 0);
    mowi_tag(&mut buf, b"hdrl");

    // avih (56 bytes)
    mowi_tag(&mut buf, b"avih"); mowi_u32(&mut buf, 56);
    mowi_u32(&mut buf, us_per_frame);
    mowi_u32(&mut buf, 0); mowi_u32(&mut buf, 0);
    mowi_u32(&mut buf, 0x0010); // AVIF_HASINDEX
    mowi_u32(&mut buf, frame_count);
    for _ in 0..3 { mowi_u32(&mut buf, 0); } // InitialFrames, Streams=1 placeholder, SuggestedBufSize
    { let n = buf.len() - 4; buf[n..n+4].copy_from_slice(&1u32.to_le_bytes()); } // Streams = 1
    for _ in 0..6 { mowi_u32(&mut buf, 0); } // Width, Height, Reserved×4

    // strl
    mowi_tag(&mut buf, b"LIST");
    let strl_pos = buf.len(); mowi_u32(&mut buf, 0);
    mowi_tag(&mut buf, b"strl");

    // strh (56 bytes)
    mowi_tag(&mut buf, b"strh"); mowi_u32(&mut buf, 56);
    mowi_tag(&mut buf, b"vids"); mowi_tag(&mut buf, b"MJPG");
    mowi_u32(&mut buf, 0); mowi_u32(&mut buf, 0); // Flags, Priority+Language
    mowi_u32(&mut buf, 0); // InitialFrames
    mowi_u32(&mut buf, 1); mowi_u32(&mut buf, fps_u); // Scale, Rate
    mowi_u32(&mut buf, 0); mowi_u32(&mut buf, frame_count); // Start, Length
    mowi_u32(&mut buf, 0); mowi_u32(&mut buf, u32::MAX); // SugBufSize, Quality
    mowi_u32(&mut buf, 0); // SampleSize
    for _ in 0..4 { buf.extend_from_slice(&0u16.to_le_bytes()); } // rcFrame

    // strf / BITMAPINFOHEADER (40 bytes)
    mowi_tag(&mut buf, b"strf"); mowi_u32(&mut buf, 40);
    mowi_u32(&mut buf, 40); // biSize
    mowi_u32(&mut buf, 0); mowi_u32(&mut buf, 0); // Width, Height
    buf.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    buf.extend_from_slice(&24u16.to_le_bytes()); // biBitCount
    mowi_tag(&mut buf, b"MJPG"); // biCompression
    for _ in 0..5 { mowi_u32(&mut buf, 0); } // SizeImage, X/YPels, ClrUsed, ClrImportant

    // backfill strl
    let strl_sz = (buf.len() - strl_pos - 4) as u32;
    buf[strl_pos..strl_pos+4].copy_from_slice(&strl_sz.to_le_bytes());
    // backfill hdrl
    let hdrl_sz = (buf.len() - hdrl_pos - 4) as u32;
    buf[hdrl_pos..hdrl_pos+4].copy_from_slice(&hdrl_sz.to_le_bytes());

    // movi
    mowi_tag(&mut buf, b"LIST");
    mowi_u32(&mut buf, (4 + movi.len()) as u32);
    mowi_tag(&mut buf, b"movi");
    let movi_start = buf.len() as u32;
    buf.extend_from_slice(&movi);

    // idx1
    mowi_tag(&mut buf, b"idx1");
    mowi_u32(&mut buf, frame_count * 16);
    for (off, sz) in &offsets {
        mowi_tag(&mut buf, b"00dc");
        mowi_u32(&mut buf, 0x10); // AVIIF_KEYFRAME
        mowi_u32(&mut buf, movi_start + off - 8);
        mowi_u32(&mut buf, *sz);
    }

    // backfill RIFF
    let riff_sz = (buf.len() - riff_pos - 4) as u32;
    buf[riff_pos..riff_pos+4].copy_from_slice(&riff_sz.to_le_bytes());

    std::fs::write(path, &buf)
}

#[inline] fn mowi_len(v: &[u8]) -> u32 { v.len() as u32 }
#[inline] fn mowi_tag(v: &mut Vec<u8>, t: &[u8; 4]) { v.extend_from_slice(t); }
#[inline] fn mowi_u32(v: &mut Vec<u8>, n: u32) { v.extend_from_slice(&n.to_le_bytes()); }
