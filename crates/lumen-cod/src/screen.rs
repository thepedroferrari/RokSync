//! Whole-frame statistics plus a few small HUD patches from a downscaled BGRA desktop frame
//! (DXGI Desktop Duplication), assuming the game fills the screen. The whole frame is sampled on
//! a ≤240 px wide grid; the three HUD patches (under 8 % of the frame) are read at full
//! resolution. All values in [`ScreenFeatures`] are 0..1; effect thresholds live in `lib.rs`.

use crate::Fx;
use lumen_screen::Frame;

/// A screen region in normalized coordinates (0..1, origin top-left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneRect {
    pub name: &'static str,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl ZoneRect {
    const fn new(name: &'static str, x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { name, x, y, w, h }
    }

    /// Pixel bounds `(x0, x1, y0, y1)` (end-exclusive) of this zone in a `w`×`h` frame; never empty.
    fn px(&self, w: usize, h: usize) -> (usize, usize, usize, usize) {
        let span = |o: f32, l: f32, n: usize| {
            let a = ((o * n as f32) as usize).min(n.saturating_sub(1));
            let b = (((o + l) * n as f32).ceil() as usize).clamp(a + 1, n);
            (a, b)
        };
        let (x0, x1) = span(self.x, self.w, w);
        let (y0, y1) = span(self.y, self.h, h);
        (x0, x1, y0, y1)
    }
}

/// Thickness of the edge band read for the red damage vignette (fraction of width / height).
pub const EDGE_X: f32 = 0.12;
pub const EDGE_Y: f32 = 0.14;

const WHOLE: ZoneRect = ZoneRect::new("whole screen", 0.0, 0.0, 1.0, 1.0);
const EDGES: [ZoneRect; 4] = [
    ZoneRect::new("top edge", 0.0, 0.0, 1.0, EDGE_Y),
    ZoneRect::new("bottom edge", 0.0, 1.0 - EDGE_Y, 1.0, EDGE_Y),
    ZoneRect::new("left edge", 0.0, 0.0, EDGE_X, 1.0),
    ZoneRect::new("right edge", 1.0 - EDGE_X, 0.0, EDGE_X, 1.0),
];
/// Square patch around the crosshair (square on a 16:9 screen): where hit markers appear.
pub const HIT_ZONE: ZoneRect = ZoneRect::new("crosshair", 0.44, 0.3933, 0.12, 0.2133);
/// Strip below the crosshair: where the kill pop-up / "+100" appears.
pub const KILL_ZONE: ZoneRect = ZoneRect::new("below crosshair", 0.36, 0.64, 0.28, 0.11);
/// Bottom-right corner: the scorestreak icons.
pub const STREAK_ZONE: ZoneRect = ZoneRect::new("streak icons", 0.80, 0.775, 0.165, 0.17);

/// The regions each effect looks at (empty for sound effects).
pub fn zones(fx: Fx) -> &'static [ZoneRect] {
    match fx {
        Fx::Flashed | Fx::Nvg | Fx::Gas => std::slice::from_ref(&WHOLE),
        Fx::Damage | Fx::LowHp => &EDGES,
        Fx::Hit => std::slice::from_ref(&HIT_ZONE),
        Fx::Kill => std::slice::from_ref(&KILL_ZONE),
        Fx::Streak => std::slice::from_ref(&STREAK_ZONE),
        Fx::Explosion | Fx::Firefight => &[],
    }
}

/// Statistics of the crosshair patch ([`HIT_ZONE`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CentreStats {
    pub luma: f32,
    /// Fraction of *off-axis* pixels (the four diagonal quadrants, so the "+" crosshair arms and
    /// dot are ignored) that are near-white: luma ≥ 200/255 and chroma ≤ 48/255. Hit markers
    /// are a white "X", exactly where a plain crosshair has nothing.
    pub white_diag: f32,
    /// Same quadrants, strongly red (r ≥ 140 and r ≥ 2×max(g, b)): the red kill-shot marker.
    pub red_diag: f32,
}

