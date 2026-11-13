//! Background screen capture. Runs only while something wants it (Screen page open or screen sync streaming).

use lumen_engine::ChannelPos;
use lumen_hue::Rgb;
use lumen_screen::capture::{monitors, Capturer, Grab};
use lumen_screen::{Frame, MonitorInfo, NRect, Sampler, ScreenSettings};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct ScreenShared {
    pub wanted: bool,
    pub monitor: Option<MonitorInfo>,
    pub settings: ScreenSettings,
    pub follow_window: Option<isize>,
    pub chans: Vec<ChannelPos>,
    pub cod_wanted: bool,
    pub cod: Option<crate::games_fx::SharedCod>,
    pub colors: Vec<(u8, Rgb)>,
    /// Latest preview frame, shared (not copied) with the UI; `thumb_seq` bumps when it changes.
    pub thumb: Option<Arc<Frame>>,
    pub thumb_seq: u64,
    /// region actually read (after window-follow + aspect), normalised to the monitor
    pub region: NRect,
    /// content after black-bar removal
    pub content: NRect,
    pub status: String,
    pub fps: f32,
}

pub type SharedScreen = Arc<Mutex<ScreenShared>>;

pub fn spawn() -> SharedScreen {
    let shared: SharedScreen = Arc::new(Mutex::new(ScreenShared { region: NRect::FULL, content: NRect::FULL, ..Default::default() }));
    let sh = shared.clone();
    std::thread::Builder::new()
        .name("screen".into())
        .spawn(move || run(sh))
        .expect("spawn screen thread");
    shared
}

fn run(sh: SharedScreen) {
    let mut cap: Option<Capturer> = None;
    let mut sampler = Sampler::default();
    let mut last: Option<Arc<Frame>> = None;
    let mut last_tick = Instant::now();
    let mut last_thumb = Instant::now();
    let mut frames = 0u32;
    let mut fps_t = Instant::now();
    let mut cap_width = 0u32;
    loop {
        let (sync_wanted, cod, mon, mut settings, follow, chans) = {
            let s = sh.lock().unwrap();
            let cod = if s.cod_wanted { s.cod.clone() } else { None };
            (s.wanted, cod, s.monitor.clone(), s.settings.clone(), s.follow_window, s.chans.clone())
        };
        if !sync_wanted && cod.is_none() {
            cap = None;
            last = None;
            std::thread::sleep(Duration::from_millis(120));
            continue;
        }
        let mon = match mon.or_else(|| monitors().ok().and_then(lumen_screen::primary)) {
            Some(m) => m,
            None => {
                sh.lock().unwrap().status = "No monitor found".into();
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
        };
        let prof = crate::perf::profile();
        let tick_start = Instant::now();
        // pace to the preset's sample rate (grab() already waits for a new frame, ≤16 ms)
        let pace = || {
            let budget = if prof.capture_fps == 0 { Duration::from_millis(4) } else { Duration::from_secs_f32(1.0 / prof.capture_fps as f32) };
            std::thread::sleep(budget.saturating_sub(tick_start.elapsed()).max(Duration::from_millis(2)));
        };
        if cap.as_ref().map(|c| c.monitor != mon).unwrap_or(true) || cap_width != prof.capture_width {
            cap = None; // release the old duplication before creating a new one
            match Capturer::new(&mon, prof.capture_width) {
                Ok(c) => {
                    cap = Some(c);
                    cap_width = prof.capture_width;
                    sampler = Sampler::default();
                    let mut s = sh.lock().unwrap();
                    s.status = format!("Capturing {}", mon.label());
                    s.monitor = Some(mon.clone());
                }
                Err(e) => {
                    sh.lock().unwrap().status = format!("Can't capture: {e:#}");
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
            }
        }
        match cap.as_mut().unwrap().grab(16) {
            Ok(Grab::Frame(f)) => {
                if let Some(cod) = &cod {
                    let feats = lumen_cod::analyze(&f);
                    cod.lock().unwrap().update_screen(&feats, Instant::now());
                }
                last = sync_wanted.then(|| Arc::new(f));
                frames += 1;
            }
            Ok(Grab::Unchanged) => {}
            Err(e) => {
                sh.lock().unwrap().status = format!("{e:#}. Reconnecting…");
                cap = None;
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
        }
        // CoD-only: frame analysis needs no light colours or preview
        if !sync_wanted {
            pace();
            continue;
        }
        let Some(frame) = &last else { continue };
        let now = Instant::now();
        let dt = (now - last_tick).as_secs_f32().min(0.2);
        last_tick = now;

        if let Some(hwnd) = follow {
            if let Some((l, t, r, b)) = lumen_screen::windows_list::window_rect(hwnd) {
                let (mw, mh) = (mon.width as f32, mon.height as f32);
                settings.region = NRect {
                    x: (l - mon.left) as f32 / mw,
                    y: (t - mon.top) as f32 / mh,
                    w: (r - l) as f32 / mw,
                    h: (b - t) as f32 / mh,
                }
                .clamp();
            }
        }
        let mut region = settings.region.clamp();
        if let Some(a) = settings.aspect {
            region = region.fit_aspect(a, frame.width as f32, frame.height as f32);
        }
        let colors = sampler.process(frame, &settings, &chans, dt);

        let mut s = sh.lock().unwrap();
        s.colors = colors;
        s.region = region;
        s.content = sampler.content;
        // the preview shares the frame; a new one only when the picture changed
        let new_picture = !s.thumb.as_ref().is_some_and(|t| Arc::ptr_eq(t, frame));
        if new_picture && now - last_thumb > prof.thumb_every {
            s.thumb = Some(frame.clone());
            s.thumb_seq += 1;
            last_thumb = now;
        }
        if fps_t.elapsed() > Duration::from_secs(1) {
            s.fps = frames as f32 / fps_t.elapsed().as_secs_f32();
            frames = 0;
            fps_t = Instant::now();
        }
        drop(s);
        pace();
    }
}
