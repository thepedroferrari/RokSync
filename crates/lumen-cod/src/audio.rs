//! The PC's audio output via WASAPI loopback. cpal opens the default *output* device as an input
//! stream, which on WASAPI sets `AUDCLNT_STREAMFLAGS_LOOPBACK`. The stream lives on its own thread
//! (cpal streams are not `Send`) and is re-opened when the default output device changes.
//!
//! DSP per ~10 ms block, all one-pole filters:
//! * low band ≈20–150 Hz (20 Hz DC-blocking high-pass, then two 150 Hz low-passes): explosions;
//! * high band > ≈1.5 kHz (input minus a 1.5 kHz low-pass): gunfire transients;
//! * `boom` / `onset`: dB the low / high band rises above its 1.5 s / 300 ms average;
//! * `onsets_per_s`: rising onsets ≥ 8 dB (60 ms refractory), counted over the last 3 s.
//!
//! `boom` and `onset` are peak-held for 150 ms so a 10–60 Hz poller never misses a 10 ms spike.
//! WASAPI delivers nothing while the PC is silent, so `updated` goes stale ([`AudioLevels::fresh`]).

use anyhow::{anyhow, Context, Result};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Latest audio measurements, shared between the audio thread and the app.
#[derive(Debug, Clone, Default)]
pub struct AudioLevels {
    /// When the last block was analyzed (None = never).
    pub updated: Option<Instant>,
    pub running: bool,
    pub device: String,
    pub status: String,
    /// Full-band RMS of the latest block, 0..1 (1 = full scale).
    pub rms: f32,
    /// RMS below ≈150 Hz.
    pub low: f32,
    /// RMS ≈150 Hz–1.5 kHz.
    pub mid: f32,
    /// RMS above ≈1.5 kHz.
    pub high: f32,
    /// Low-band rise over its 1.5 s average, in dB (≥ 0), peak-held 150 ms.
    pub boom: f32,
    /// High-band rise over its 300 ms average, in dB (≥ 0), peak-held 150 ms.
    pub onset: f32,
    /// Sharp onsets per second over the last 3 s.
    pub onsets_per_s: f32,
    pub blocks: u64,
}

impl AudioLevels {
    /// The values describe the last ~0.3 s. (False while the PC is silent: WASAPI sends nothing.)
    pub fn fresh(&self, now: Instant) -> bool {
        self.updated
            .is_some_and(|t| now.saturating_duration_since(t) < Duration::from_millis(300))
    }

    /// Every measurement, without the device name and status text: a copy that allocates
    /// nothing, for loops that poll the levels many times a second.
    pub fn numbers(&self) -> Self {
        let Self { updated, running, device: _, status: _, rms, low, mid, high, boom, onset, onsets_per_s, blocks } = *self;
        Self { updated, running, device: String::new(), status: String::new(), rms, low, mid, high, boom, onset, onsets_per_s, blocks }
    }
}

pub type SharedAudio = Arc<Mutex<AudioLevels>>;

/// Keeps the loopback stream alive; drop it to stop listening.
pub struct AudioHandle {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for AudioHandle {
    fn drop(&mut self) {
        drop(self.stop.take()); // disconnects the channel: the thread wakes and exits
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Starts listening to the default output device. Errors (no device, unsupported format, …) are
/// returned and also written to `shared.status`; this never panics.
pub fn spawn_loopback(shared: SharedAudio) -> Result<AudioHandle> {
    spawn_loopback_on(shared, None)
}

/// Starts listening to the output device named `device`, or to whatever Windows plays to when
/// None (re-opened when the default changes). A named device that goes away is retried every 2 s.
pub fn spawn_loopback_on(shared: SharedAudio, device: Option<String>) -> Result<AudioHandle> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<()>>(1);
    let thread = std::thread::Builder::new()
        .name("loopback".into())
        .spawn(move || run(shared, device, stop_rx, ready_tx))
        .context("starting the audio thread")?;
    let first = ready_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_else(|_| Err(anyhow!("audio device did not respond")));
    let handle = AudioHandle {
        stop: Some(stop_tx),
        thread: Some(thread),
    };
    first.map(|()| handle) // on error the handle drops here and joins the thread
}

fn set_status(shared: &SharedAudio, running: bool, device: &str, status: &str) {
    if let Ok(mut l) = shared.lock() {
        l.running = running;
        l.device = device.to_string();
        l.status = status.to_string();
    }
}

/// Names of the PC's audio outputs, default first.
pub fn output_devices() -> Vec<String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let mut out: Vec<String> = host.output_devices().map(|ds| ds.filter_map(|d| d.name().ok()).collect()).unwrap_or_default();
    if let Some(d) = default_output() {
        if let Some(i) = out.iter().position(|n| *n == d) {
            out.swap(0, i);
        }
    }
    out
}

/// The output Windows plays to right now.
pub fn default_output() -> Option<String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    cpal::default_host().default_output_device().and_then(|d| d.name().ok())
}

