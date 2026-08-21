/*!
 * 纯 Rust 帧差运动侦测（优化版）
 *
 * 算法：
 *  1. 将 RGB 帧转换为灰度（rayon 行并行）
 *  2. 对灰度图做 3x3 分离式均值模糊（horizontal pass → vertical pass）
 *     复杂度 O(6n) 而非原版 O(9n)，且横向 pass 用 rayon 并行
 *  3. 计算与上一帧的绝对差值（rayon 并行）
 *  4. 二值化（阈值 = sensitivity）
 *  5. 膨胀（5x5 核，1 次迭代）
 *  6. 扫描连通像素块，聚合为边界矩形
 *  7. 过滤面积 < min_area 的矩形
 *  8. 返回检测矩形列表和是否有运动
 */

use rayon::prelude::*;

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// RGB 像素转灰度（BT.601 系数）
#[inline]
fn rgb_to_gray(r: u8, g: u8, b: u8) -> u8 {
    ((r as u32 * 77 + g as u32 * 150 + b as u32 * 29) >> 8) as u8
}

/// 将 RGB 字节数组（宽*高*3）转为灰度数组（rayon 行并行）
pub fn to_grayscale(rgb: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let mut gray = vec![0u8; w * h];
    // 按行并行处理
    gray.par_chunks_mut(w)
        .enumerate()
        .for_each(|(row, out_row)| {
            let base = row * w * 3;
            for x in 0..w {
                let b = base + x * 3;
                out_row[x] = rgb_to_gray(rgb[b], rgb[b + 1], rgb[b + 2]);
            }
        });
    gray
}

/// 3x3 分离式均值模糊（separable box filter）
///
/// 分两遍：横向 pass → 纵向 pass，总复杂度 O(6n) 而非 O(9n)。
/// 横向 pass 使用 rayon 行并行，适合多核场景。
pub fn box_blur(src: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;

    // --- Pass 1: 横向 1x3 均值（rayon 行并行）---
    let mut tmp = vec![0u8; w * h];
    tmp.par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, row_out)| {
            let row_in = &src[y * w..(y + 1) * w];
            // 边界像素直接复制
            row_out[0]     = row_in[0];
            row_out[w - 1] = row_in[w - 1];
            for x in 1..(w - 1) {
                row_out[x] = ((row_in[x - 1] as u32 + row_in[x] as u32 + row_in[x + 1] as u32) / 3) as u8;
            }
        });

    // --- Pass 2: 纵向 3x1 均值（顺序，无法简单并行因列跨行） ---
    let mut dst = vec![0u8; w * h];
    // 顶行和底行直接复制
    dst[..w].copy_from_slice(&tmp[..w]);
    dst[(h - 1) * w..].copy_from_slice(&tmp[(h - 1) * w..]);
    for y in 1..(h - 1) {
        for x in 0..w {
            dst[y * w + x] = ((tmp[(y - 1) * w + x] as u32
                + tmp[y * w + x] as u32
                + tmp[(y + 1) * w + x] as u32)
                / 3) as u8;
        }
    }
    dst
}

/// 绝对差值后二值化（rayon 并行）
fn abs_diff_thresh(a: &[u8], b: &[u8], thresh: u8) -> Vec<u8> {
    let mut out = vec![0u8; a.len()];
    out.par_iter_mut()
        .zip(a.par_iter().zip(b.par_iter()))
        .for_each(|(o, (&x, &y))| {
            *o = if x.abs_diff(y) > thresh { 255 } else { 0 };
        });
    out
}

/// 简单膨胀（5x5 核）
fn dilate(src: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let mut dst = vec![0u8; w * h];
    let r: isize = 2; // 半径
    for y in 0..h {
        for x in 0..w {
            'outer: for dy in -r..=r {
                for dx in -r..=r {
                    let ny = y as isize + dy;
                    let nx = x as isize + dx;
                    if ny >= 0 && ny < h as isize && nx >= 0 && nx < w as isize {
                        if src[ny as usize * w + nx as usize] > 0 {
                            dst[y * w + x] = 255;
                            break 'outer;
                        }
                    }
                }
            }
        }
    }
    dst
}

/// Union-Find（并查集）用于连通域分析
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self { Self { parent: (0..n).collect() } }
    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }
    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb { self.parent[ra] = rb; }
    }
}

