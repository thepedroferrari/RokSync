//! The streaming loop: render the active source + overlays and push to the bridge at the
//! performance preset's stream rate.

use crate::screen_service::SharedScreen;
use crate::games_fx::FxSources;
use lumen_engine::{apply_overlays, render, ChannelPos, Program};
use lumen_hue::{Bridge, EntConfig, Rgb, Streamer};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};


#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Source {
    #[default]
    Look,
    Screen,
}

#[derive(Default)]
pub struct SyncShared {
    /// Shared, so the stream thread takes it without copying the look every tick.
    pub program: Arc<Program>,
    pub source: Source,
    pub cs2_enabled: bool,
    pub dota_enabled: bool,
    pub cod_enabled: bool,
    /// Game-effect strength from the active mode, 0..1.
    pub fx_gain: f32,
    /// Sound layer on top of the source; None = off.
    pub sound: Option<crate::sound::SoundLayer>,
    pub running: bool,
    pub connecting: bool,
    pub status: String,
    pub last_frame: Vec<(u8, Rgb)>,
}

pub type SharedSync = Arc<Mutex<SyncShared>>;

pub struct SyncHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

pub fn channel_positions(area: &EntConfig) -> Vec<ChannelPos> {
    area.channels
        .iter()
        .map(|c| ChannelPos { id: c.channel_id, x: c.position.x as f32, y: c.position.y as f32, z: c.position.z as f32 })
        .collect()
}

/// One frame of the active source (no overlays).
pub fn base_frame(program: &Program, source: Source, chans: &[ChannelPos], t: f32, screen: &SharedScreen) -> Vec<(u8, Rgb)> {
    match source {
        Source::Look => render(program, chans, t),
        Source::Screen => {
            let s = screen.lock().unwrap();
            chans
                .iter()
                .map(|c| {
                    let col = s.colors.iter().find(|(id, _)| *id == c.id).map(|(_, c)| *c).unwrap_or(Rgb::BLACK);
                    (c.id, col.scale(program.brightness))
                })
                .collect()
        }
    }
}