/// Statistics of the HUD regions the calibration targets.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HudStats {
    /// [`KILL_ZONE`]: fraction of bright pixels (luma ≥ 200/255), i.e. pop-up text.
    pub kill_bright: f32,
    /// [`STREAK_ZONE`]: fraction of lit, coloured pixels (luma ≥ 140/255, chroma ≥ 60/255):
    /// a streak icon that "lights up".
    pub streak_lit: f32,
    pub streak_luma: f32,
}

/// Features of one frame. All values 0..1.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ScreenFeatures {
    /// False when the frame was empty or malformed (every other field is then 0).
    pub valid: bool,
    /// Mean Rec.709 luma.
    pub mean_luma: f32,
    /// Standard deviation of luma (contrast). Smoke and gas flatten it.
    pub luma_spread: f32,
    /// Flashbang: fraction of near-white pixels (luma ≥ 224/255, chroma ≤ 36/255).
    /// A CoD flash pushes this to 0.8–1.0; bright daylight scenes rarely pass 0.3.
    pub white_frac: f32,
    /// Mean chroma (max − min of r, g, b).
    pub saturation: f32,
    /// `1 − saturation / 0.20`, clamped: 0 = normal colourful game, 1 = fully grey.
    pub desaturation: f32,
    /// Red fraction (r ≥ 48 and r ≥ 1.6×max(g, b): dark blood red counts) per edge band:
    /// top, bottom, left, right.
    pub red_sides: [f32; 4],
    /// Red fraction in the central 40 %×40 % of the frame (the reference).
    pub red_centre: f32,
    /// Damage / low health: the *weakest* side's red fraction minus the centre's.
    /// The vignette rings the whole screen, so a red wall or door on one side can't fake it,
    /// and a red scene fills the centre too and cancels out. Typical hit: 0.3–0.7.
    pub red_vignette: f32,
    /// Night vision: of the lit pixels (luma ≥ 20/255), the fraction that are green-dominant
    /// (g ≥ 32, g ≥ 1.4×r, g ≥ 1.3×b). NVG frames are ≥ 0.8; a grassy field stays far lower
    /// because sky, guns and HUD are not green. 0 when under 20 % of the frame is lit.
    pub green_mono: f32,
    /// Gas: fraction of muted yellow-green pixels (g ≥ 60, 0.7×g ≤ r ≤ 1.05×g, b ≤ 0.75×g,
    /// chroma 14..120).
    pub haze: f32,
    /// Gas / smoke score: max of "washed out" (low saturation × low contrast × mid luma) and
    /// `haze / 0.6`. The lib only accepts it when the colour drops relative to the recent past.
    pub gas: f32,
    pub centre: CentreStats,
    pub hud: HudStats,
    /// Hit-marker signal: `centre.white_diag − white_frac` (a flashbang can't fake it).
    pub hit_signal: f32,
    /// Kill pop-up signal: `max(hud.kill_bright − white_frac, centre.red_diag)`.
    pub kill_signal: f32,
    /// Scorestreak signal: `hud.streak_lit`.
    pub streak_signal: f32,
}

#[inline]
fn split(p: &[u8]) -> (u32, u32, u32) {
    (p[2] as u32, p[1] as u32, p[0] as u32)
}

/// (luma, chroma) in 0..255 from 8-bit r, g, b. Rec.709 weights in 8.8 fixed point.
#[inline]
fn luma_chroma(r: u32, g: u32, b: u32) -> (u32, u32) {
    let l = (54 * r + 183 * g + 19 * b) >> 8;
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    (l, mx - mn)
}

fn frac(n: u32, d: u32) -> f32 {
    if d == 0 {
        0.0
    } else {
        n as f32 / d as f32
    }
}

