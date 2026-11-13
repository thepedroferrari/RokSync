//! Sound sync: a capture service that runs only while something asks for it, and the reactor that
//! turns PC sound into light. The reactor and [`apply`] allocate nothing per tick.

use crate::config::SoundStyle;
use crate::sync::Source;
use lumen_cod::{AudioHandle, AudioLevels, SharedAudio};
use lumen_engine::ChannelPos;
use lumen_hue::Rgb;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Req {
    device: Option<String>,
    until: Option<Instant>,
}

/// Capture control shared by the stream thread, the UI and the capture thread.
pub struct SoundCtl {
    pub audio: SharedAudio,
    req: Mutex<Req>,
}

pub type SharedSound = Arc<SoundCtl>;

impl SoundCtl {
    /// Keeps capture running for `secs` longer. Callers repeat it while they need sound, so a
    /// caller that stops (a stream ending, a page closing) can never leave capture on.
    pub fn want(&self, secs: f32) {
        let until = Instant::now() + Duration::from_secs_f32(secs);
        let mut r = self.req.lock().unwrap();
        if r.until.is_none_or(|u| u < until) {
            r.until = Some(until);
        }
    }

    /// None follows whatever Windows plays to.
    pub fn set_device(&self, d: Option<String>) {
        self.req.lock().unwrap().device = d;
    }

    pub fn levels(&self) -> AudioLevels {
        self.audio.lock().map(|l| l.numbers()).unwrap_or_default()
    }

    /// (listening, device name, status)
    pub fn state(&self) -> (bool, String, String) {
        self.audio.lock().map(|l| (l.running, l.device.clone(), l.status.clone())).unwrap_or_default()
    }

    #[cfg(test)]
    pub fn idle() -> SharedSound {
        Arc::new(SoundCtl { audio: SharedAudio::default(), req: Mutex::new(Req::default()) })
    }
}

pub fn spawn(device: Option<String>) -> SharedSound {
    let ctl = Arc::new(SoundCtl { audio: SharedAudio::default(), req: Mutex::new(Req { device, until: None }) });
    let c = ctl.clone();
    let _ = std::thread::Builder::new().name("sound".into()).spawn(move || run(c));
    ctl
}

