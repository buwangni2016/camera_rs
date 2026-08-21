/*!
 * 运动热力图模块
 * 将运动检测的触发区域累加成热力图，并导出为 PNG 图片
 */

use crate::state::AppState;

/// 当检测到运动轮廓时，将运动区域（像素坐标）叠加到热力图格子
pub fn record_motion(
    state: &AppState,
    contours: &[(u32, u32, u32, u32)],
    frame_w: u32,
    frame_h: u32,
) {
    if frame_w == 0 || frame_h == 0 {
        return;
    }
    let (cols, rows) = state.heatmap_size;
    let mut hm = state.heatmap.lock();
    for &(x, y, w, h) in contours {
        // 将轮廓包围盒映射到热力图格子
        let gx1 = (x * cols / frame_w).min(cols - 1);
        let gy1 = (y * rows / frame_h).min(rows - 1);
        let gx2 = ((x + w) * cols / frame_w).min(cols - 1);
        let gy2 = ((y + h) * rows / frame_h).min(rows - 1);
        for gy in gy1..=gy2 {
            for gx in gx1..=gx2 {
                let idx = (gy * cols + gx) as usize;
                if idx < hm.len() {
                    hm[idx] = hm[idx].saturating_add(1);
                }
            }
        }
    }
}

/// 将热力图数据渲染为 PNG 字节（彩色伪色图）
pub fn render_heatmap_png(state: &AppState) -> Vec<u8> {
    let (cols, rows) = state.heatmap_size;
    let hm = state.heatmap.lock().clone();
    let max_val = hm.iter().copied().max().unwrap_or(1).max(1);

    // 每格放大为 10x10 像素
    let cell = 10u32;
    let img_w = cols * cell;
    let img_h = rows * cell;
    let mut rgb = vec![0u8; (img_w * img_h * 3) as usize];

    for gy in 0..rows {
        for gx in 0..cols {
            let idx = (gy * cols + gx) as usize;
            let v = hm[idx] as f32 / max_val as f32; // 0.0-1.0
            let (r, g, b) = heat_color(v);
            // 写入 cell x cell 块
            for dy in 0..cell {
                for dx in 0..cell {
                    let px = gx * cell + dx;
                    let py = gy * cell + dy;
                    let pi = ((py * img_w + px) * 3) as usize;
                    rgb[pi] = r;
                    rgb[pi + 1] = g;
                    rgb[pi + 2] = b;
                }
            }
        }
    }

    let img = image::ImageBuffer::<image::Rgb<u8>, Vec<u8>>::from_raw(img_w, img_h, rgb)
        .expect("热力图缓冲区无效");
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_with_encoder(image::codecs::png::PngEncoder::new(&mut buf))
        .ok();
    buf
}

/// 将热力图数据序列化为 JSON（供前端 Canvas 绘制）
pub fn heatmap_json(state: &AppState) -> serde_json::Value {
    let (cols, rows) = state.heatmap_size;
    let hm = state.heatmap.lock().clone();
    let max_val = hm.iter().copied().max().unwrap_or(1).max(1);
    let normalized: Vec<f32> = hm.iter().map(|&v| v as f32 / max_val as f32).collect();
    serde_json::json!({
        "cols":  cols,
        "rows":  rows,
        "max":   max_val,
        "data":  normalized,
    })
}

/// 清空热力图
pub fn clear_heatmap(state: &AppState) {
    let mut hm = state.heatmap.lock();
    for v in hm.iter_mut() {
        *v = 0;
    }
}

// ============================================================
//  伪色映射（蓝 -> 青 -> 绿 -> 黄 -> 红）
// ============================================================

fn heat_color(t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let (r, g, b) = if t < 0.25 {
        let s = t / 0.25;
        (0.0, s, 1.0)
    } else if t < 0.5 {
        let s = (t - 0.25) / 0.25;
        (0.0, 1.0, 1.0 - s)
    } else if t < 0.75 {
        let s = (t - 0.5) / 0.25;
        (s, 1.0, 0.0)
    } else {
        let s = (t - 0.75) / 0.25;
        (1.0, 1.0 - s, 0.0)
    };
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}
