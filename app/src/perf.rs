//! Performance presets. One global setting read by the UI, the screen-capture thread, the
//! streaming thread and the AI model, so changing it takes effect immediately.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Perf {
    Low,
    #[default]
    Medium,
    High,
    Extreme,
}

pub struct Profile {
    /// Packets per second to the bridge. Hue relays ~25 updates/s to the lights; more than that
    /// mostly buys resilience against lost UDP packets.
    pub stream_hz: u32,
    /// Width the screen is downscaled to on the GPU before reading it back.
    pub capture_width: u32,
    /// Screen samples per second (0 = as fast as the screen changes).
    pub capture_fps: u32,
    /// UI redraw interval while RokSync is focused / while another app (a game) is.
    pub ui_focused: Duration,
    pub ui_background: Duration,
    pub ui_animations: bool,
    pub thumb_every: Duration,
    pub model_gpu: bool,
    pub model_threads: Option<u32>,
    /// Unload the model after this long unused. None = keep it while RokSync is open.
    pub model_idle: Option<Duration>,
    /// Load the model as soon as RokSync opens (otherwise: when the Mood page opens).
    pub model_preload: bool,
}

static CURRENT: AtomicU8 = AtomicU8::new(1);

impl Perf {
    pub const ALL: [Perf; 4] = [Perf::Low, Perf::Medium, Perf::High, Perf::Extreme];

    pub fn label(self) -> &'static str {
        match self {
            Perf::Low => "Eco",
            Perf::Medium => "Balanced",
            Perf::High => "High",
            Perf::Extreme => "Extreme",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Perf::Low => "ECO",
            Perf::Medium => "BALANCED",
            Perf::High => "HIGH",
            Perf::Extreme => "EXTREME",
        }
    }

    pub fn tagline(self) -> &'static str {
        match self {
            Perf::Low => "Laptops and low-power PCs",
            Perf::Medium => "Films and most games",
            Perf::High => "Fast games, quicker reaction",
            Perf::Extreme => "Every frame, lowest delay",
        }
    }

    pub fn profile(self) -> Profile {
        let ms = Duration::from_millis;
        match self {
            Perf::Low => Profile {
                stream_hz: 25,
                capture_width: 160,
                capture_fps: 20,
                ui_focused: ms(250),
                ui_background: ms(3000),
                ui_animations: false,
                thumb_every: ms(500),
                model_gpu: false,
                model_threads: Some(2),
                model_idle: Some(Duration::from_secs(30)),
                model_preload: false,
            },
            Perf::Medium => Profile {
                stream_hz: 50,
                capture_width: 240,
                capture_fps: 30,
                ui_focused: ms(100),
                ui_background: ms(1000),
                ui_animations: true,
                thumb_every: ms(200),
                model_gpu: true,
                model_threads: None,
                model_idle: Some(Duration::from_secs(120)),
                model_preload: false,
            },
            Perf::High => Profile {
                stream_hz: 50,
                capture_width: 320,
                capture_fps: 60,
                ui_focused: ms(50),
                ui_background: ms(1000),
                ui_animations: true,
                thumb_every: ms(120),
                model_gpu: true,
                model_threads: None,
                model_idle: Some(Duration::from_secs(300)),
                model_preload: false,
            },
            Perf::Extreme => Profile {
                stream_hz: 60,
                capture_width: 480,
                capture_fps: 0,
                ui_focused: ms(16),
                ui_background: ms(500),
                ui_animations: true,
                thumb_every: ms(60),
                model_gpu: true,
                model_threads: None,
                model_idle: None,
                model_preload: true,
            },
        }
    }
}

pub fn set(p: Perf) {
    CURRENT.store(p as u8, Ordering::Relaxed);
}

pub fn get() -> Perf {
    Perf::ALL[CURRENT.load(Ordering::Relaxed) as usize % 4]
}

pub fn profile() -> Profile {
    get().profile()
}