impl SyncHandle {
    /// `program` and `source` are installed before the thread starts, so the first frame never
    /// renders whatever an earlier stream left in `shared`.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        bridge: Bridge,
        client_key: String,
        area: EntConfig,
        shared: SharedSync,
        fx: FxSources,
        screen: SharedScreen,
        program: Arc<Program>,
        source: Source,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        {
            let mut sh = shared.lock().unwrap();
            sh.program = program;
            sh.source = source;
            sh.connecting = true;
            sh.status = format!("Connecting to \"{}\"…", area.metadata.name);
        }
        let thread = std::thread::Builder::new()
            .name("hue-stream".into())
            .spawn(move || {
                let mut streamer = match Streamer::start(&bridge, &client_key, &area.id) {
                    Ok(s) => s,
                    Err(e) => {
                        let mut sh = shared.lock().unwrap();
                        sh.running = false;
                        sh.connecting = false;
                        sh.status = format!("Couldn't start: {e:#}");
                        return;
                    }
                };
                {
                    let mut sh = shared.lock().unwrap();
                    sh.running = true;
                    sh.connecting = false;
                    sh.status = format!("Live on \"{}\" · {} channels", area.metadata.name, area.channels.len());
                }
                let chans = channel_positions(&area);
                let lay = crate::sound::Layout::new(&chans);
                let mut reactor = crate::sound::Reactor::default();
                let mut scratch = Vec::with_capacity(chans.len());
                let t0 = Instant::now();
                let mut next = Instant::now();
                let mut last = t0;
                while !stop2.load(Ordering::Relaxed) {
                    let now = Instant::now();
                    let (program, source, cs2_on, dota_on, cod_on, fx_gain, sound) = {
                        let sh = shared.lock().unwrap();
                        (sh.program.clone(), sh.source, sh.cs2_enabled, sh.dota_enabled, sh.cod_enabled, sh.fx_gain, sh.sound)
                    };
                    if source == Source::Screen {
                        let mut s = screen.lock().unwrap();
                        s.wanted = true;
                        if s.chans.len() != chans.len() {
                            s.chans = chans.clone();
                        }
                    }
                    let mut frame = base_frame(&program, source, &chans, (now - t0).as_secs_f32(), &screen);
                    if let Some(layer) = sound {
                        fx.sound.want(1.0);
                        reactor.update(&fx.sound.levels(), now, (now - last).as_secs_f32());
                        crate::sound::apply(&mut frame, &lay, &reactor, layer, source, &mut scratch);
                    }
                    last = now;
                    apply_game_fx(&mut frame, &fx, now, fx_gain, cs2_on, dota_on, cod_on, program.brightness);
                    if let Err(e) = streamer.send(&frame) {
                        shared.lock().unwrap().status = format!("Stream lost: {e:#}");
                        break;
                    }
                    std::mem::swap(&mut shared.lock().unwrap().last_frame, &mut frame);
                    next += Duration::from_secs_f64(1.0 / crate::perf::profile().stream_hz.max(1) as f64);
                    let now = Instant::now();
                    if next > now {
                        std::thread::sleep(next - now);
                    } else {
                        next = now;
                    }
                }
                drop(streamer); // stops the entertainment area
                let mut sh = shared.lock().unwrap();
                sh.running = false;
                if !sh.status.starts_with("Stream lost") {
                    sh.status = "Stopped".into();
                }
            })
            .expect("spawn stream thread");
        Self { stop, thread: Some(thread) }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for SyncHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Layer the enabled games' effects on a frame. Dota can also supply the base between events
/// (day/night or faction colour) unless it is set to follow screen sync.
#[allow(clippy::too_many_arguments)]
pub fn apply_game_fx(frame: &mut [(u8, Rgb)], fx: &FxSources, now: Instant, gain: f32, cs2: bool, dota: bool, cod: bool, brightness: f32) {
    // A TEST press plays on the lights even while that game's effects are switched off.
    let testing = |t: Option<Instant>, dur: f32| t.is_some_and(|t0| now.saturating_duration_since(t0).as_secs_f32() < dur);
    let mut ovs = Vec::new();
    {
        let s = fx.dota.lock().unwrap();
        if dota {
            if let Some(base) = s.base_color(now) {
                let base = base.scale(brightness.max(0.05));
                for (_, c) in frame.iter_mut() {
                    *c = base;
                }
            }
        }
        if dota || s.test.is_some_and(|(f, t0)| testing(Some(t0), f.duration())) {
            ovs.extend(s.overlays(now));
        }
    }
    {
        let s = fx.cs2.lock().unwrap();
        if cs2 || s.test.is_some_and(|(f, t0)| testing(Some(t0), f.duration())) {
            ovs.extend(s.overlays(now));
        }
    }
    {
        let s = fx.cod.lock().unwrap();
        if cod || s.test.is_some_and(|(f, t0)| testing(Some(t0), f.duration())) {
            ovs.extend(s.overlays(now));
        }
    }
    if !ovs.is_empty() {
        lumen_engine::scale_overlays(&mut ovs, gain);
        apply_overlays(frame, &ovs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_engine::{Look, Motion};

    #[test]
    fn a_stream_starts_with_the_callers_program_and_source() {
        let shared: SharedSync = Arc::new(Mutex::new(SyncShared { source: Source::Screen, ..Default::default() }));
        let fx = FxSources { cs2: Default::default(), dota: Default::default(), cod: Default::default(), sound: crate::sound::SoundCtl::idle() };
        let bridge = Bridge::new("127.0.0.1:9", "k").unwrap();
        let program = Program {
            look: Look::Palette(vec![Rgb::from_hsv(0.0, 1.0, 1.0), Rgb::from_hsv(0.6, 1.0, 1.0)]),
            motion: Motion::Drift { period_s: 8.0 },
            brightness: 1.0,
        };
        let h = SyncHandle::start(bridge, String::new(), EntConfig::default(), shared.clone(), fx, Default::default(), Arc::new(program.clone()), Source::Look);
        {
            let sh = shared.lock().unwrap();
            assert_eq!(sh.source, Source::Look);
            assert_eq!(*sh.program, program);
        }
        drop(h);
    }
}