/// The audio thread: owns the stream, re-opens it after errors, a default-device change (when
/// following the default) or the named device coming back.
fn run(shared: SharedAudio, want: Option<String>, stop: mpsc::Receiver<()>, ready: mpsc::SyncSender<Result<()>>) {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let mut ready = Some(ready);
    loop {
        let failed = Arc::new(AtomicBool::new(false));
        let opened = open(&host, want.as_deref(), &shared, failed.clone());
        let device = match &opened {
            Ok((_, name)) => {
                set_status(&shared, true, name, "listening");
                name.clone()
            }
            Err(e) => {
                set_status(&shared, false, "", &format!("{e:#}"));
                String::new()
            }
        };
        if let Some(r) = ready.take() {
            let failed_first = opened.as_ref().err().map(|e| anyhow!("{e:#}"));
            let _ = r.send(failed_first.map_or(Ok(()), Err));
            if opened.is_err() {
                return;
            }
        }
        // hold the stream until told to stop, the stream errors, or the device to follow changes
        let is_open = opened.is_ok();
        let _stream = opened.ok().map(|(s, _)| s);
        loop {
            match stop.recv_timeout(Duration::from_secs(2)) {
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                _ => {
                    set_status(&shared, false, &device, "stopped");
                    return;
                }
            }
            let moved = want.is_none()
                && host.default_output_device().and_then(|d| d.name().ok()).unwrap_or_default() != device;
            if !is_open || failed.load(Ordering::Relaxed) || moved {
                break; // drop the stream and open again
            }
        }
    }
}

/// Opens a loopback input stream on the named output device, or the default one.
fn open(
    host: &cpal::Host,
    want: Option<&str>,
    shared: &SharedAudio,
    failed: Arc<AtomicBool>,
) -> Result<(cpal::Stream, String)> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let device = match want {
        None => host.default_output_device().ok_or_else(|| anyhow!("no audio output device"))?,
        Some(name) => host
            .output_devices()
            .context("listing audio outputs")?
            .find(|d| d.name().is_ok_and(|n| n == name))
            .ok_or_else(|| anyhow!("{name} is not connected"))?,
    };
    let name = device.name().unwrap_or_else(|_| "output device".into());
    let cfg = device
        .default_output_config()
        .context("reading the output format")?;
    let channels = cfg.channels().max(1) as usize;
    let rate = cfg.sample_rate().0 as f32;
    let stream_cfg: cpal::StreamConfig = cfg.config();
    let on_err = {
        let shared = shared.clone();
        move |e: cpal::StreamError| {
            failed.store(true, Ordering::Relaxed);
            if let Ok(mut l) = shared.lock() {
                l.running = false;
                l.status = format!("audio stream error: {e}");
            }
        }
    };
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => {
            build::<f32>(&device, &stream_cfg, channels, rate, shared.clone(), on_err)?
        }
        cpal::SampleFormat::I16 => {
            build::<i16>(&device, &stream_cfg, channels, rate, shared.clone(), on_err)?
        }
        cpal::SampleFormat::I32 => {
            build::<i32>(&device, &stream_cfg, channels, rate, shared.clone(), on_err)?
        }
        cpal::SampleFormat::U16 => {
            build::<u16>(&device, &stream_cfg, channels, rate, shared.clone(), on_err)?
        }
        f => return Err(anyhow!("unsupported audio sample format {f:?}")),
    };
    stream.play().context("starting audio loopback")?;
    Ok((stream, name))
}

fn build<T>(
    device: &cpal::Device,
    cfg: &cpal::StreamConfig,
    channels: usize,
    rate: f32,
    shared: SharedAudio,
    on_err: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    use cpal::traits::DeviceTrait;
    use cpal::Sample;
    let mut an = Analyzer::new(rate);
    let mut mono = Vec::new();
    let data = move |data: &[T], _: &cpal::InputCallbackInfo| {
        mono.clear();
        mono.extend(
            data.chunks_exact(channels)
                .map(|fr| fr.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / channels as f32),
        );
        if an.process(&mono) {
            // never block the audio thread on the GUI: if the lock is busy, the next callback writes
            if let Ok(mut l) = shared.try_lock() {
                an.write(&mut l, Instant::now());
            }
        }
    };
    device
        .build_input_stream(cfg, data, on_err, None)
        .context("couldn't listen to PC sound")
}

/// One-pole low-pass coefficient for cutoff `hz` at sample rate `sr`.
fn one_pole(hz: f32, sr: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * hz / sr).exp()
}

/// EMA coefficient for time constant `tau_s` updated every `dt_s`.
fn ema(tau_s: f32, dt_s: f32) -> f32 {
    1.0 - (-dt_s / tau_s).exp()
}

