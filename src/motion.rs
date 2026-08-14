/*!
 * 纯 Rust 帧差运动侦测
 *
 * 算法：
 *  1. 将 RGB 帧转换为灰度
 *  2. 对灰度图做 3x3 均值模糊（近似高斯）
 *  3. 计算与上一帧的绝对差值
 *  4. 二值化（阈值 = sensitivity）
 *  5. 膨胀（5x5 核，1 次迭代）
 *  6. 扫描连通像素块，聚合为边界矩形
 *  7. 过滤面积 < min_area 的矩形
 *  8. 返回检测矩形列表和是否有运动
 */

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

/// 将 RGB 字节数组（宽*高*3）转为灰度数组
pub fn to_grayscale(rgb: &[u8], width: u32, height: u32) -> Vec<u8> {
    let len = (width * height) as usize;
    let mut gray = vec![0u8; len];
    for i in 0..len {
        let base = i * 3;
        gray[i] = rgb_to_gray(rgb[base], rgb[base + 1], rgb[base + 2]);
    }
    gray
}

/// 3x3 均值模糊（box filter）
pub fn box_blur(src: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let mut dst = vec![0u8; w * h];
    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let sum: u32 = (0..3usize).flat_map(|dy| {
                (0..3usize).map(move |dx| src[(y + dy - 1) * w + (x + dx - 1)] as u32)
            }).sum();
            dst[y * w + x] = (sum / 9) as u8;
        }
    }
    // 边界直接复制
    for x in 0..w { dst[x] = src[x]; dst[(h-1)*w+x] = src[(h-1)*w+x]; }
    for y in 0..h { dst[y*w] = src[y*w]; dst[y*w+w-1] = src[y*w+w-1]; }
    dst
}

/// 绝对差值后二值化
fn abs_diff_thresh(a: &[u8], b: &[u8], thresh: u8) -> Vec<u8> {
    a.iter().zip(b.iter()).map(|(&x, &y)| {
        if x.abs_diff(y) > thresh { 255 } else { 0 }
    }).collect()
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