/// Analyze one frame. Never panics; a malformed frame gives `ScreenFeatures::default()`.
pub fn analyze(frame: &Frame) -> ScreenFeatures {
    let (w, h) = (frame.width as usize, frame.height as usize);
    if w < 8 || h < 8 || frame.bgra.len() < w * h * 4 {
        return ScreenFeatures::default();
    }
    let stride = w * 4;
    let step = (w / 240).max(1);
    let ex = ((w as f32 * EDGE_X) as usize).max(1);
    let ey = ((h as f32 * EDGE_Y) as usize).max(1);
    let (cx0, cx1, cy0, cy1) = (w * 3 / 10, w * 7 / 10, h * 3 / 10, h * 7 / 10);

    let (mut n, mut sum_l, mut sum_l2, mut sum_c) = (0u32, 0u64, 0u64, 0u64);
    let (mut white, mut lit, mut green, mut haze) = (0u32, 0u32, 0u32, 0u32);
    let mut side_n = [0u32; 4];
    let mut side_red = [0u32; 4];
    let (mut centre_n, mut centre_red) = (0u32, 0u32);

    for y in (0..h).step_by(step) {
        let row = &frame.bgra[y * stride..(y + 1) * stride];
        let top = y < ey;
        let bottom = y >= h - ey;
        let mid_y = (cy0..cy1).contains(&y);
        for x in (0..w).step_by(step) {
            let (r, g, b) = split(&row[x * 4..x * 4 + 4]);
            let (l, c) = luma_chroma(r, g, b);
            n += 1;
            sum_l += l as u64;
            sum_l2 += (l * l) as u64;
            sum_c += c as u64;
            if l >= 224 && c <= 36 {
                white += 1;
            }
            if l >= 20 {
                lit += 1;
                if g >= 32 && g * 10 >= r * 14 && g * 10 >= b * 13 {
                    green += 1;
                }
            }
            if g >= 60
                && r * 100 >= g * 70
                && r * 100 <= g * 105
                && b * 100 <= g * 75
                && (14..=120).contains(&c)
            {
                haze += 1;
            }
            let red = r >= 48 && r * 10 >= g.max(b) * 16;
            let sides = [top, bottom, x < ex, x >= w - ex];
            for (i, &on) in sides.iter().enumerate() {
                if on {
                    side_n[i] += 1;
                    side_red[i] += red as u32;
                }
            }
            if mid_y && (cx0..cx1).contains(&x) {
                centre_n += 1;
                centre_red += red as u32;
            }
        }
    }

    let nf = n.max(1) as f64;
    let mean_l = sum_l as f64 / nf;
    let var_l = (sum_l2 as f64 / nf - mean_l * mean_l).max(0.0);
    let mean_luma = (mean_l / 255.0) as f32;
    let luma_spread = (var_l.sqrt() / 255.0) as f32;
    let saturation = (sum_c as f64 / nf / 255.0) as f32;
    let white_frac = frac(white, n);

    let red_sides = [0, 1, 2, 3].map(|i| frac(side_red[i], side_n[i]));
    let red_centre = frac(centre_red, centre_n);
    let weakest = red_sides.iter().copied().fold(f32::INFINITY, f32::min);
    let red_vignette = (weakest - red_centre).max(0.0);

    let green_mono = if lit * 5 >= n { frac(green, lit) } else { 0.0 };
    let haze = frac(haze, n);
    let mid_luma = if (0.3..=0.88).contains(&mean_luma) {
        1.0
    } else {
        0.0
    };
    let washed = (1.0 - saturation / 0.10).clamp(0.0, 1.0)
        * (1.0 - luma_spread / 0.15).clamp(0.0, 1.0)
        * mid_luma;
    let gas = washed.max((haze / 0.6).clamp(0.0, 1.0));

    let centre = centre_stats(frame, w, h);
    let hud = hud_stats(frame, w, h);
    ScreenFeatures {
        valid: true,
        mean_luma,
        luma_spread,
        white_frac,
        saturation,
        desaturation: (1.0 - saturation / 0.20).clamp(0.0, 1.0),
        red_sides,
        red_centre,
        red_vignette,
        green_mono,
        haze,
        gas,
        centre,
        hud,
        hit_signal: (centre.white_diag - white_frac).max(0.0),
        kill_signal: (hud.kill_bright - white_frac).max(centre.red_diag).max(0.0),
        streak_signal: hud.streak_lit,
    }
}

/// Visits every pixel of a zone at full resolution: `f(x, y, r, g, b)`.
fn each_px(
    frame: &Frame,
    w: usize,
    h: usize,
    z: &ZoneRect,
    mut f: impl FnMut(usize, usize, u32, u32, u32),
) {
    let (x0, x1, y0, y1) = z.px(w, h);
    for y in y0..y1 {
        let row = &frame.bgra[y * w * 4 + x0 * 4..y * w * 4 + x1 * 4];
        for (i, p) in row.as_chunks::<4>().0.iter().enumerate() {
            let (r, g, b) = split(p);
            f(x0 + i, y, r, g, b);
        }
    }
}