fn db(a: f32, b: f32) -> f32 {
    20.0 * (a.max(1e-9) / b.max(1e-9)).log10()
}

/// Onset needed to count toward firefight density, dB above the 300 ms high-band average.
const ONSET_DB: f32 = 8.0;
/// Below these absolute levels the band is treated as silent (≈ −40 / −48 dBFS).
const LOW_GATE: f32 = 0.01;
const HIGH_GATE: f32 = 0.004;
const HOLD_S: f32 = 0.15;
const DENSITY_WINDOW_S: f32 = 3.0;

/// Block analyzer: pure DSP, no I/O.
#[derive(Debug, Clone)]
pub struct Analyzer {
    block_len: usize,
    dt: f32,
    a_dc: f32,
    a_low: f32,
    a_high: f32,
    a_low_avg: f32,
    a_high_avg: f32,
    dc: f32,
    lp1: f32,
    lp2: f32,
    lp_hi: f32,
    n: usize,
    sq: [f32; 4],
    rms: [f32; 4],
    low_avg: Option<f32>,
    high_avg: Option<f32>,
    prev_onset: f32,
    boom_hold: (f32, f32),
    onset_hold: (f32, f32),
    t: f32,
    last_onset: f32,
    onsets: VecDeque<f32>,
    blocks: u64,
}

