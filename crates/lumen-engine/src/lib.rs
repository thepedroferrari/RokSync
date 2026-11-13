//! Turns a `Program` into per-channel colours for one instant in time. Pure functions, no I/O.

use lumen_hue::Rgb;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    LeftRight,
    FrontBack,
    BottomTop,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Look {
    Solid(Rgb),
    /// Stops spread along an axis by each channel's position.
    Gradient { stops: Vec<Rgb>, axis: Axis },
    PerChannel(BTreeMap<u8, Rgb>),
    /// Colours distributed over the channels in left-to-right order.
    Palette(Vec<Rgb>),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Motion {
    Static,
    /// `spread` 0 = all in unison, 1 = a wave across the room.
    Breathe { period_s: f32, depth: f32, spread: f32 },
    Drift { period_s: f32 },
    Flicker { amount: f32, speed: f32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub look: Look,
    pub motion: Motion,
    /// 0..=1 master brightness
    pub brightness: f32,
}

impl Default for Program {
    fn default() -> Self {
        Self { look: Look::Solid(Rgb::new(1.0, 0.55, 0.2)), motion: Motion::Static, brightness: 1.0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ChannelPos {
    pub id: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Effects layered on top of the rendered program, applied in order.
#[derive(Clone, Copy, Debug)]
pub enum Overlay {
    Tint { color: Rgb, alpha: f32 },
    Desaturate(f32),
    Dim(f32),
}

pub fn render(p: &Program, chans: &[ChannelPos], t: f32) -> Vec<(u8, Rgb)> {
    let n = chans.len().max(1);
    let axis_of = |c: &ChannelPos, a: Axis| match a {
        Axis::LeftRight => c.x,
        Axis::FrontBack => c.y,
        Axis::BottomTop => c.z,
    };
    let drift = match p.motion {
        Motion::Drift { period_s } if period_s > 0.0 => t / period_s,
        _ => 0.0,
    };

    // order channels left→right for palette distribution so neighbours get neighbouring colours
    let mut order: Vec<usize> = (0..chans.len()).collect();
    order.sort_by(|&a, &b| chans[a].x.partial_cmp(&chans[b].x).unwrap_or(std::cmp::Ordering::Equal));
    let mut rank = vec![0usize; chans.len()];
    for (r, &i) in order.iter().enumerate() {
        rank[i] = r;
    }

    chans
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let base = match &p.look {
                Look::Solid(col) => *col,
                Look::PerChannel(m) => m.get(&c.id).copied().unwrap_or(Rgb::BLACK),
                Look::Gradient { stops, axis } => {
                    let (lo, hi) = chans.iter().fold((f32::MAX, f32::MIN), |(lo, hi), ch| {
                        let v = axis_of(ch, *axis);
                        (lo.min(v), hi.max(v))
                    });
                    let u = if hi - lo > 1e-4 { (axis_of(c, *axis) - lo) / (hi - lo) } else { 0.5 };
                    if drift != 0.0 {
                        sample_cyclic(stops, u + drift)
                    } else {
                        sample_linear(stops, u)
                    }
                }
                Look::Palette(cols) => {
                    let u = rank[i] as f32 / n as f32;
                    sample_cyclic(cols, u + drift)
                }
            };
            let k = match p.motion {
                Motion::Breathe { period_s, depth, spread } if period_s > 0.0 => {
                    let phase = t / period_s - spread * rank[i] as f32 / n as f32;
                    let wave = 0.5 - 0.5 * (TAU * phase).cos(); // 0..1
                    1.0 - depth.clamp(0.0, 1.0) * (1.0 - wave)
                }
                Motion::Flicker { amount, speed } => 1.0 - amount.clamp(0.0, 1.0) * value_noise(i as u32, t * speed.max(0.1)),
                _ => 1.0,
            };
            (c.id, base.scale(k * p.brightness.clamp(0.0, 1.0)))
        })
        .collect()
}

/// Gain 0 = off, 1 = full strength.
pub fn scale_overlays(overlays: &mut [Overlay], gain: f32) {
    let g = gain.clamp(0.0, 1.0);
    for o in overlays.iter_mut() {
        match o {
            Overlay::Tint { alpha, .. } => *alpha *= g,
            Overlay::Desaturate(a) | Overlay::Dim(a) => *a *= g,
        }
    }
}

pub fn apply_overlays(frame: &mut [(u8, Rgb)], overlays: &[Overlay]) {
    for o in overlays {
        for (_, c) in frame.iter_mut() {
            *c = match *o {
                Overlay::Tint { color, alpha } => c.lerp(color, alpha),
                Overlay::Desaturate(a) => c.desaturate(a),
                Overlay::Dim(a) => c.scale(1.0 - a.clamp(0.0, 1.0)),
            };
        }
    }
}

pub fn sample_linear(stops: &[Rgb], u: f32) -> Rgb {
    match stops.len() {
        0 => Rgb::BLACK,
        1 => stops[0],
        n => {
            let x = u.clamp(0.0, 1.0) * (n - 1) as f32;
            let i = (x.floor() as usize).min(n - 2);
            stops[i].lerp(stops[i + 1], x - i as f32)
        }
    }
}

pub fn sample_cyclic(stops: &[Rgb], u: f32) -> Rgb {
    match stops.len() {
        0 => Rgb::BLACK,
        1 => stops[0],
        n => {
            let x = u.rem_euclid(1.0) * n as f32;
            let i = x.floor() as usize % n;
            let f = x - x.floor();
            let f = f * f * (3.0 - 2.0 * f); // smoothstep
            stops[i].lerp(stops[(i + 1) % n], f)
        }
    }
}

fn hash(a: u32, b: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Smooth 1D noise in 0..1, independent per channel.
fn value_noise(ch: u32, t: f32) -> f32 {
    let i = t.floor();
    let f = t - i;
    let f = f * f * (3.0 - 2.0 * f);
    let a = hash(ch, i as u32);
    let b = hash(ch, i as u32 + 1);
    a + (b - a) * f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chans() -> Vec<ChannelPos> {
        (0..4).map(|i| ChannelPos { id: i, x: -1.0 + i as f32 * 2.0 / 3.0, y: 0.0, z: 0.0 }).collect()
    }

    #[test]
    fn gradient_ends_match_stops() {
        let red = Rgb::new(1.0, 0.0, 0.0);
        let blue = Rgb::new(0.0, 0.0, 1.0);
        let p = Program { look: Look::Gradient { stops: vec![red, blue], axis: Axis::LeftRight }, motion: Motion::Static, brightness: 1.0 };
        let f = render(&p, &chans(), 0.0);
        assert_eq!(f[0].1, red);
        assert_eq!(f[3].1, blue);
    }

    #[test]
    fn breathe_stays_in_range() {
        let p = Program { look: Look::Solid(Rgb::WHITE), motion: Motion::Breathe { period_s: 4.0, depth: 0.8, spread: 0.0 }, brightness: 1.0 };
        for step in 0..100 {
            for (_, c) in render(&p, &chans(), step as f32 * 0.1) {
                assert!(c.r >= 0.199 && c.r <= 1.0001);
            }
        }
    }
}