fn centre_stats(frame: &Frame, w: usize, h: usize) -> CentreStats {
    let (x0, x1, y0, y1) = HIT_ZONE.px(w, h);
    // twice the distance from the patch centre, compared against 22 % of the patch size
    let (cx2, cy2) = ((x0 + x1 - 1) as i64, (y0 + y1 - 1) as i64);
    let (lim_x, lim_y) = (22 * (x1 - x0) as i64, 22 * (y1 - y0) as i64);
    let (mut n, mut sum_l, mut diag, mut white, mut red) = (0u32, 0u64, 0u32, 0u32, 0u32);
    each_px(frame, w, h, &HIT_ZONE, |x, y, r, g, b| {
        let (l, c) = luma_chroma(r, g, b);
        n += 1;
        sum_l += l as u64;
        let off_axis =
            (2 * x as i64 - cx2).abs() * 100 > lim_x && (2 * y as i64 - cy2).abs() * 100 > lim_y;
        if off_axis {
            diag += 1;
            white += (l >= 200 && c <= 48) as u32;
            red += (r >= 140 && r * 10 >= g.max(b) * 20) as u32;
        }
    });
    CentreStats {
        luma: sum_l as f32 / n.max(1) as f32 / 255.0,
        white_diag: frac(white, diag),
        red_diag: frac(red, diag),
    }
}

fn hud_stats(frame: &Frame, w: usize, h: usize) -> HudStats {
    let (mut kn, mut kb) = (0u32, 0u32);
    each_px(frame, w, h, &KILL_ZONE, |_, _, r, g, b| {
        kn += 1;
        kb += (luma_chroma(r, g, b).0 >= 200) as u32;
    });
    let (mut sn, mut slit, mut sl) = (0u32, 0u32, 0u64);
    each_px(frame, w, h, &STREAK_ZONE, |_, _, r, g, b| {
        let (l, c) = luma_chroma(r, g, b);
        sn += 1;
        sl += l as u64;
        slit += (l >= 140 && c >= 60) as u32;
    });
    HudStats {
        kill_bright: frac(kb, kn),
        streak_lit: frac(slit, sn),
        streak_luma: sl as f32 / sn.max(1) as f32 / 255.0,
    }
}

#[cfg(test)]
pub(crate) mod synth {
    use lumen_screen::Frame;

    pub const W: u32 = 480;
    pub const H: u32 = 270;

    /// A frame whose pixel colour (r, g, b 0..255) is `f(x, y)`.
    pub fn frame(f: impl Fn(u32, u32) -> [u8; 3]) -> Frame {
        let mut bgra = Vec::with_capacity((W * H * 4) as usize);
        for y in 0..H {
            for x in 0..W {
                let [r, g, b] = f(x, y);
                bgra.extend_from_slice(&[b, g, r, 255]);
            }
        }
        Frame {
            width: W,
            height: H,
            bgra,
        }
    }

    /// A plausible game scene: blue-grey sky, olive horizon, brown ground, some texture.
    pub fn scene(x: u32, y: u32) -> [u8; 3] {
        let n = ((x * 7 + y * 13) % 23) as u8;
        if y < H / 2 {
            [90 + n, 120 + n, 160 + n]
        } else if y < H * 3 / 5 {
            [100 + n, 105 + n, 70]
        } else {
            [110 + n, 85 + n, 60 + n / 2]
        }
    }

    pub fn neutral() -> Frame {
        frame(scene)
    }

    pub fn white() -> Frame {
        frame(|_, _| [250, 250, 248])
    }

    /// The scene with a dark-red vignette `band` px deep all around.
    pub fn red_edges(band: u32) -> Frame {
        frame(|x, y| {
            let d = x.min(y).min(W - 1 - x).min(H - 1 - y);
            if d < band {
                [150, 18, 22]
            } else {
                scene(x, y)
            }
        })
    }

    /// Night vision: green monochrome version of the scene.
    pub fn nvg() -> Frame {
        frame(|x, y| {
            let [r, g, b] = scene(x, y);
            let l = ((r as u32 + g as u32 + b as u32) / 3) as u8;
            [l / 5, l, l / 4]
        })
    }