/// 连通域分析，返回每个区域的边界矩形
pub fn find_contour_rects(mask: &[u8], width: u32, height: u32) -> Vec<Rect> {
    let w = width as usize;
    let h = height as usize;
    let mut labels = vec![0usize; w * h];
    let mut uf = UnionFind::new(w * h + 1);
    let mut next_label = 1usize;

    // 第一遍扫描：打标签
    for y in 0..h {
        for x in 0..w {
            if mask[y * w + x] == 0 { continue; }
            let idx = y * w + x;

            let above = if y > 0 && mask[(y-1)*w+x] > 0 { labels[(y-1)*w+x] } else { 0 };
            let left  = if x > 0 && mask[y*w+x-1]  > 0 { labels[y*w+x-1]  } else { 0 };

            labels[idx] = match (above, left) {
                (0, 0) => { let l = next_label; next_label += 1; l }
                (a, 0) => a,
                (0, l) => l,
                (a, l) => { uf.union(a, l); a }
            };
        }
    }

    // 第二遍：合并标签，统计边界框
    use std::collections::HashMap;
    let mut boxes: HashMap<usize, (usize, usize, usize, usize)> = HashMap::new(); // label -> (xmin,ymin,xmax,ymax)

    for y in 0..h {
        for x in 0..w {
            let raw = labels[y * w + x];
            if raw == 0 { continue; }
            let root = uf.find(raw);
            let e = boxes.entry(root).or_insert((x, y, x, y));
            e.0 = e.0.min(x);
            e.1 = e.1.min(y);
            e.2 = e.2.max(x);
            e.3 = e.3.max(y);
        }
    }

    boxes.values().map(|&(x0, y0, x1, y1)| Rect {
        x: x0 as u32,
        y: y0 as u32,
        w: (x1 - x0 + 1) as u32,
        h: (y1 - y0 + 1) as u32,
    }).collect()
}

/// 在 RGB 图像上画矩形（红色，2px 边框）
pub fn draw_rect(rgb: &mut [u8], width: u32, height: u32, r: &Rect) {
    let (w, h) = (width as usize, height as usize);
    let (rx, ry, rw, rh) = (r.x as usize, r.y as usize, r.w as usize, r.h as usize);
    let x1 = (rx + rw).min(w - 1);
    let y1 = (ry + rh).min(h - 1);

    let set_pixel = |rgb: &mut [u8], x: usize, y: usize| {
        let b = (y * w + x) * 3;
        rgb[b] = 255; rgb[b+1] = 0; rgb[b+2] = 0;
    };

    for x in rx..=x1 {
        if ry < h { set_pixel(rgb, x, ry); }
        if y1 < h { set_pixel(rgb, x, y1); }
    }
    for y in ry..=y1 {
        if rx < w { set_pixel(rgb, rx, y); }
        if x1 < w { set_pixel(rgb, x1, y); }
    }
    // 2px 宽
    for x in rx..=x1 {
        if ry+1 < h { set_pixel(rgb, x, ry+1); }
        if y1 > 0 && y1-1 < h { set_pixel(rgb, x, y1-1); }
    }
}

/// 对 RGB 帧执行运动侦测
/// 返回：(检测到的矩形列表, 是否有运动, 新的灰度帧用于下次比较)
pub fn detect_motion(
    rgb: &mut Vec<u8>,
    width: u32,
    height: u32,
    prev_gray: &Option<Vec<u8>>,
    sensitivity: u8,
    min_area: u32,
) -> (Vec<Rect>, bool, Vec<u8>) {
    let gray = to_grayscale(rgb, width, height);
    let blurred = box_blur(&gray, width, height);

    let Some(prev) = prev_gray else {
        return (vec![], false, blurred);
    };

    let diff = abs_diff_thresh(prev, &blurred, sensitivity);
    let dilated = dilate(&diff, width, height);
    let rects = find_contour_rects(&dilated, width, height);

    let filtered: Vec<Rect> = rects.into_iter()
        .filter(|r| r.w * r.h >= min_area)
        .collect();

    let detected = !filtered.is_empty();
    for r in &filtered {
        draw_rect(rgb, width, height, r);
    }

    (filtered, detected, blurred)
}

