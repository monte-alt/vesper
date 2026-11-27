//! Anti-aliased 2D software canvas and swap-chain pixel packer.

#![allow(clippy::too_many_arguments)]

use windows::Win32::Graphics::Dxgi::Common::*;

use super::font::{GLYPH, glyph};

/// Integer scale for the swap-chain height: 1x up to 1600p, 2x at 4K.
pub(super) fn auto_scale(height: u32) -> u32 {
    if height >= 1800 { 2 } else { 1 }
}

/// Bytes per pixel for a supported swap-chain format.
pub(super) fn bpp(format: DXGI_FORMAT) -> Option<usize> {
    match format {
        DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_B8G8R8A8_UNORM | DXGI_FORMAT_R10G10B10A2_UNORM => Some(4),
        DXGI_FORMAT_R16G16B16A16_FLOAT => Some(8),
        _ => None,
    }
}

fn f32_to_f16(v: f32) -> u16 {
    let bits = v.to_bits();
    let exp = ((bits >> 23) & 0xff) as i32 - 127;
    let mant = bits & 0x7f_ffff;
    if exp < -14 {
        return 0;
    }
    let h_exp = ((exp + 15).clamp(0, 31) as u16) << 10;
    let h_mant = (mant >> 13) as u16;
    h_exp | h_mant
}

/// Encodes an RGB colour into `dst` (length == `bpp(format)`).
pub(super) fn encode(format: DXGI_FORMAT, [r, g, b]: [u8; 3], dst: &mut [u8]) {
    match format {
        DXGI_FORMAT_R8G8B8A8_UNORM => dst[..4].copy_from_slice(&[r, g, b, 0xFF]),
        DXGI_FORMAT_B8G8R8A8_UNORM => dst[..4].copy_from_slice(&[b, g, r, 0xFF]),
        DXGI_FORMAT_R10G10B10A2_UNORM => {
            let r10 = (u32::from(r) * 1023 / 255) & 0x3FF;
            let g10 = (u32::from(g) * 1023 / 255) & 0x3FF;
            let b10 = (u32::from(b) * 1023 / 255) & 0x3FF;
            let word = r10 | (g10 << 10) | (b10 << 20) | (0x3 << 30);
            dst[..4].copy_from_slice(&word.to_le_bytes());
        }
        DXGI_FORMAT_R16G16B16A16_FLOAT => {
            let to_h = |c: u8| {
                let s = f32::from(c) / 255.0;
                f32_to_f16(s * s)
            };
            dst[0..2].copy_from_slice(&to_h(r).to_le_bytes());
            dst[2..4].copy_from_slice(&to_h(g).to_le_bytes());
            dst[4..6].copy_from_slice(&to_h(b).to_le_bytes());
            dst[6..8].copy_from_slice(&0x3C00u16.to_le_bytes());
        }
        _ => {}
    }
}

pub(super) fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        (f32::from(a[0]) + (f32::from(b[0]) - f32::from(a[0])) * t).round() as u8,
        (f32::from(a[1]) + (f32::from(b[1]) - f32::from(a[1])) * t).round() as u8,
        (f32::from(a[2]) + (f32::from(b[2]) - f32::from(a[2])) * t).round() as u8,
    ]
}

fn sdf_rounded_box(px: f32, py: f32, bx: f32, by: f32, bw: f32, bh: f32, r: f32) -> f32 {
    let cx = bx + bw * 0.5;
    let cy = by + bh * 0.5;
    let hx = (bw * 0.5 - r).max(0.0);
    let hy = (bh * 0.5 - r).max(0.0);
    let dx = (px - cx).abs() - hx;
    let dy = (py - cy).abs() - hy;
    let out = dx.max(0.0).hypot(dy.max(0.0));
    let inside = dx.max(dy).min(0.0);
    out + inside - r
}

pub(super) struct Canvas {
    pub(super) w: usize,
    pub(super) h: usize,
    pub(super) rgb: Vec<[u8; 3]>,
}

impl Canvas {
    pub(super) fn new(w: u32, h: u32, fill: [u8; 3]) -> Canvas {
        Canvas {
            w: w as usize,
            h: h as usize,
            rgb: vec![fill; (w * h) as usize],
        }
    }