    /// The scene with a white "X" hit marker around the screen centre.
    pub fn hit_marker() -> Frame {
        frame(|x, y| {
            let (dx, dy) = (x as i32 - W as i32 / 2, y as i32 - H as i32 / 2);
            let on_x = (dx.abs() - dy.abs()).abs() <= 1 && (5..=18).contains(&dx.abs());
            if on_x {
                [255, 255, 255]
            } else {
                scene(x, y)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::synth::*;
    use super::*;

    #[test]
    fn neutral_scene_reads_quiet() {
        let f = analyze(&neutral());
        assert!(f.valid);
        assert!(f.white_frac < 0.05, "{f:?}");
        assert!(f.red_vignette < 0.05, "{f:?}");
        assert!(f.green_mono < 0.2, "{f:?}");
        assert!(f.gas < 0.3, "{f:?}");
        assert!(f.hit_signal < 0.02, "{f:?}");
    }

    #[test]
    fn white_frame_is_a_flash() {
        let f = analyze(&white());
        assert!(f.white_frac > 0.95 && f.mean_luma > 0.95);
        assert_eq!(f.gas, 0.0, "a flash is not smoke");
        assert_eq!(f.hit_signal, 0.0, "a flash is not a hit marker");
    }

    #[test]
    fn red_edges_are_a_vignette_but_one_red_wall_is_not() {
        let f = analyze(&red_edges(40));
        assert!(f.red_vignette > 0.5, "{f:?}");
        // a red wall filling the left third only
        let wall = frame(|x, y| {
            if x < W / 3 {
                [160, 20, 20]
            } else {
                scene(x, y)
            }
        });
        assert!(analyze(&wall).red_vignette < 0.05);
        // an all-red scene cancels against the centre
        assert!(analyze(&frame(|_, _| [170, 20, 25])).red_vignette < 0.05);
    }

    #[test]
    fn green_mono_is_night_vision() {
        assert!(analyze(&nvg()).green_mono > 0.9);
    }

    #[test]
    fn hit_marker_lights_the_crosshair_diagonals_only() {
        let f = analyze(&hit_marker());
        assert!(f.hit_signal > 0.05, "{f:?}");
        // a plain "+" crosshair does not count
        let plus = frame(|x, y| {
            let (dx, dy) = (x as i32 - W as i32 / 2, y as i32 - H as i32 / 2);
            if (dx.abs() <= 1 && dy.abs() < 20) || (dy.abs() <= 1 && dx.abs() < 20) {
                [255, 255, 255]
            } else {
                scene(x, y)
            }
        });
        assert!(analyze(&plus).hit_signal < 0.01);
    }

    #[test]
    fn malformed_frames_do_not_panic() {
        assert!(!analyze(&Frame::default()).valid);
        assert!(
            !analyze(&Frame {
                width: 100,
                height: 100,
                bgra: vec![0; 10]
            })
            .valid
        );
        let tiny = Frame {
            width: 8,
            height: 8,
            bgra: vec![255; 8 * 8 * 4],
        };
        assert!(analyze(&tiny).valid);
    }

    #[test]
    fn every_screen_effect_has_a_zone() {
        for fx in Fx::ALL {
            assert_eq!(
                zones(fx).is_empty(),
                fx.source() == crate::Source::Sound,
                "{fx:?}"
            );
            for z in zones(fx) {
                assert!(
                    z.x >= 0.0 && z.y >= 0.0 && z.x + z.w <= 1.0001 && z.y + z.h <= 1.0001,
                    "{z:?}"
                );
            }
        }
    }

    #[test]
    fn analyze_is_fast() {
        let f = neutral();
        let t = std::time::Instant::now();
        for _ in 0..50 {
            std::hint::black_box(analyze(std::hint::black_box(&f)));
        }
        let per = t.elapsed().as_secs_f64() / 50.0;
        // bound covers debug builds; release is ~10× faster
        assert!(per < 0.02, "analyze took {:.3} ms", per * 1e3);
        eprintln!("analyze 480x270: {:.3} ms", per * 1e3);
    }
}