// ============================================================
//  单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_rgb(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
        vec![[r, g, b]; (w * h) as usize].into_flattened()
    }

    // --- to_grayscale ---

    #[test]
    fn grayscale_black_is_zero() {
        let rgb = solid_rgb(4, 4, 0, 0, 0);
        let gray = to_grayscale(&rgb, 4, 4);
        assert!(gray.iter().all(|&v| v == 0));
    }

    #[test]
    fn grayscale_white_is_255() {
        let rgb = solid_rgb(4, 4, 255, 255, 255);
        let gray = to_grayscale(&rgb, 4, 4);
        // BT.601: (255*77 + 255*150 + 255*29) >> 8 = (255*256) >> 8 = 255
        assert!(gray.iter().all(|&v| v == 255));
    }

    #[test]
    fn grayscale_pure_red() {
        let rgb = solid_rgb(2, 2, 255, 0, 0);
        let gray = to_grayscale(&rgb, 2, 2);
        // (255*77) >> 8 = 76
        assert!(gray.iter().all(|&v| v == 76), "expected 76, got {}", gray[0]);
    }

    #[test]
    fn grayscale_output_size() {
        let rgb = solid_rgb(10, 8, 128, 64, 32);
        let gray = to_grayscale(&rgb, 10, 8);
        assert_eq!(gray.len(), 10 * 8);
    }

    // --- box_blur ---

    #[test]
    fn blur_uniform_image_unchanged() {
        // 均匀图像经过均值模糊后仍是均匀的
        let src = vec![100u8; 8 * 8];
        let blurred = box_blur(&src, 8, 8);
        // 内部像素应等于 100，边界复制
        for y in 1..7 {
            for x in 1..7 {
                assert_eq!(blurred[y * 8 + x], 100, "at ({x},{y})");
            }
        }
    }

    #[test]
    fn blur_output_size_matches_input() {
        let src = vec![50u8; 16 * 12];
        let blurred = box_blur(&src, 16, 12);
        assert_eq!(blurred.len(), src.len());
    }

    #[test]
    fn blur_center_spike_smoothed() {
        let w = 7usize;
        let h = 7usize;
        let mut src = vec![0u8; w * h];
        src[3 * w + 3] = 255; // 中心单个亮点
        let blurred = box_blur(&src, w as u32, h as u32);
        // 中心点应被平滑到 255/9 ≈ 28
        assert!(blurred[3 * w + 3] < 255, "center should be smoothed");
        assert!(blurred[3 * w + 3] > 0,   "center should retain some signal");
    }

    // --- find_contour_rects ---

    #[test]
    fn no_rects_on_blank_mask() {
        let mask = vec![0u8; 10 * 10];
        let rects = find_contour_rects(&mask, 10, 10);
        assert!(rects.is_empty());
    }

    #[test]
    fn single_blob_produces_one_rect() {
        let w = 10u32;
        let h = 10u32;
        let mut mask = vec![0u8; (w * h) as usize];
        // 3x3 亮块，左上角 (2,2)
        for dy in 0..3u32 {
            for dx in 0..3u32 {
                mask[((2 + dy) * w + (2 + dx)) as usize] = 255;
            }
        }
        let rects = find_contour_rects(&mask, w, h);
        assert_eq!(rects.len(), 1);
        let r = &rects[0];
        assert_eq!(r.x, 2);
        assert_eq!(r.y, 2);
        assert_eq!(r.w, 3);
        assert_eq!(r.h, 3);
    }

    #[test]
    fn two_separate_blobs_produce_two_rects() {
        let w = 20u32;
        let h = 10u32;
        let mut mask = vec![0u8; (w * h) as usize];
        // Blob A at (1,1) size 2x2
        for dy in 0..2u32 { for dx in 0..2u32 { mask[((1+dy)*w+(1+dx)) as usize] = 255; } }
        // Blob B at (15,1) size 2x2
        for dy in 0..2u32 { for dx in 0..2u32 { mask[((1+dy)*w+(15+dx)) as usize] = 255; } }
        let rects = find_contour_rects(&mask, w, h);
        assert_eq!(rects.len(), 2);
    }

    // --- detect_motion ---

    #[test]
    fn no_motion_on_first_frame() {
        let mut rgb = solid_rgb(8, 8, 100, 100, 100);
        let (rects, detected, _) = detect_motion(&mut rgb, 8, 8, &None, 30, 10);
        assert!(!detected);
        assert!(rects.is_empty());
    }

    #[test]
    fn no_motion_on_identical_frames() {
        let mut rgb = solid_rgb(16, 16, 128, 64, 32);
        let (_, _, prev_gray) = detect_motion(&mut rgb, 16, 16, &None, 30, 10);

        let mut rgb2 = solid_rgb(16, 16, 128, 64, 32);
        let (rects, detected, _) = detect_motion(&mut rgb2, 16, 16, &Some(prev_gray), 30, 10);
        assert!(!detected, "identical frames should not trigger motion");
        assert!(rects.is_empty());
    }

    #[test]
    fn motion_detected_on_changed_frame() {
        // 帧 1：全黑
        let mut rgb1 = solid_rgb(32, 32, 0, 0, 0);
        let (_, _, prev_gray) = detect_motion(&mut rgb1, 32, 32, &None, 10, 1);

        // 帧 2：全白（极大变化）
        let mut rgb2 = solid_rgb(32, 32, 255, 255, 255);
        let (rects, detected, _) = detect_motion(&mut rgb2, 32, 32, &Some(prev_gray), 10, 1);
        assert!(detected, "large frame change should trigger motion");
        assert!(!rects.is_empty());
    }

    #[test]
    fn min_area_filter_suppresses_small_blobs() {
        let mut rgb1 = solid_rgb(32, 32, 0, 0, 0);
        let (_, _, prev_gray) = detect_motion(&mut rgb1, 32, 32, &None, 10, 1);

        let mut rgb2 = solid_rgb(32, 32, 255, 255, 255);
        // min_area 极大，应过滤掉所有检测结果
        let (rects, detected, _) = detect_motion(&mut rgb2, 32, 32, &Some(prev_gray), 10, 999_999);
        assert!(!detected);
        assert!(rects.is_empty());
    }
}