    #[inline]
    pub(super) fn blend_px(&mut self, x: i32, y: i32, color: [u8; 3], alpha: f32) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 || alpha <= 0.002 {
            return;
        }
        let a = alpha.clamp(0.0, 1.0);
        let idx = y as usize * self.w + x as usize;
        if a >= 0.998 {
            self.rgb[idx] = color;
            return;
        }
        let bg = self.rgb[idx];
        self.rgb[idx] = [
            (f32::from(bg[0]) + (f32::from(color[0]) - f32::from(bg[0])) * a).round() as u8,
            (f32::from(bg[1]) + (f32::from(color[1]) - f32::from(bg[1])) * a).round() as u8,
            (f32::from(bg[2]) + (f32::from(color[2]) - f32::from(bg[2])) * a).round() as u8,
        ];
    }

    pub(super) fn rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: [u8; 3]) {
        let x0 = (x as usize).min(self.w);
        let y0 = (y as usize).min(self.h);
        let x1 = (x.saturating_add(w) as usize).min(self.w);
        let y1 = (y.saturating_add(h) as usize).min(self.h);
        for yy in y0..y1 {
            self.rgb[yy * self.w + x0..yy * self.w + x1].fill(color);
        }
    }

    pub(super) fn fill_grad_h(&mut self, x: u32, y: u32, w: u32, h: u32, left: [u8; 3], right: [u8; 3]) {
        if w == 0 || h == 0 {
            return;
        }
        let denom = (w.saturating_sub(1)).max(1) as f32;
        for dx in 0..w {
            let col = lerp_rgb(left, right, dx as f32 / denom);
            self.rect(x + dx, y, 1, h, col);
        }
    }

    pub(super) fn rounded_rect_grad_v(
        &mut self,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        r: f32,
        top: [u8; 3],
        bot: [u8; 3],
    ) {
        if w == 0 || h == 0 {
            return;
        }
        let x0 = x as i32;
        let y0 = y as i32;
        let x1 = (x + w).min(self.w as u32) as i32;
        let y1 = (y + h).min(self.h as u32) as i32;
        let denom = (h.saturating_sub(1)).max(1) as f32;
        for yy in y0..y1 {
            let col = lerp_rgb(top, bot, (yy - y0) as f32 / denom);
            for xx in x0..x1 {
                let d = sdf_rounded_box(xx as f32 + 0.5, yy as f32 + 0.5, x as f32, y as f32, w as f32, h as f32, r);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                self.blend_px(xx, yy, col, cov);
            }
        }
    }

    pub(super) fn rounded_rect_fill(
        &mut self,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        r: f32,
        color: [u8; 3],
        alpha: f32,
    ) {
        if w == 0 || h == 0 || alpha <= 0.002 {
            return;
        }
        let x0 = x as i32;
        let y0 = y as i32;
        let x1 = (x + w).min(self.w as u32) as i32;
        let y1 = (y + h).min(self.h as u32) as i32;
        for yy in y0..y1 {
            for xx in x0..x1 {
                let d = sdf_rounded_box(xx as f32 + 0.5, yy as f32 + 0.5, x as f32, y as f32, w as f32, h as f32, r);
                let cov = (0.5 - d).clamp(0.0, 1.0) * alpha;
                self.blend_px(xx, yy, color, cov);
            }
        }
    }

    pub(super) fn rounded_rect_stroke(
        &mut self,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        r: f32,
        color: [u8; 3],
        alpha: f32,
    ) {
        if w == 0 || h == 0 || alpha <= 0.002 {
            return;
        }
        let x0 = x as i32;
        let y0 = y as i32;
        let x1 = (x + w).min(self.w as u32) as i32;
        let y1 = (y + h).min(self.h as u32) as i32;
        for yy in y0..y1 {
            for xx in x0..x1 {
                let d = sdf_rounded_box(xx as f32 + 0.5, yy as f32 + 0.5, x as f32, y as f32, w as f32, h as f32, r);
                // 1 px stroke band centered near the outer perimeter.
                let ring = (1.0 - (d + 0.5).abs()).clamp(0.0, 1.0) * alpha;
                self.blend_px(xx, yy, color, ring);
            }
        }
    }

    pub(super) fn circle_aa(&mut self, cx: f32, cy: f32, radius: f32, color: [u8; 3], alpha: f32) {
        let x0 = (cx - radius - 1.5).floor().max(0.0) as i32;
        let y0 = (cy - radius - 1.5).floor().max(0.0) as i32;
        let x1 = (cx + radius + 1.5).ceil().min(self.w as f32) as i32;
        let y1 = (cy + radius + 1.5).ceil().min(self.h as f32) as i32;
        for yy in y0..y1 {
            for xx in x0..x1 {
                let dist = (xx as f32 + 0.5 - cx).hypot(yy as f32 + 0.5 - cy) - radius;
                let cov = (0.5 - dist).clamp(0.0, 1.0) * alpha;
                self.blend_px(xx, yy, color, cov);
            }
        }
    }

    pub(super) fn line_aa(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, color: [u8; 3], alpha: f32) {
        let half = (width * 0.5).max(0.45);
        let pad = half + 1.5;
        let min_x = x0.min(x1).sub(pad).floor().max(0.0) as i32;
        let min_y = y0.min(y1).sub(pad).floor().max(0.0) as i32;
        let max_x = x0.max(x1).add(pad).ceil().min(self.w as f32) as i32;
        let max_y = y0.max(y1).add(pad).ceil().min(self.h as f32) as i32;
        let vx = x1 - x0;
        let vy = y1 - y0;
        let len2 = vx * vx + vy * vy;
        for yy in min_y..max_y {
            let py = yy as f32 + 0.5;
            for xx in min_x..max_x {
                let px = xx as f32 + 0.5;
                let t = if len2 > 1e-6 {
                    (((px - x0) * vx + (py - y0) * vy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let cx = x0 + vx * t;
                let cy = y0 + vy * t;
                let dist = (px - cx).hypot(py - cy) - half;
                let cov = (0.5 - dist).clamp(0.0, 1.0) * alpha;
                self.blend_px(xx, yy, color, cov);
            }
        }
    }

    pub(super) fn dashed_h_line(&mut self, x: u32, y: u32, w: u32, color: [u8; 3], alpha: f32) {
        let mut cx = x;
        let end = x + w;
        while cx < end {
            let seg = 4u32.min(end - cx);
            for dx in 0..seg {
                self.blend_px((cx + dx) as i32, y as i32, color, alpha);
            }
            cx += 8;
        }
    }

    /// Minimalist Vesper emblem: dusk pill with vector `V` and twilight star.
    pub(super) fn draw_vesper_emblem(&mut self, x: u32, y: u32, size: u32) {
        let s = size as f32;
        self.rounded_rect_grad_v(x, y, size, size, s * 0.24, [24, 30, 56], [38, 28, 52]);
        self.rounded_rect_stroke(x, y, size, size, s * 0.24, [110, 122, 185], 0.85);
        let ox = x as f32;
        let oy = y as f32;
        // Faceted V strokes (indigo -> violet -> warm amber)
        self.line_aa(ox + s * 0.26, oy + s * 0.28, ox + s * 0.50, oy + s * 0.78, (s * 0.13).max(1.8), [129, 140, 248], 1.0);
        self.line_aa(ox + s * 0.74, oy + s * 0.28, ox + s * 0.50, oy + s * 0.78, (s * 0.13).max(1.8), [251, 146, 60], 1.0);
        // Dusk star glint at top-center
        self.circle_aa(ox + s * 0.50, oy + s * 0.22, (s * 0.08).max(1.1), [251, 191, 36], 1.0);
    }

    /// Draws `text` cleanly with a subtle dark shadow for high contrast over gradients.
    pub(super) fn text(&mut self, mut x: u32, y: u32, px: u32, color: [u8; 3], text: &str) -> u32 {
        let step = (GLYPH.0 + 1) * px;
        let shadow = [6, 8, 15];
        for ch in text.bytes() {
            let bits = glyph(ch);
            for (row, &mask) in bits.iter().enumerate() {
                if mask == 0 {
                    continue;
                }
                for col in 0..GLYPH.0 {
                    if (mask >> (GLYPH.0 - 1 - col)) & 1 != 0 {
                        let gx = x + col * px;
                        let gy = y + row as u32 * px;
                        self.rect(gx + 1, gy + 1, px, px, shadow);
                        self.rect(gx, gy, px, px, color);
                    }
                }
            }
            x += step;
        }
        x
    }

    pub(super) fn text_sep(&mut self, mut x: u32, y: u32, px: u32, color: [u8; 3], sep: [u8; 3], text: &str) -> u32 {
        for (i, part) in text.split('|').enumerate() {
            if i > 0 {
                x = self.text(x, y, px, sep, "|");
            }
            x = self.text(x, y, px, color, part);
        }
        x
    }

    /// Smooth anti-aliased frame-time area graph with optional target line & secondary series.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_sparkline(
        &mut self,
        gx: u32,
        gy: u32,
        gw: u32,
        gh: u32,
        primary_ms: &[f32],
        secondary_ms: Option<&[f32]>,
        max_scale: f32,
        target_fps: u32,
        color_fn: fn(f32) -> [u8; 3],
    ) {
        self.rounded_rect_fill(gx, gy, gw, gh, 6.0, [8, 11, 20], 1.0);
        self.rounded_rect_stroke(gx, gy, gw, gh, 6.0, [29, 37, 61], 1.0);

        let pad_x = 4u32;
        let pad_y = 4u32;
        let iw = gw.saturating_sub(pad_x * 2);
        let ih = gh.saturating_sub(pad_y * 2);
        if iw < 8 || ih < 8 {
            return;
        }
        let ix = gx + pad_x;
        let iy = gy + pad_y;
        let bot_y = iy + ih - 1;

        // Subtle mid-grid line.
        let mid_y = iy + ih / 2;
        self.dashed_h_line(ix, mid_y, iw, [36, 44, 68], 0.65);

        // Target reference line (30 FPS = 33.3 ms in muted amber, 60 FPS = 16.7 ms in muted emerald).
        let (target_ms, target_col, target_lbl) = if target_fps >= 60 {
            (16.67f32, [40, 140, 108], "16.7ms")
        } else {
            (33.33f32, [160, 108, 22], "33.3ms")
        };
        if target_ms < max_scale {
            let ty = (bot_y as f32 - (target_ms / max_scale) * (ih - 2) as f32).round() as u32;
            self.dashed_h_line(ix, ty, iw, target_col, 0.55);
            self.text(ix + 2, ty.saturating_sub(9), 1, target_col, target_lbl);
        }

        if primary_ms.is_empty() {
            return;
        }

        let points = primary_ms.len().min(iw as usize / 2).max(1);
        let slice = &primary_ms[primary_ms.len() - points..];
        let step_x = if points > 1 {
            (iw - 1) as f32 / (points - 1) as f32
        } else {
            0.0
        };
        let x_off = if points == 1 { ix as f32 + iw as f32 - 2.0 } else { ix as f32 };
        let map_y = |ms: f32| -> f32 {
            let frac = (ms / max_scale).clamp(0.02, 1.0);
            bot_y as f32 - frac * (ih - 3) as f32
        };

        // Translucent vertical area fill under primary curve.
        for x_rel in 0..iw {
            let fx = if points > 1 {
                (x_rel as f32 / (iw - 1) as f32) * (points - 1) as f32
            } else {
                0.0
            };
            let idx = (fx.floor() as usize).min(points - 1);
            let next = (idx + 1).min(points - 1);
            let frac = fx - idx as f32;
            let ms = slice[idx] + (slice[next] - slice[idx]) * frac;
            let top_y = map_y(ms).ceil() as u32;
            let base_col = color_fn(ms);
            for yy in top_y..=bot_y {
                let h_frac = 1.0 - ((yy - top_y) as f32 / (bot_y.saturating_sub(top_y).max(1)) as f32);
                let fill_alpha = 0.05 + 0.28 * h_frac;
                self.blend_px((ix + x_rel) as i32, yy as i32, base_col, fill_alpha);
            }
        }

        // Secondary curve (e.g., CPU/submit ms in sky cyan) underneath primary curve.
        if let Some(sec) = secondary_ms {
            let sp = sec.len().min(points);
            if sp >= 2 {
                let s_slice = &sec[sec.len() - sp..];
                let sx0 = ix as f32 + (points - sp) as f32 * step_x;
                for i in 0..sp - 1 {
                    self.line_aa(
                        sx0 + i as f32 * step_x,
                        map_y(s_slice[i]),
                        sx0 + (i + 1) as f32 * step_x,
                        map_y(s_slice[i + 1]),
                        1.3,
                        [56, 189, 248],
                        0.78,
                    );
                }
            }
        }

        // Smooth AA primary stroke.
        if points == 1 {
            self.circle_aa(x_off, map_y(slice[0]), 2.0, color_fn(slice[0]), 1.0);
        } else {
            for i in 0..points - 1 {
                let col = color_fn((slice[i] + slice[i + 1]) * 0.5);
                self.line_aa(
                    x_off + i as f32 * step_x,
                    map_y(slice[i]),
                    x_off + (i + 1) as f32 * step_x,
                    map_y(slice[i + 1]),
                    1.9,
                    col,
                    0.96,
                );
            }
        }
    }

    /// Packs the logical canvas scaled by `scale` into swap-chain bytes.
    pub(super) fn pack(&self, scale: u32, format: DXGI_FORMAT, pitch: usize) -> Vec<u8> {
        let bytes = bpp(format).unwrap_or(4);
        let s = scale as usize;
        let out_h = self.h * s;
        let mut out = vec![0u8; pitch * out_h];
        let mut enc = [0u8; 8];
        for y in 0..self.h {
            let top = (y * s) * pitch;
            for x in 0..self.w {
                encode(format, self.rgb[y * self.w + x], &mut enc[..bytes]);
                for i in 0..s {
                    let at = top + (x * s + i) * bytes;
                    out[at..at + bytes].copy_from_slice(&enc[..bytes]);
                }
            }
            for i in 1..s {
                out.copy_within(top..top + pitch, top + i * pitch);
            }
        }
        out
    }
}

trait FloatAddSub {
    fn add(self, v: f32) -> f32;
    fn sub(self, v: f32) -> f32;
}
impl FloatAddSub for f32 {
    #[inline]
    fn add(self, v: f32) -> f32 {
        self + v
    }
    #[inline]
    fn sub(self, v: f32) -> f32 {
        self - v
    }
}
