use crate::Frame;
use lumen_engine::ChannelPos;
use lumen_hue::Rgb;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Normalised rectangle (0..1) inside the captured monitor.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Default for NRect {
    fn default() -> Self {
        NRect::FULL
    }
}

impl NRect {
    pub const FULL: NRect = NRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

    pub fn clamp(self) -> NRect {
        let x = self.x.clamp(0.0, 0.99);
        let y = self.y.clamp(0.0, 0.99);
        NRect { x, y, w: self.w.clamp(0.01, 1.0 - x), h: self.h.clamp(0.01, 1.0 - y) }
    }

    /// Largest centred sub-rect with the given aspect (width/height), in pixel space of `px_w × px_h`.
    pub fn fit_aspect(self, aspect: f32, px_w: f32, px_h: f32) -> NRect {
        let (w, h) = (self.w * px_w, self.h * px_h);
        let (nw, nh) = if w / h > aspect { (h * aspect, h) } else { (w, w / aspect) };
        NRect {
            x: self.x + (w - nw) / 2.0 / px_w,
            y: self.y + (h - nh) / 2.0 / px_h,
            w: nw / px_w,
            h: nh / px_h,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleMode {
    Average,
    /// Favours saturated, bright pixels: a red explosion on a grey sky reads as red.
    Vibrant,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScreenSettings {
    pub region: NRect,
    /// Content aspect lock, e.g. 16/9 or 2.39 (None = use the region as is).
    pub aspect: Option<f32>,
    pub detect_bars: bool,
    pub mode: SampleMode,
    /// Size of the area each light reads, 0.05..1 of the content width.
    pub zone: f32,
    /// 0 = instant, 1 = very smooth
    pub smoothing: f32,
    /// 0..2, 1 = unchanged
    pub saturation: f32,
    pub min_brightness: f32,
    pub max_brightness: f32,
}

impl Default for ScreenSettings {
    fn default() -> Self {
        Self {
            region: NRect::FULL,
            aspect: None,
            detect_bars: true,
            mode: SampleMode::Vibrant,
            zone: 0.3,
            smoothing: 0.35,
            saturation: 1.15,
            min_brightness: 0.0,
            max_brightness: 1.0,
        }
    }
}

/// Finds letterbox/pillarbox bars and only changes its mind when the evidence is stable,
/// so fades to black and dark scenes don't make the crop jump around.
#[derive(Default)]
pub struct BarDetector {
    current: (u32, u32, u32, u32), // top, bottom, left, right in pixels of the region
    candidate: (u32, u32, u32, u32),
    streak: u32,
}

const BLACK: f32 = 0.055;

impl BarDetector {
    fn line_is_black(frame: &Frame, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>) -> bool {
        let mut bright = 0u32;
        let mut total = 0u32;
        for y in ys {
            for x in xs.clone() {
                let [r, g, b] = frame.px(x, y);
                if r.max(g).max(b) > BLACK {
                    bright += 1;
                }
                total += 1;
            }
        }
        total > 0 && bright * 50 < total // < 2 % non-black pixels (subtitles, noise)
    }

    /// Returns the content rectangle (pixels: x0, y0, x1, y1) inside `r`.
    pub fn update(&mut self, frame: &Frame, r: (u32, u32, u32, u32)) -> (u32, u32, u32, u32) {
        let (x0, y0, x1, y1) = r;
        let (w, h) = (x1 - x0, y1 - y0);
        let max_v = h * 30 / 100;
        let max_h = w * 30 / 100;

        let mut top = 0;
        while top < max_v && Self::line_is_black(frame, x0..x1, y0 + top..y0 + top + 1) {
            top += 1;
        }
        // whole frame black: keep the current crop
        if top >= max_v && Self::line_is_black(frame, x0..x1, y0 + h / 2 - 1..y0 + h / 2 + 1) {
            self.streak = 0;
            return self.apply(r);
        }
        let mut bottom = 0;
        while bottom < max_v && Self::line_is_black(frame, x0..x1, y1 - bottom - 1..y1 - bottom) {
            bottom += 1;
        }
        let mut left = 0;
        while left < max_h && Self::line_is_black(frame, x0 + left..x0 + left + 1, y0..y1) {
            left += 1;
        }
        let mut right = 0;
        while right < max_h && Self::line_is_black(frame, x1 - right - 1..x1 - right, y0..y1) {
            right += 1;
        }
        let found = (top, bottom, left, right);

        let near = |a: (u32, u32, u32, u32), b: (u32, u32, u32, u32)| {
            a.0.abs_diff(b.0) <= 1 && a.1.abs_diff(b.1) <= 1 && a.2.abs_diff(b.2) <= 1 && a.3.abs_diff(b.3) <= 1
        };
        // content appeared inside the current bars → shrink the crop immediately
        if found.0 < self.current.0 || found.1 < self.current.1 || found.2 < self.current.2 || found.3 < self.current.3 {
            let shrink = (
                found.0.min(self.current.0),
                found.1.min(self.current.1),
                found.2.min(self.current.2),
                found.3.min(self.current.3),
            );
            self.current = shrink;
        }
        if near(found, self.candidate) {
            self.streak += 1;
        } else {
            self.candidate = found;
            self.streak = 0;
        }
        // grow the crop only after ~1 s of consistent evidence
        if self.streak >= 25 {
            self.current = self.candidate;
        }
        self.apply(r)
    }

    fn apply(&self, r: (u32, u32, u32, u32)) -> (u32, u32, u32, u32) {
        let (x0, y0, x1, y1) = r;
        let (t, b, l, rr) = self.current;
        let nx0 = (x0 + l).min(x1 - 1);
        let ny0 = (y0 + t).min(y1 - 1);
        ((nx0), (ny0), (x1 - rr).max(nx0 + 1), (y1 - b).max(ny0 + 1))
    }
}

pub struct Sampler {
    pub bars: BarDetector,
    smoothed: HashMap<u8, Rgb>,
    /// Last content rect, normalised to the frame (for drawing in the UI).
    pub content: NRect,
}

impl Default for Sampler {
    fn default() -> Self {
        Self { bars: BarDetector::default(), smoothed: HashMap::new(), content: NRect::FULL }
    }
}

fn boost_saturation(c: Rgb, k: f32) -> Rgb {
    let l = c.luma();
    let gray = Rgb::new(l, l, l);
    let out = Rgb::new(gray.r + (c.r - gray.r) * k, gray.g + (c.g - gray.g) * k, gray.b + (c.b - gray.b) * k);
    Rgb::new(out.r.clamp(0.0, 1.0), out.g.clamp(0.0, 1.0), out.b.clamp(0.0, 1.0))
}

impl Sampler {
    /// `dt`: seconds since the last call (drives smoothing).
    pub fn process(&mut self, frame: &Frame, s: &ScreenSettings, chans: &[ChannelPos], dt: f32) -> Vec<(u8, Rgb)> {
        let fw = frame.width as f32;
        let fh = frame.height as f32;
        let mut region = s.region.clamp();
        if let Some(a) = s.aspect {
            region = region.fit_aspect(a, fw, fh);
        }
        let px = (
            (region.x * fw) as u32,
            (region.y * fh) as u32,
            ((region.x + region.w) * fw).ceil().min(fw) as u32,
            ((region.y + region.h) * fh).ceil().min(fh) as u32,
        );
        let px = (px.0, px.1, px.2.max(px.0 + 2), px.3.max(px.1 + 2));
        let (x0, y0, x1, y1) = if s.detect_bars { self.bars.update(frame, px) } else { px };
        self.content = NRect { x: x0 as f32 / fw, y: y0 as f32 / fh, w: (x1 - x0) as f32 / fw, h: (y1 - y0) as f32 / fh };
        let (cw, ch) = ((x1 - x0) as f32, (y1 - y0) as f32);

        // channel → screen position. x = left/right. Height (z) maps to top/bottom only if lights
        // actually differ in height; otherwise each light reads its whole column.
        let (zmin, zmax) = chans.iter().fold((f32::MAX, f32::MIN), |(a, b), c| (a.min(c.z), b.max(c.z)));
        let use_z = zmax - zmin > 0.3;
        let zone_w = s.zone.clamp(0.05, 1.0);

        let alpha = if s.smoothing <= 0.0 { 1.0 } else { 1.0 - (-dt / (0.02 + 0.6 * s.smoothing.powi(2))).exp() };

        chans
            .iter()
            .map(|c| {
                let u = ((c.x + 1.0) / 2.0).clamp(0.0, 1.0);
                let (v0, v1) = if use_z {
                    let v = 1.0 - (c.z - zmin) / (zmax - zmin);
                    ((v - zone_w * 0.7).max(0.0), (v + zone_w * 0.7).min(1.0))
                } else {
                    (0.0, 1.0)
                };
                let (u0, u1) = ((u - zone_w / 2.0).max(0.0), (u + zone_w / 2.0).min(1.0));
                let sx0 = x0 + (u0 * cw) as u32;
                let sx1 = (x0 + (u1 * cw).ceil() as u32).clamp(sx0 + 1, x1);
                let sy0 = y0 + (v0 * ch) as u32;
                let sy1 = (y0 + (v1 * ch).ceil() as u32).clamp(sy0 + 1, y1);

                let (mut r, mut g, mut b, mut wsum) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                for y in sy0..sy1 {
                    for x in sx0..sx1 {
                        let p = frame.px(x, y);
                        let w = match s.mode {
                            SampleMode::Average => 1.0,
                            SampleMode::Vibrant => {
                                let mx = p[0].max(p[1]).max(p[2]);
                                let mn = p[0].min(p[1]).min(p[2]);
                                let sat = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
                                0.05 + mx * (0.3 + 3.0 * sat * sat)
                            }
                        };
                        r += p[0] * w;
                        g += p[1] * w;
                        b += p[2] * w;
                        wsum += w;
                    }
                }
                let mut col = if wsum > 0.0 { Rgb::new(r / wsum, g / wsum, b / wsum) } else { Rgb::BLACK };
                col = boost_saturation(col, s.saturation);
                let v = col.max_channel();
                let target_v = s.min_brightness + (s.max_brightness - s.min_brightness) * v;
                col = if v > 1e-4 { col.scale(target_v / v) } else { Rgb::new(target_v, target_v, target_v) };

                let prev = self.smoothed.get(&c.id).copied().unwrap_or(col);
                let out = prev.lerp(col, alpha);
                self.smoothed.insert(c.id, out);
                (c.id, out)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 64×36 frame: black bars top/bottom (6 px each), left half red, right half blue.
    fn letterboxed() -> Frame {
        let (w, h) = (64u32, 36u32);
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        for y in 6..30 {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if x < 32 {
                    bgra[i + 2] = 230;
                } else {
                    bgra[i] = 230;
                }
                bgra[i + 3] = 255;
            }
        }
        Frame { width: w, height: h, bgra }
    }

    #[test]
    fn bars_detected_after_stable_frames() {
        let f = letterboxed();
        let mut d = BarDetector::default();
        let mut r = (0, 0, 0, 0);
        for _ in 0..30 {
            r = d.update(&f, (0, 0, 64, 36));
        }
        assert_eq!((r.1, r.3), (6, 30));
    }

    #[test]
    fn black_frame_keeps_crop() {
        let f = letterboxed();
        let mut d = BarDetector::default();
        for _ in 0..30 {
            d.update(&f, (0, 0, 64, 36));
        }
        let black = Frame { width: 64, height: 36, bgra: vec![0; 64 * 36 * 4] };
        let r = d.update(&black, (0, 0, 64, 36));
        assert_eq!((r.1, r.3), (6, 30));
    }

    #[test]
    fn left_light_reads_left_side() {
        let f = letterboxed();
        let chans = [ChannelPos { id: 0, x: -1.0, y: 0.0, z: 0.0 }, ChannelPos { id: 1, x: 1.0, y: 0.0, z: 0.0 }];
        let mut s = Sampler::default();
        let set = ScreenSettings { smoothing: 0.0, saturation: 1.0, ..Default::default() };
        let out = s.process(&f, &set, &chans, 0.02);
        assert!(out[0].1.r > 0.8 && out[0].1.b < 0.1, "{:?}", out[0]);
        assert!(out[1].1.b > 0.8 && out[1].1.r < 0.1, "{:?}", out[1]);
    }
}