fn run(ctl: SharedSound) {
    let mut open: Option<(AudioHandle, Option<String>)> = None;
    let mut retry_at: Option<Instant> = None;
    loop {
        let (want, device) = {
            let r = ctl.req.lock().unwrap();
            (r.until.is_some_and(|u| u > Instant::now()), r.device.clone())
        };
        if !want {
            open = None;
            retry_at = None;
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        if open.as_ref().is_some_and(|(_, d)| *d != device) {
            open = None;
            retry_at = None;
        }
        if open.is_none() && retry_at.is_none_or(|t| Instant::now() >= t) {
            match lumen_cod::spawn_loopback_on(ctl.audio.clone(), device.clone()) {
                Ok(h) => open = Some((h, device)),
                Err(e) => {
                    if let Ok(mut l) = ctl.audio.lock() {
                        l.running = false;
                        l.status = format!("{e:#}");
                    }
                    retry_at = Some(Instant::now() + Duration::from_secs(3));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Bands below these RMS levels count as silence (bass, mids, highs).
const GATE: [f32; 3] = [0.004, 0.003, 0.0015];
/// Auto-gain: each band's reference peak halves in this many seconds of quieter sound.
const PEAK_HALF_LIFE: f32 = 3.0;
/// Envelope release time constants; attack is instant.
const RELEASE: [f32; 3] = [0.16, 0.12, 0.09];
/// A beat: bass this many dB over its running average, at most one per `BEAT_GAP` seconds.
const BEAT_DB: f32 = 5.0;
const BEAT_GAP: f32 = 0.18;
const FLASH_DECAY: f32 = 0.25;

/// Light response to PC sound. Feed it levels at any rate; outputs are 0..1.
#[derive(Clone, Debug)]
pub struct Reactor {
    peak: [f32; 3],
    /// Bass, mids, highs after auto-gain, contrast and release.
    pub env: [f32; 3],
    /// 1 on a beat, decaying.
    pub flash: f32,
    /// Beats so far (wraps).
    pub beats: u32,
    boom_prev: f32,
    t: f32,
    last_beat: f32,
}

impl Default for Reactor {
    fn default() -> Self {
        Self { peak: GATE, env: [0.0; 3], flash: 0.0, beats: 0, boom_prev: 0.0, t: 0.0, last_beat: f32::NEG_INFINITY }
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Reactor {
    pub fn update(&mut self, l: &AudioLevels, now: Instant, dt: f32) {
        let dt = dt.clamp(0.0, 0.25);
        self.t += dt;
        let fresh = l.fresh(now);
        let bands = if fresh { [l.low, l.mid, l.high] } else { [0.0; 3] };
        let keep = 0.5f32.powf(dt / PEAK_HALF_LIFE);
        for (i, &x) in bands.iter().enumerate() {
            self.peak[i] = (self.peak[i] * keep).max(x).max(GATE[i] * 2.0);
            let norm = if x < GATE[i] { 0.0 } else { (x / self.peak[i]).min(1.0) };
            let v = smoothstep(0.2, 0.95, norm);
            self.env[i] = v.max(self.env[i] * (-dt / RELEASE[i]).exp());
        }
        let boom = if fresh { l.boom } else { 0.0 };
        if boom >= BEAT_DB && self.boom_prev < BEAT_DB && self.t - self.last_beat >= BEAT_GAP {
            self.beats = self.beats.wrapping_add(1);
            self.flash = 1.0;
            self.last_beat = self.t;
        } else {
            self.flash *= (-dt / FLASH_DECAY).exp();
        }
        self.boom_prev = boom;
    }

    /// Band level at `u` from left (0, bass) to right (1, highs).
    fn across(&self, u: f32) -> f32 {
        let [b, m, h] = self.env;
        if u < 0.5 {
            b + (m - b) * u * 2.0
        } else {
            m + (h - m) * (u - 0.5) * 2.0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundLayer {
    pub style: SoundStyle,
    /// How hard sound moves the lights, 0..1.
    pub amount: f32,
}

/// Where each channel sits left to right, worked out once per stream.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    /// Per frame index: 0 leftmost .. 1 rightmost, by rank so bands spread evenly.
    pub u: Vec<f32>,
    /// Frame indices from left to right.
    pub order: Vec<usize>,
}

impl Layout {
    pub fn new(chans: &[ChannelPos]) -> Self {
        let mut order: Vec<usize> = (0..chans.len()).collect();
        order.sort_by(|&a, &b| chans[a].x.total_cmp(&chans[b].x).then(chans[a].id.cmp(&chans[b].id)));
        let mut u = vec![0.5; chans.len()];
        let last = chans.len().saturating_sub(1).max(1) as f32;
        for (rank, &i) in order.iter().enumerate() {
            u[i] = if chans.len() == 1 { 0.0 } else { rank as f32 / last };
        }
        Self { u, order }
    }
}

/// Moves a frame with the sound. Mood colours: Pulse, Spectrum or Beat. Screen colours: punch,
/// a lift on bass and hits that never clips a colour's hue. `scratch` is reused between ticks.
pub fn apply(frame: &mut [(u8, Rgb)], lay: &Layout, r: &Reactor, layer: SoundLayer, source: Source, scratch: &mut Vec<Rgb>) {
    let a = layer.amount.clamp(0.0, 1.0);
    if a <= 0.0 || frame.len() != lay.u.len() {
        return;
    }
    match source {
        Source::Screen => {
            let hit = r.env[0].max(0.6 * r.env[2]);
            let dip = 0.25 * a;
            let k = (1.0 - dip) + (dip + 1.6 * a) * hit;
            for (_, c) in frame.iter_mut() {
                let m = c.max_channel();
                if m > 1e-4 {
                    *c = c.scale(k.min(1.0 / m));
                }
            }
        }
        Source::Look => {
            let floor = 1.0 - 0.9 * a;
            match layer.style {
                SoundStyle::Pulse => {
                    let k = floor + (1.0 - floor) * r.env[0];
                    for (_, c) in frame.iter_mut() {
                        *c = c.scale(k);
                    }
                }
                SoundStyle::Spectrum => {
                    for (i, (_, c)) in frame.iter_mut().enumerate() {
                        *c = c.scale(floor + (1.0 - floor) * r.across(lay.u[i]));
                    }
                }
                SoundStyle::Beat => {
                    let n = lay.order.len();
                    let shift = r.beats as usize % n.max(1);
                    if shift != 0 {
                        scratch.clear();
                        scratch.extend(lay.order.iter().map(|&i| frame[i].1));
                        for (j, &i) in lay.order.iter().enumerate() {
                            frame[i].1 = scratch[(j + shift) % n];
                        }
                    }
                    let k = floor + (1.0 - floor) * r.flash.max(0.5 * r.env[0]);
                    for (_, c) in frame.iter_mut() {
                        *c = c.scale(k);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lv(low: f32, mid: f32, high: f32, boom: f32, now: Instant) -> AudioLevels {
        AudioLevels { updated: Some(now), low, mid, high, boom, ..Default::default() }
    }

    fn chans(n: usize) -> Vec<ChannelPos> {
        (0..n).map(|i| ChannelPos { id: i as u8, x: 1.0 - i as f32 * 0.5, y: 0.0, z: 0.0 }).collect()
    }

    #[test]
    fn loud_bass_after_quiet_drives_the_bass_envelope_and_silence_lets_go() {
        let mut r = Reactor::default();
        let t = Instant::now();
        for i in 0..50 {
            r.update(&lv(0.01, 0.01, 0.0, 0.0, t + Duration::from_millis(i * 20)), t + Duration::from_millis(i * 20), 0.02);
        }
        r.update(&lv(0.3, 0.01, 0.0, 12.0, t + Duration::from_secs(1)), t + Duration::from_secs(1), 0.02);
        assert!(r.env[0] > 0.95, "{r:?}");
        assert!(r.env[2] < 0.2, "{r:?}");
        let later = t + Duration::from_secs(5);
        for i in 0..100 {
            r.update(&AudioLevels::default(), later + Duration::from_millis(i * 20), 0.02);
        }
        assert!(r.env.iter().all(|e| *e < 0.01), "{r:?}");
    }

    #[test]
    fn beats_need_a_rise_and_a_gap() {
        let mut r = Reactor::default();
        let t = Instant::now();
        let mut prev = 0u64;
        let mut at = |r: &mut Reactor, ms: u64, boom: f32| {
            let now = t + Duration::from_millis(ms);
            r.update(&lv(0.2, 0.0, 0.0, boom, now), now, (ms - prev) as f32 / 1000.0);
            prev = ms;
        };
        at(&mut r, 0, 9.0);
        at(&mut r, 20, 9.0);
        assert_eq!(r.beats, 1, "a held peak is one beat");
        at(&mut r, 40, 0.0);
        at(&mut r, 60, 9.0);
        assert_eq!(r.beats, 1, "too soon after the last one");
        at(&mut r, 80, 0.0);
        at(&mut r, 300, 9.0);
        assert_eq!(r.beats, 2);
        assert!(r.flash > 0.99);
    }

    #[test]
    fn spectrum_puts_bass_on_the_left() {
        let c = chans(3);
        let lay = Layout::new(&c);
        assert_eq!(lay.order, vec![2, 1, 0]);
        let r = Reactor { env: [1.0, 0.0, 0.0], ..Default::default() };
        let mut f: Vec<(u8, Rgb)> = c.iter().map(|ch| (ch.id, Rgb::WHITE)).collect();
        apply(&mut f, &lay, &r, SoundLayer { style: SoundStyle::Spectrum, amount: 1.0 }, Source::Look, &mut Vec::new());
        assert!(f[2].1.r > 0.99, "leftmost light follows the bass: {f:?}");
        assert!(f[0].1.r < 0.15, "rightmost follows the highs: {f:?}");
    }

    #[test]
    fn punch_lifts_without_clipping_hue() {
        let lay = Layout::new(&chans(2));
        let r = Reactor { env: [1.0, 0.0, 0.0], ..Default::default() };
        let mut f = vec![(0u8, Rgb::new(0.2, 0.1, 0.0)), (1u8, Rgb::new(0.9, 0.45, 0.0))];
        apply(&mut f, &lay, &r, SoundLayer { style: SoundStyle::Spectrum, amount: 1.0 }, Source::Screen, &mut Vec::new());
        assert!(f[0].1.r > 0.4 && (f[0].1.g / f[0].1.r - 0.5).abs() < 1e-4);
        assert!(f[1].1.r <= 1.0 + 1e-6 && (f[1].1.g / f[1].1.r - 0.5).abs() < 1e-4);
    }

    #[test]
    fn beat_style_rotates_colours_along_the_room() {
        let c = chans(3);
        let lay = Layout::new(&c);
        let (red, green, blue) = (Rgb::new(1.0, 0.0, 0.0), Rgb::new(0.0, 1.0, 0.0), Rgb::new(0.0, 0.0, 1.0));
        let mut f = vec![(0u8, red), (1u8, green), (2u8, blue)];
        let r = Reactor { beats: 1, flash: 1.0, ..Default::default() };
        apply(&mut f, &lay, &r, SoundLayer { style: SoundStyle::Beat, amount: 1.0 }, Source::Look, &mut Vec::new());
        assert_eq!(f[2].1, green, "the leftmost light takes its right neighbour's colour");
        assert_eq!(f[0].1, blue, "and the rightmost wraps round to the leftmost's");
    }

    #[test]
    fn nothing_changes_at_zero() {
        let lay = Layout::new(&chans(2));
        let r = Reactor { env: [1.0; 3], flash: 1.0, beats: 1, ..Default::default() };
        let before = vec![(0u8, Rgb::new(0.3, 0.2, 0.1)), (1u8, Rgb::new(0.1, 0.2, 0.3))];
        let mut f = before.clone();
        apply(&mut f, &lay, &r, SoundLayer { style: SoundStyle::Beat, amount: 0.0 }, Source::Look, &mut Vec::new());
        assert_eq!(f, before);
    }
}