impl Analyzer {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let block_len = (sr / 100.0).round() as usize;
        let dt = block_len as f32 / sr;
        Self {
            block_len,
            dt,
            a_dc: one_pole(20.0, sr),
            a_low: one_pole(150.0, sr),
            a_high: one_pole(1500.0, sr),
            a_low_avg: ema(1.5, dt),
            a_high_avg: ema(0.3, dt),
            dc: 0.0,
            lp1: 0.0,
            lp2: 0.0,
            lp_hi: 0.0,
            n: 0,
            sq: [0.0; 4],
            rms: [0.0; 4],
            low_avg: None,
            high_avg: None,
            prev_onset: 0.0,
            boom_hold: (0.0, 0.0),
            onset_hold: (0.0, 0.0),
            t: 0.0,
            last_onset: f32::NEG_INFINITY,
            onsets: VecDeque::new(),
            blocks: 0,
        }
    }

    /// Feeds mono samples (−1..1). Returns true if at least one block completed.
    pub fn process(&mut self, mono: &[f32]) -> bool {
        let mut done = false;
        for &x in mono {
            let x = if x.is_finite() { x } else { 0.0 };
            self.dc += self.a_dc * (x - self.dc);
            let x0 = x - self.dc;
            self.lp1 += self.a_low * (x0 - self.lp1);
            self.lp2 += self.a_low * (self.lp1 - self.lp2);
            self.lp_hi += self.a_high * (x0 - self.lp_hi);
            let (low, high) = (self.lp2, x0 - self.lp_hi);
            let mid = self.lp_hi - self.lp2;
            self.sq[0] += x0 * x0;
            self.sq[1] += low * low;
            self.sq[2] += mid * mid;
            self.sq[3] += high * high;
            self.n += 1;
            if self.n >= self.block_len {
                self.end_block();
                done = true;
            }
        }
        done
    }

    fn end_block(&mut self) {
        let n = self.n.max(1) as f32;
        self.rms = self.sq.map(|s| (s / n).sqrt());
        self.sq = [0.0; 4];
        self.n = 0;
        self.t += self.dt;
        self.blocks += 1;
        let (low, high) = (self.rms[1], self.rms[3]);

        let low_avg = *self.low_avg.get_or_insert(low);
        let high_avg = *self.high_avg.get_or_insert(high);
        let boom = if low >= LOW_GATE {
            db(low, low_avg.max(LOW_GATE * 0.3)).max(0.0)
        } else {
            0.0
        };
        let onset = if high >= HIGH_GATE {
            db(high, high_avg.max(HIGH_GATE * 0.3)).max(0.0)
        } else {
            0.0
        };
        self.low_avg = Some(low_avg + self.a_low_avg * (low - low_avg));
        self.high_avg = Some(high_avg + self.a_high_avg * (high - high_avg));

        // rising onsets with a refractory period, counted over a sliding window
        if onset >= ONSET_DB && self.prev_onset < ONSET_DB && self.t - self.last_onset >= 0.06 {
            self.last_onset = self.t;
            self.onsets.push_back(self.t);
        }
        self.prev_onset = onset;
        while self
            .onsets
            .front()
            .is_some_and(|&t0| self.t - t0 > DENSITY_WINDOW_S)
        {
            self.onsets.pop_front();
        }

        let hold = |h: &mut (f32, f32), v: f32, t: f32| {
            if v >= h.0 || t - h.1 > HOLD_S {
                *h = (v, t);
            }
        };
        hold(&mut self.boom_hold, boom, self.t);
        hold(&mut self.onset_hold, onset, self.t);
    }

    pub fn onsets_per_s(&self) -> f32 {
        self.onsets.len() as f32 / DENSITY_WINDOW_S
    }

    pub fn write(&self, l: &mut AudioLevels, now: Instant) {
        l.updated = Some(now);
        [l.rms, l.low, l.mid, l.high] = self.rms;
        l.boom = self.boom_hold.0;
        l.onset = self.onset_hold.0;
        l.onsets_per_s = self.onsets_per_s();
        l.blocks = self.blocks;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const SR: f32 = 48_000.0;

    fn tone(hz: f32, amp: f32, secs: f32) -> Vec<f32> {
        (0..(SR * secs) as usize)
            .map(|i| amp * (TAU * hz * i as f32 / SR).sin())
            .collect()
    }

    fn noise(amp: f32, n: usize, seed: &mut u32) -> Vec<f32> {
        (0..n)
            .map(|_| {
                *seed ^= *seed << 13;
                *seed ^= *seed >> 17;
                *seed ^= *seed << 5;
                amp * ((*seed as f32 / u32::MAX as f32) * 2.0 - 1.0)
            })
            .collect()
    }

    #[test]
    fn a_low_thump_after_quiet_is_a_boom() {
        let mut a = Analyzer::new(SR);
        let mut lv = AudioLevels::default();
        a.process(&tone(60.0, 0.02, 2.0));
        a.write(&mut lv, Instant::now());
        assert!(lv.boom < 3.0, "{lv:?}");
        a.process(&tone(60.0, 0.6, 0.08));
        a.write(&mut lv, Instant::now());
        assert!(lv.boom > 15.0, "{lv:?}");
        assert!(lv.low > lv.high * 5.0, "{lv:?}");
    }

    #[test]
    fn a_high_tone_is_not_a_boom() {
        let mut a = Analyzer::new(SR);
        let mut lv = AudioLevels::default();
        a.process(&tone(3000.0, 0.01, 1.0));
        a.process(&tone(3000.0, 0.6, 0.1));
        a.write(&mut lv, Instant::now());
        assert!(lv.boom < 3.0, "{lv:?}");
        assert!(lv.onset > 10.0, "{lv:?}");
    }

    #[test]
    fn rapid_clicks_build_firefight_density() {
        let mut a = Analyzer::new(SR);
        let mut seed = 1234u32;
        let mut lv = AudioLevels::default();
        a.process(&vec![0.0; SR as usize]);
        // 3 s of bursts: 8 shots/s, each a 15 ms noise click over silence
        for _ in 0..24 {
            a.process(&noise(0.5, (SR * 0.015) as usize, &mut seed));
            a.process(&vec![0.0; (SR * 0.11) as usize]);
        }
        a.write(&mut lv, Instant::now());
        assert!(lv.onsets_per_s > 5.0, "{lv:?}");
        a.process(&noise(0.001, (SR * 3.5) as usize, &mut seed));
        a.write(&mut lv, Instant::now());
        assert!(lv.onsets_per_s < 0.5, "{lv:?}");
    }

    #[test]
    fn garbage_samples_are_ignored() {
        let mut a = Analyzer::new(0.0); // nonsense rate is clamped
        a.process(&[f32::NAN, f32::INFINITY, 0.5, -0.5]);
        let mut lv = AudioLevels::default();
        a.process(&vec![0.1; 1000]);
        a.write(&mut lv, Instant::now());
        assert!(lv.rms.is_finite() && lv.boom.is_finite());
    }

    #[test]
    fn stale_levels_are_not_fresh() {
        let now = Instant::now();
        let lv = AudioLevels {
            updated: Some(now),
            ..Default::default()
        };
        assert!(lv.fresh(now));
        assert!(!lv.fresh(now + Duration::from_secs(1)));
        assert!(!AudioLevels::default().fresh(now));
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;

    /// Opens the real loopback stream. `cargo test -p lumen-cod -- --ignored` on a PC with audio.
    #[test]
    #[ignore]
    fn opens_real_loopback() {
        let shared = SharedAudio::default();
        match spawn_loopback(shared.clone()) {
            Ok(h) => {
                std::thread::sleep(Duration::from_millis(500));
                let l = shared.lock().map(|l| l.clone()).unwrap_or_default();
                eprintln!(
                    "{} · {} · {} blocks · rms {:.4}",
                    l.device, l.status, l.blocks, l.rms
                );
                assert!(l.running);
                drop(h);
                assert!(shared.lock().is_ok_and(|l| !l.running), "drop stops it");
            }
            Err(e) => eprintln!("no loopback here: {e:#}"),
        }
    }
}
