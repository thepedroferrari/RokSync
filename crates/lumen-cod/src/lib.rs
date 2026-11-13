//! Call of Duty light effects, estimated from desktop frames (DXGI Desktop Duplication,
//! [`screen`]), the PC's audio output (WASAPI loopback, [`audio`]) and the foreground window
//! title ([`foreground_title`]). Nothing reads or hooks the game process.

pub mod audio;
pub mod calibrate;
pub mod screen;

pub use audio::{default_output, output_devices, spawn_loopback, spawn_loopback_on, AudioHandle, AudioLevels, SharedAudio};
pub use calibrate::{Calibration, HudProfile};
pub use screen::{analyze, zones, ScreenFeatures, ZoneRect};

use lumen_engine::Overlay;
use lumen_hue::Rgb;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

pub const WHY_ESTIMATED: &str = "Call of Duty shares no game data and its anti-cheat is strict, so RokSync only watches the screen and listens to the sound, like you do. Effects can misfire. They start at low sensitivity, so raise it if they miss.";
pub const NEVERS: [&str; 3] = [
    "Never reads game memory",
    "Never injects or hooks the game",
    "Never draws over the game",
];
pub const GUARD_FOCUS: (&str, &str) = (
    "Only while CoD is in front",
    "Pauses when you alt-tab, so your desktop never triggers effects",
);
pub const GUARD_CALM: (&str, &str) = (
    "Reduce flashing",
    "Flashbangs and explosions become soft fades",
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Screen,
    Sound,
    Hud,
}

impl Source {
    pub fn tag(self) -> &'static str {
        match self {
            Source::Screen => "SCREEN",
            Source::Sound => "SOUND",
            Source::Hud => "HUD",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Screen,
    Sound,
    NeedsCalibration,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Screen, Group::Sound, Group::NeedsCalibration];

    pub fn name(self) -> &'static str {
        match self {
            Group::Screen => "FROM THE SCREEN",
            Group::Sound => "FROM THE SOUND",
            Group::NeedsCalibration => "NEEDS CALIBRATION",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Group::Screen => "what you see",
            Group::Sound => "what you hear",
            Group::NeedsCalibration => "reads your HUD, calibrate once first",
        }
    }

    pub fn effects(self) -> &'static [Fx] {
        match self {
            Group::Screen => &Fx::ALL[0..5],
            Group::Sound => &Fx::ALL[5..7],
            Group::NeedsCalibration => &Fx::ALL[7..10],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fx {
    Flashed,
    Damage,
    LowHp,
    Nvg,
    Gas,
    Explosion,
    Firefight,
    Hit,
    Kill,
    Streak,
}

const N: usize = 10;

impl Fx {
    pub const ALL: [Fx; N] = [
        Fx::Flashed,
        Fx::Damage,
        Fx::LowHp,
        Fx::Nvg,
        Fx::Gas,
        Fx::Explosion,
        Fx::Firefight,
        Fx::Hit,
        Fx::Kill,
        Fx::Streak,
    ];

    /// Stable key for settings files.
    pub fn id(self) -> &'static str {
        match self {
            Fx::Flashed => "flashed",
            Fx::Damage => "damage",
            Fx::LowHp => "lowhp",
            Fx::Nvg => "nvg",
            Fx::Gas => "gas",
            Fx::Explosion => "explosion",
            Fx::Firefight => "firefight",
            Fx::Hit => "hit",
            Fx::Kill => "kill",
            Fx::Streak => "streak",
        }
    }

    pub fn from_id(id: &str) -> Option<Fx> {
        Fx::ALL.iter().copied().find(|f| f.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Fx::Flashed => "Flashed or stunned",
            Fx::Damage => "Taking damage",
            Fx::LowHp => "Low health",
            Fx::Nvg => "Night vision",
            Fx::Gas => "Gas and smoke",
            Fx::Explosion => "Explosions",
            Fx::Firefight => "Firefight",
            Fx::Hit => "Hit marker",
            Fx::Kill => "Kill confirmed",
            Fx::Streak => "Scorestreak ready",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Fx::Flashed => "Room goes white when the screen does",
            Fx::Damage => "Quick red edge when the screen edges flash red",
            Fx::LowHp => "Slow red pulse when red edges last over a second",
            Fx::Nvg => "Room drops to dim green while night vision is on",
            Fx::Gas => "Room greys out when the screen washes out",
            Fx::Explosion => "Orange burst on a heavy low hit in the game audio",
            Fx::Firefight => {
                "Flicker that builds with sustained gunfire, then settles"
            }
            Fx::Hit => "Tiny white tick when a hit marker shows at the crosshair",
            Fx::Kill => "Quick orange tick when the kill pop-up shows below the crosshair",
            Fx::Streak => "Gold glow when a streak icon lights up",
        }
    }

    pub fn color(self) -> Rgb {
        let hex = match self {
            Fx::Flashed => "#FFFFFF",
            Fx::Damage => "#D7263D",
            Fx::LowHp => "#8A1C2B",
            Fx::Nvg => "#00E676",
            Fx::Gas => "#8C8C93",
            Fx::Explosion => "#FF6A00",
            Fx::Firefight => "#FFB000",
            Fx::Hit => "#F3F1EA",
            Fx::Kill => "#FF5A1F",
            Fx::Streak => "#FFD400",
        };
        Rgb::from_hex(hex).unwrap_or(Rgb::WHITE)
    }

    pub fn source(self) -> Source {
        match self {
            Fx::Flashed | Fx::Damage | Fx::LowHp | Fx::Nvg | Fx::Gas => Source::Screen,
            Fx::Explosion | Fx::Firefight => Source::Sound,
            Fx::Hit | Fx::Kill | Fx::Streak => Source::Hud,
        }
    }

    pub fn group(self) -> Group {
        match self.source() {
            Source::Screen => Group::Screen,
            Source::Sound => Group::Sound,
            Source::Hud => Group::NeedsCalibration,
        }
    }

    /// HUD effects stay inactive (confidence 0) until calibration has learned them.
    pub fn needs_calibration(self) -> bool {
        self.source() == Source::Hud
    }

    /// 1..=5, higher fires more easily.
    pub fn default_sensitivity(self) -> u8 {
        match self {
            Fx::Nvg => 3,
            Fx::Streak => 1,
            _ => 2,
        }
    }

    pub fn default_on(self) -> bool {
        !matches!(self, Fx::Gas | Fx::Streak)
    }

    /// How reliable this detector is in general, 1..=3.
    pub fn reliability(self) -> u8 {
        match self {
            Fx::Flashed | Fx::Nvg | Fx::Explosion => 3,
            Fx::Damage | Fx::LowHp | Fx::Firefight => 2,
            Fx::Gas | Fx::Hit | Fx::Kill | Fx::Streak => 1,
        }
    }

    pub fn reliability_label(self) -> (&'static str, Rgb) {
        match self.reliability() {
            3 => ("HIGH CONFIDENCE", Rgb::new(0.486, 1.0, 0.42)),
            2 => ("USUALLY RIGHT", Rgb::new(1.0, 0.69, 0.0)),
            _ => ("HIT OR MISS", Rgb::new(0.79, 0.78, 0.75)),
        }
    }

    /// Length of one TEST play, seconds.
    pub fn duration(self) -> f32 {
        match self {
            Fx::Flashed => 2.4,
            Fx::Damage => 0.6,
            Fx::LowHp => 4.0,
            Fx::Nvg | Fx::Gas => 3.0,
            Fx::Explosion => 1.6,
            Fx::Firefight => 4.0,
            Fx::Hit => 0.3,
            Fx::Kill => 0.55,
            Fx::Streak => 1.8,
        }
    }

    fn i(self) -> usize {
        self as usize
    }
}

/// Fraction of the (base → peak) span a signal must cross, per sensitivity 1..=5.
fn sens_frac(s: u8) -> f32 {
    [0.8, 0.65, 0.5, 0.38, 0.28][(s.clamp(1, 5) - 1) as usize]
}

/// Live confidence: 0 far below the threshold, 0.5 at it, 1 at twice the distance.
fn conf_of(signal: f32, base: f32, thr: f32) -> f32 {
    ((signal - base) / (thr - base).max(1e-4) * 0.5).clamp(0.0, 1.0)
}

fn ema(tau_s: f32, dt: f32) -> f32 {
    1.0 - (-dt / tau_s.max(1e-3)).exp()
}

/// Moves `l` toward `target` by fraction `a`, snapping the last 1.5 % so effects really end.
fn approach(l: &mut f32, target: f32, a: f32) {
    *l += (target - *l) * a.clamp(0.0, 1.0);
    if (*l - target).abs() < 0.015 {
        *l = target;
    }
}

fn secs(a: Instant, b: Instant) -> f32 {
    b.saturating_duration_since(a).as_secs_f32()
}

/// On/off with hold times (hysteresis in time as well as level), so effects don't flicker.
#[derive(Debug, Clone, Copy, Default)]
struct Gate {
    on: bool,
    since: Option<Instant>,
}

impl Gate {
    /// Returns true on the update where the gate switches on.
    fn update(
        &mut self,
        want_on: bool,
        want_off: bool,
        on_hold: f32,
        off_hold: f32,
        now: Instant,
    ) -> bool {
        let (pending, hold) = if self.on {
            (want_off, off_hold)
        } else {
            (want_on, on_hold)
        };
        if !pending {
            self.since = None;
            return false;
        }
        let t0 = *self.since.get_or_insert(now);
        if secs(t0, now) >= hold {
            self.on = !self.on;
            self.since = None;
            return self.on;
        }
        false
    }
}

#[derive(Debug, Clone, Default)]
struct Detect {
    last_screen: Option<Instant>,
    last_audio: Option<Instant>,
    conf: [f32; N],
    level: [f32; N],
    shot: [Option<(Instant, f32)>; N],
    flash_on: bool,
    flash_low_at: Option<Instant>,
    dmg_armed: bool,
    dmg_prev: f32,
    lowhp: Gate,
    nvg: Gate,
    gas: Gate,
    streak: Gate,
    sat_avg: Option<f32>,
    hit_armed: bool,
    kill_armed: bool,
    boom_armed: bool,
}

impl Detect {
    fn fresh() -> Self {
        Self {
            dmg_armed: true,
            hit_armed: true,
            kill_armed: true,
            boom_armed: true,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct CodState {
    /// Live events skip these; TEST still plays them.
    pub fx_off: HashSet<Fx>,
    /// Per-effect strength 0..1 (missing = 1).
    pub fx_level: HashMap<Fx, f32>,
    /// Per-effect sensitivity 1..=5 (missing = [`Fx::default_sensitivity`]).
    pub sensitivity: HashMap<Fx, u8>,
    /// Flashbangs and explosions become soft fades, firefight stops flickering.
    pub calm: bool,
    /// Pause live effects unless CoD is in front (focus comes from `set_foreground`).
    pub only_in_front: bool,
    pub calibration: Calibration,
    /// An effect fired from the UI; plays even with no game running.
    pub test: Option<(Fx, Instant)>,
    pub last_event: String,
    foreground: bool,
    det: Detect,
}

impl Default for CodState {
    fn default() -> Self {
        Self {
            fx_off: Fx::ALL.into_iter().filter(|f| !f.default_on()).collect(),
            fx_level: HashMap::new(),
            sensitivity: HashMap::new(),
            calm: false,
            only_in_front: true,
            calibration: Calibration::default(),
            test: None,
            last_event: String::new(),
            foreground: false,
            det: Detect::fresh(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Live {
    /// Continuous effects: 0..1.
    level: f32,
    /// One-shot effects: (seconds since it fired, strength 0..1).
    shot: Option<(f32, f32)>,
}

/// Paint order: later overlays win, so the flash (you're blind) is last.
const PAINT_ORDER: [Fx; N] = [
    Fx::Gas,
    Fx::Nvg,
    Fx::Firefight,
    Fx::LowHp,
    Fx::Damage,
    Fx::Explosion,
    Fx::Streak,
    Fx::Kill,
    Fx::Hit,
    Fx::Flashed,
];

impl CodState {
    /// Report whether CoD is the foreground window (call ~1–4×/s). Going to the background
    /// clears the live effects at once, so the desktop never triggers anything.
    pub fn set_foreground(&mut self, is_cod_in_front: bool) {
        self.foreground = is_cod_in_front;
        if self.paused() {
            self.det = Detect::fresh();
        }
    }

    pub fn paused(&self) -> bool {
        self.only_in_front && !self.foreground
    }

    pub fn sensitivity(&self, fx: Fx) -> u8 {
        self.sensitivity
            .get(&fx)
            .copied()
            .unwrap_or(fx.default_sensitivity())
            .clamp(1, 5)
    }

    pub fn set_sensitivity(&mut self, fx: Fx, s: u8) {
        self.sensitivity.insert(fx, s.clamp(1, 5));
    }

    pub fn level(&self, fx: Fx) -> f32 {
        self.fx_level
            .get(&fx)
            .copied()
            .unwrap_or(1.0)
            .clamp(0.0, 1.0)
    }

    pub fn calibrated_for(&self, fx: Fx) -> bool {
        let p = self.calibration.profile();
        match fx {
            Fx::Hit => p.and_then(|p| p.hit()).is_some(),
            Fx::Kill => p.and_then(|p| p.kill()).is_some(),
            Fx::Streak => p.and_then(|p| p.streak()).is_some(),
            _ => true,
        }
    }

    pub fn is_on(&self, fx: Fx) -> bool {
        !self.fx_off.contains(&fx) && self.calibrated_for(fx)
    }

    /// How sure the live detector is right now, 0..1 (≥ 0.5 means it is firing). 0 while
    /// paused, while its input is stale, and for HUD effects before calibration.
    pub fn confidence(&self, fx: Fx) -> f32 {
        if self.paused() || !self.calibrated_for(fx) {
            return 0.0;
        }
        self.det.conf[fx.i()]
    }

    /// Fire an effect from the UI; it plays once, in place of live effects (also while paused).
    pub fn fire_test(&mut self, fx: Fx, now: Instant) {
        self.test = Some((fx, now));
        self.last_event = format!("test: {}", fx.label().to_lowercase());
    }

    fn event(&mut self, fx: Fx, now: Instant, strength: f32) {
        self.det.shot[fx.i()] = Some((now, strength.clamp(0.0, 1.0)));
        self.note(fx);
    }

    fn note(&mut self, fx: Fx) {
        if !self.fx_off.contains(&fx) {
            self.last_event = fx.label().to_lowercase();
        }
    }

    fn shot_age(&self, fx: Fx, now: Instant) -> f32 {
        self.det.shot[fx.i()].map_or(f32::INFINITY, |(t, _)| secs(t, now))
    }

    /// Feed one analyzed frame (call for every captured frame, any rate from ~10 to 60 Hz).
    pub fn update_screen(&mut self, f: &ScreenFeatures, now: Instant) {
        if !f.valid {
            return;
        }
        if self.paused() {
            self.det = Detect::fresh();
            return;
        }
        self.calibration.observe(f);
        let dt = self
            .det
            .last_screen
            .map_or(1.0 / 30.0, |t| secs(t, now))
            .clamp(0.0, 0.25);
        self.det.last_screen = Some(now);
        let profile = self.calibration.profile().copied().unwrap_or_default();

        self.detect_flash(f, profile.flash(), dt, now);
        self.detect_vignette(f, profile.vignette(), dt, now);
        self.detect_nvg_gas(f, dt, now);
        self.detect_hud(f, &profile, now);
    }

    /// Flashed: engages only on a *sudden* whitening (a white wall you pan onto doesn't count),
    /// then follows the screen's whiteness frame by frame: instant rise, 120 ms release.
    fn detect_flash(&mut self, f: &ScreenFeatures, peak: Option<f32>, dt: f32, now: Instant) {
        let full = peak.unwrap_or(0.92).clamp(0.3, 1.0);
        let base = 0.2 * full;
        let thr = base + 0.7 * full * sens_frac(self.sensitivity(Fx::Flashed));
        let d = &mut self.det;
        if f.white_frac < thr - 0.2 {
            d.flash_low_at = Some(now);
        }
        let sudden = d.flash_low_at.is_some_and(|t| secs(t, now) < 0.5);
        let mut started = false;
        if !d.flash_on && f.white_frac >= thr && sudden {
            d.flash_on = true;
            started = true;
        } else if d.flash_on && f.white_frac < thr * 0.6 {
            d.flash_on = false;
        }
        let target = if d.flash_on {
            (f.white_frac / full).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let l = &mut d.level[Fx::Flashed.i()];
        if target >= *l {
            *l = target
        } else {
            approach(l, target, ema(0.12, dt))
        }
        let c = conf_of(f.white_frac, base, thr);
        d.conf[Fx::Flashed.i()] = if d.flash_on { c.max(0.5) } else { c.min(0.49) };
        if started {
            self.note(Fx::Flashed);
        }
    }

    /// Damage: a short pulse when the edge vignette crosses the threshold (or jumps again).
    /// Low health: the vignette stays up > 1 s → sustained heartbeat until it fades for 0.4 s.
    fn detect_vignette(
        &mut self,
        f: &ScreenFeatures,
        learned: Option<(f32, f32)>,
        dt: f32,
        now: Instant,
    ) {
        let (base, peak) = learned.unwrap_or((0.0, 0.5));
        let thr = base + (peak - base) * sens_frac(self.sensitivity(Fx::Damage));
        let low_thr = base + (peak - base) * sens_frac(self.sensitivity(Fx::LowHp));
        let v = f.red_vignette;
        let age = self.shot_age(Fx::Damage, now);
        let jump = v - self.det.dmg_prev >= 0.5 * (thr - base);
        if v >= thr && age > 0.3 && (self.det.dmg_armed || jump) {
            self.det.dmg_armed = false;
            self.event(
                Fx::Damage,
                now,
                (0.6 + (v - thr) / (peak - base).max(0.05)).min(1.0),
            );
        }
        if v < base + 0.7 * (thr - base) {
            self.det.dmg_armed = true;
        }
        self.det.dmg_prev = v;
        self.det.conf[Fx::Damage.i()] = conf_of(v, base, thr);

        let d = &mut self.det;
        let on = d.lowhp.update(
            v >= base + 0.8 * (low_thr - base),
            v < base + 0.6 * (low_thr - base),
            1.0,
            0.4,
            now,
        );
        let l = &mut d.level[Fx::LowHp.i()];
        approach(l, if d.lowhp.on { 1.0 } else { 0.0 }, ema(0.3, dt));
        d.conf[Fx::LowHp.i()] = if d.lowhp.on {
            conf_of(v, base, low_thr).max(0.5)
        } else {
            conf_of(v, base, low_thr).min(0.49)
        };
        if on {
            self.note(Fx::LowHp);
        }
    }

    /// Night vision: green-monochrome frame for 0.35 s. Gas: washed-out (or yellow-green) frame
    /// whose colour dropped well below the recent average, for 0.8 s.
    fn detect_nvg_gas(&mut self, f: &ScreenFeatures, dt: f32, now: Instant) {
        let nvg_thr = 0.3 + 0.6 * sens_frac(self.sensitivity(Fx::Nvg));
        let gas_thr = 0.3 + 0.7 * sens_frac(self.sensitivity(Fx::Gas));
        let d = &mut self.det;
        let nvg_on = d.nvg.update(
            f.green_mono >= nvg_thr,
            f.green_mono < 0.75 * nvg_thr,
            0.35,
            0.3,
            now,
        );
        let l = &mut d.level[Fx::Nvg.i()];
        approach(l, if d.nvg.on { 1.0 } else { 0.0 }, ema(0.25, dt));
        d.conf[Fx::Nvg.i()] = conf_of(f.green_mono, 0.3, nvg_thr);

        let sat_avg = *d.sat_avg.get_or_insert(f.saturation);
        let washed = f.saturation < 0.7 * sat_avg || f.haze >= 0.5;
        let gas_on = d.gas.update(
            f.gas >= gas_thr && washed && !d.flash_on,
            f.gas < 0.7 * gas_thr,
            0.8,
            0.6,
            now,
        );
        if !d.gas.on {
            d.sat_avg = Some(sat_avg + (f.saturation - sat_avg) * ema(6.0, dt));
        }
        let l = &mut d.level[Fx::Gas.i()];
        approach(l, if d.gas.on { 1.0 } else { 0.0 }, ema(0.4, dt));
        d.conf[Fx::Gas.i()] = if washed {
            conf_of(f.gas, 0.3, gas_thr)
        } else {
            0.0
        };
        if nvg_on {
            self.note(Fx::Nvg);
        }
        if gas_on {
            self.note(Fx::Gas);
        }
    }

    /// HUD: hit marker / kill pop-up are rising edges over the learned baseline; the streak
    /// icon must stay lit for 0.7 s and fires once until it goes dark again for 2 s.
    fn detect_hud(&mut self, f: &ScreenFeatures, p: &HudProfile, now: Instant) {
        let flash = self.det.flash_on;
        for (fx, learned, signal) in [
            (Fx::Hit, p.hit(), f.hit_signal),
            (Fx::Kill, p.kill(), f.kill_signal),
        ] {
            let Some((base, peak)) = learned else {
                self.det.conf[fx.i()] = 0.0;
                continue;
            };
            let thr = base + (peak - base) * sens_frac(self.sensitivity(fx));
            let armed = if fx == Fx::Hit {
                self.det.hit_armed
            } else {
                self.det.kill_armed
            };
            let mut next_armed = armed;
            if armed && signal >= thr && !flash {
                next_armed = false;
                self.event(fx, now, 1.0);
            } else if signal < base + 0.5 * (thr - base) {
                next_armed = true;
            }
            if fx == Fx::Hit {
                self.det.hit_armed = next_armed;
            } else {
                self.det.kill_armed = next_armed;
            }
            self.det.conf[fx.i()] = conf_of(signal, base, thr);
        }

        let Some((base, high)) = p.streak() else {
            self.det.conf[Fx::Streak.i()] = 0.0;
            return;
        };
        let thr = base + (high - base) * (0.5 + sens_frac(self.sensitivity(Fx::Streak)));
        let s = f.streak_signal;
        if self
            .det
            .streak
            .update(s >= thr, s < base + 0.5 * (thr - base), 0.7, 2.0, now)
        {
            self.event(Fx::Streak, now, 1.0);
        }
        self.det.conf[Fx::Streak.i()] = conf_of(s, base, thr);
    }

    /// Feed the latest audio levels (call every frame; cheap). Stale levels mean silence.
    pub fn update_audio(&mut self, a: &AudioLevels, now: Instant) {
        if self.paused() {
            self.det = Detect::fresh();
            return;
        }
        let dt = self
            .det
            .last_audio
            .map_or(1.0 / 30.0, |t| secs(t, now))
            .clamp(0.0, 0.25);
        self.det.last_audio = Some(now);
        let (boom, density) = if a.fresh(now) {
            (a.boom, a.onsets_per_s)
        } else {
            (0.0, 0.0)
        };

        // Explosion: a rising low-band jump (dB over its 1.5 s average), 350 ms refractory
        let thr = 20.0 * sens_frac(self.sensitivity(Fx::Explosion));
        if boom >= thr && self.det.boom_armed && self.shot_age(Fx::Explosion, now) > 0.35 {
            self.det.boom_armed = false;
            self.event(Fx::Explosion, now, (0.6 + (boom - thr) / 15.0).min(1.0));
        } else if boom < 0.5 * thr {
            self.det.boom_armed = true;
        }
        self.det.conf[Fx::Explosion.i()] = conf_of(boom, 0.0, thr);

        // Firefight: onset density builds the level slowly (1.2 s) and lets it settle (2.5 s)
        let thr = 6.0 * sens_frac(self.sensitivity(Fx::Firefight));
        let target = ((density - 0.5 * thr) / thr).clamp(0.0, 1.0);
        let l = &mut self.det.level[Fx::Firefight.i()];
        let was = *l;
        let tau = if target > *l { 1.2 } else { 2.5 };
        approach(l, target, ema(tau, dt));
        let crossed = was < 0.3 && *l >= 0.3;
        self.det.conf[Fx::Firefight.i()] = conf_of(density, 0.0, thr);
        if crossed {
            self.note(Fx::Firefight);
        }
    }

    /// Light effects for this instant, with the user's switches, strengths and calm applied.
    /// Later overlays win.
    pub fn overlays(&self, now: Instant) -> Vec<Overlay> {
        let t_now = now_secs(now);
        if let Some((fx, t0)) = self.test {
            let t = secs(t0, now);
            if t < fx.duration() {
                return self.shaped(fx, paint(fx, demo(fx, t), self.calm, t_now), self.level(fx));
            }
        }
        if self.paused() {
            return Vec::new();
        }
        let screen_fresh = self.det.last_screen.is_some_and(|t| secs(t, now) < 0.5);
        let audio_fresh = self.det.last_audio.is_some_and(|t| secs(t, now) < 0.5);
        let mut out = Vec::new();
        for fx in PAINT_ORDER {
            if !self.is_on(fx) {
                continue;
            }
            let continuous = match fx.source() {
                Source::Sound => audio_fresh,
                _ => screen_fresh,
            };
            let live = Live {
                level: if continuous {
                    self.det.level[fx.i()]
                } else {
                    0.0
                },
                shot: self.det.shot[fx.i()].map(|(t, s)| (secs(t, now), s)),
            };
            out.extend(self.shaped(fx, paint(fx, live, self.calm, t_now), self.level(fx)));
        }
        out
    }

    fn shaped(&self, fx: Fx, overlays: Vec<Overlay>, k: f32) -> Vec<Overlay> {
        let soften = self.calm && matches!(fx, Fx::Flashed | Fx::Explosion);
        overlays
            .into_iter()
            .map(|o| match o {
                Overlay::Tint { color, alpha } if soften => Overlay::Tint {
                    color: color.lerp(Rgb::new(0.79, 0.78, 0.75), 0.6),
                    alpha: alpha * k * 0.45,
                },
                Overlay::Tint { color, alpha } => Overlay::Tint {
                    color,
                    alpha: alpha * k,
                },
                Overlay::Desaturate(a) => Overlay::Desaturate(a * k),
                Overlay::Dim(a) => Overlay::Dim(a * k),
            })
            .collect()
    }
}

/// What an effect looks like `t` seconds into a TEST play.
fn demo(fx: Fx, t: f32) -> Live {
    let d = fx.duration();
    let fade = |rise: f32, fall: f32| (t / rise).min(1.0) * ((d - t) / fall).clamp(0.0, 1.0);
    match fx {
        Fx::Flashed => Live {
            level: (1.0 - t / d).clamp(0.0, 1.0).powf(0.7),
            shot: None,
        },
        Fx::LowHp => Live {
            level: fade(0.3, 0.3),
            shot: None,
        },
        Fx::Nvg | Fx::Gas => Live {
            level: fade(0.4, 0.4),
            shot: None,
        },
        Fx::Firefight => Live {
            level: fade(0.6 * d, 0.8),
            shot: None,
        },
        Fx::Damage | Fx::Explosion | Fx::Hit | Fx::Kill | Fx::Streak => Live {
            level: 0.0,
            shot: Some((t, 1.0)),
        },
    }
}

/// Overlays for one effect. `t_now` drives heartbeats and flicker.
fn paint(fx: Fx, live: Live, calm: bool, t_now: f32) -> Vec<Overlay> {
    let tint = |color: Rgb, alpha: f32| Overlay::Tint {
        color,
        alpha: alpha.clamp(0.0, 1.0),
    };
    let l = live.level.clamp(0.0, 1.0);
    let shot = live
        .shot
        .filter(|(age, _)| *age >= 0.0 && *age < fx.duration());
    let attack = |age: f32, rise: f32| (age / rise).min(1.0);
    let mut out = Vec::new();
    match fx {
        Fx::Flashed if l > 0.005 => out.push(tint(Rgb::WHITE, l)),
        Fx::LowHp if l > 0.005 => {
            let beat = heartbeat((t_now * 0.9).fract());
            out.push(tint(fx.color(), l * (0.22 + 0.55 * beat)));
        }
        Fx::Nvg if l > 0.005 => {
            out.push(Overlay::Dim(0.45 * l));
            out.push(tint(fx.color(), 0.55 * l));
        }
        Fx::Gas if l > 0.005 => {
            out.push(Overlay::Desaturate(0.8 * l));
            out.push(tint(fx.color(), 0.3 * l));
        }
        Fx::Firefight if l > 0.005 => {
            let flick = if calm {
                1.0
            } else {
                0.55 + 0.45 * value_noise(t_now * 11.0)
            };
            out.push(tint(fx.color(), 0.45 * l * flick));
        }
        _ => {}
    }
    if let Some((age, s)) = shot {
        match fx {
            Fx::Damage => out.push(tint(
                fx.color(),
                s * 0.7 * attack(age, 0.03) * (-age / 0.15).exp(),
            )),
            Fx::Explosion => {
                let a = s * 0.9 * attack(age, if calm { 0.25 } else { 0.03 }) * (-age / 0.45).exp();
                out.push(tint(
                    Rgb::new(1.0, 0.85, 0.6).lerp(fx.color(), (age / 0.25).min(1.0)),
                    a,
                ));
            }
            Fx::Hit => out.push(tint(fx.color(), s * 0.4 * (-age / 0.1).exp())),
            Fx::Kill => out.push(tint(
                fx.color(),
                s * 0.65 * attack(age, 0.02) * (-age / 0.18).exp(),
            )),
            Fx::Streak => out.push(tint(
                fx.color(),
                s * 0.6 * (std::f32::consts::PI * age / fx.duration()).sin(),
            )),
            _ => {}
        }
    }
    out
}

fn now_secs(now: Instant) -> f32 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    secs(*START.get_or_init(Instant::now), now)
}

/// Two-bump "lub-dub" over one beat, 0..1.
fn heartbeat(p: f32) -> f32 {
    let bump = |c: f32, w: f32| (-((p - c) / w).powi(2)).exp();
    (bump(0.08, 0.05) + 0.6 * bump(0.28, 0.06)).min(1.0)
}

/// Smooth 1D noise, 0..1.
fn value_noise(t: f32) -> f32 {
    let hash = |i: u32| {
        let mut h = i.wrapping_mul(0x9E37_79B1);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2C1B_3C6D);
        h ^= h >> 12;
        (h & 0xFFFF) as f32 / 65535.0
    };
    let i = t.floor();
    let f = t - i;
    let f = f * f * (3.0 - 2.0 * f);
    let (a, b) = (hash(i as u32), hash(i as u32 + 1));
    a + (b - a) * f
}

/// Is this window title Call of Duty? Matches "Call of Duty®", "Call of Duty®: Modern Warfare®
/// III", "COD", "Modern Warfare", "Warzone"…, but not browser tabs, launchers, streaming or chat
/// apps that merely mention it.
pub fn is_cod_window_title(title: &str) -> bool {
    let t: String = title
        .chars()
        .filter(|c| !matches!(c, '®' | '™' | '©'))
        .collect::<String>()
        .to_lowercase();
    let t = t.trim();
    const NOT_GAME: [&str; 16] = [
        "chrome", "firefox", "edge", "opera", "brave", "youtube", "twitch", "discord", "reddit",
        "steam", "obs", "roksync", "explorer", "vlc", "spotify", "store",
    ];
    if t.contains("battle.net")
        || t.split(|c: char| !c.is_alphanumeric())
            .any(|w| NOT_GAME.contains(&w))
    {
        return false;
    }
    t.contains("call of duty")
        || t == "cod"
        || t.starts_with("cod ")
        || t.starts_with("cod:")
        || t.starts_with("modern warfare")
        || t.starts_with("black ops")
        || t.starts_with("warzone")
}

/// Title of the foreground window. Reads only the title (`GetForegroundWindow` +
/// `GetWindowTextW`): no process handle, nothing that touches the game.
#[cfg(windows)]
pub fn foreground_title() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
    // SAFETY: both calls only read; GetWindowTextW writes at most `buf.len()` u16s into our
    // buffer and tolerates a null or already-destroyed window (returns 0).
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut buf);
        let n = usize::try_from(n).ok().filter(|&n| n > 0)?.min(buf.len());
        Some(String::from_utf16_lossy(&buf[..n]))
    }
}

#[cfg(not(windows))]
pub fn foreground_title() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::screen::synth;
    use super::*;
    use std::time::Duration;

    fn ms(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    fn live() -> CodState {
        let mut s = CodState::default();
        s.set_foreground(true);
        s
    }

    fn alpha_of(o: &[Overlay]) -> f32 {
        o.iter()
            .map(|o| match *o {
                Overlay::Tint { alpha, .. } => alpha,
                Overlay::Dim(a) | Overlay::Desaturate(a) => a,
            })
            .fold(0.0, f32::max)
    }

    /// Feeds `feat` every 33 ms from `from` for `dur` ms; returns the time after.
    fn feed(s: &mut CodState, feat: &ScreenFeatures, from: Instant, dur: u64) -> Instant {
        let mut t = from;
        while t < ms(from, dur) {
            s.update_screen(feat, t);
            t = ms(t, 33);
        }
        t
    }

    #[test]
    fn neutral_frames_do_nothing() {
        let mut s = live();
        s.fx_off.clear();
        let t = feed(&mut s, &analyze(&synth::neutral()), Instant::now(), 3000);
        assert!(s.overlays(t).is_empty(), "{:?}", s.overlays(t));
        assert!(
            Fx::ALL.iter().all(|&f| s.confidence(f) < 0.5),
            "{:?}",
            Fx::ALL.map(|f| s.confidence(f))
        );
    }

    #[test]
    fn flashbang_follows_the_whiteness() {
        let mut s = live();
        let t0 = Instant::now();
        let t = feed(&mut s, &analyze(&synth::neutral()), t0, 500);
        s.update_screen(&analyze(&synth::white()), t);
        assert!(alpha_of(&s.overlays(t)) > 0.9, "instant rise");
        assert!(s.confidence(Fx::Flashed) >= 0.5);
        assert_eq!(s.last_event, "flashed or stunned");
        let t = feed(&mut s, &analyze(&synth::white()), t, 1000);
        assert!(alpha_of(&s.overlays(t)) > 0.9, "holds while white");
        let t = feed(&mut s, &analyze(&synth::neutral()), t, 500);
        assert!(
            s.overlays(t).is_empty(),
            "gone soon after the screen clears"
        );
    }

    #[test]
    fn slowly_white_scene_is_not_a_flash() {
        let mut s = live();
        // a white screen with no dark frame before it (e.g. panning onto snow / a white menu)
        let t = feed(&mut s, &analyze(&synth::white()), Instant::now(), 1500);
        assert!(s.overlays(t).is_empty());
    }

    #[test]
    fn red_edges_pulse_damage_then_low_health() {
        let mut s = live();
        let t0 = Instant::now();
        let t = feed(&mut s, &analyze(&synth::neutral()), t0, 300);
        let red = analyze(&synth::red_edges(40));
        s.update_screen(&red, t);
        assert!(s.det.shot[Fx::Damage.i()].is_some(), "damage fired");
        let pulse = alpha_of(&s.overlays(ms(t, 60)));
        assert!(pulse > 0.3, "{pulse}");
        // short pulse: gone within ~0.6 s even though the edges stay red
        let t = feed(&mut s, &red, ms(t, 33), 700);
        assert!(!s.det.lowhp.on || secs(t0, t) > 1.0);
        let t = feed(&mut s, &red, t, 1000);
        assert!(s.det.lowhp.on, "low health on after > 1 s");
        assert!(alpha_of(&s.overlays(t)) > 0.1);
        let t = feed(&mut s, &analyze(&synth::neutral()), t, 2000);
        assert!(s.overlays(t).is_empty());
    }

    #[test]
    fn green_mono_turns_on_night_vision() {
        let mut s = live();
        let t = feed(&mut s, &analyze(&synth::nvg()), Instant::now(), 1500);
        assert!(s.det.nvg.on);
        let o = s.overlays(t);
        assert!(
            o.iter().any(|o| matches!(o, Overlay::Dim(a) if *a > 0.3)),
            "{o:?}"
        );
        assert_eq!(s.last_event, "night vision");
    }

    #[test]
    fn gas_needs_a_colour_drop() {
        let mut s = live();
        s.fx_off.clear();
        let grey = ScreenFeatures {
            valid: true,
            mean_luma: 0.55,
            saturation: 0.02,
            gas: 1.0,
            ..Default::default()
        };
        let colour = ScreenFeatures {
            saturation: 0.2,
            gas: 0.0,
            ..grey
        };
        // grey from the start (e.g. a menu): nothing
        let t = feed(&mut s, &grey, Instant::now(), 2000);
        assert!(!s.det.gas.on);
        // colourful gameplay, then the frame washes out: gas
        let t = feed(&mut s, &colour, t, 8000);
        let t = feed(&mut s, &grey, t, 1500);
        assert!(s.det.gas.on);
        assert!(s
            .overlays(t)
            .iter()
            .any(|o| matches!(o, Overlay::Desaturate(a) if *a > 0.3)));
    }

    #[test]
    fn audio_explosion_and_firefight() {
        let mut s = live();
        let t0 = Instant::now();
        let at = |t: Instant, boom: f32, rate: f32| AudioLevels {
            updated: Some(t),
            boom,
            onsets_per_s: rate,
            ..Default::default()
        };
        s.update_audio(&at(t0, 2.0, 0.0), t0);
        assert!(s.overlays(t0).is_empty());
        let t1 = ms(t0, 33);
        s.update_audio(&at(t1, 24.0, 0.0), t1);
        assert_eq!(s.last_event, "explosions");
        assert!(alpha_of(&s.overlays(ms(t1, 50))) > 0.5, "orange burst");
        assert!(s.overlays(ms(t1, 1700)).is_empty(), "burst is short");
        // held boom (peak-hold) does not re-fire
        let shot = s.det.shot[Fx::Explosion.i()];
        s.update_audio(&at(ms(t1, 66), 24.0, 0.0), ms(t1, 66));
        assert_eq!(s.det.shot[Fx::Explosion.i()], shot);

        let mut t = ms(t1, 2000);
        let mut prev = 0.0;
        for i in 0..90 {
            s.update_audio(&at(t, 0.0, 8.0), t);
            let l = s.det.level[Fx::Firefight.i()];
            assert!(l >= prev - 1e-6, "builds monotonically");
            prev = l;
            if i == 3 {
                assert!(l < 0.3, "builds, doesn't jump: {l}");
            }
            t = ms(t, 33);
        }
        assert!(prev > 0.7, "{prev}");
        assert!(alpha_of(&s.overlays(t)) > 0.1);
        for _ in 0..400 {
            s.update_audio(&at(t, 0.0, 0.0), t);
            t = ms(t, 33);
        }
        assert!(s.overlays(t).is_empty(), "settles");
        // silence: WASAPI stops sending, the levels go stale → treated as quiet
        s.update_audio(&at(t0, 30.0, 9.0), t);
        assert_eq!(s.confidence(Fx::Explosion), 0.0);
    }

    #[test]
    fn sensitivity_moves_the_threshold() {
        let weak = ScreenFeatures {
            valid: true,
            red_vignette: 0.3,
            ..Default::default()
        };
        let quiet = ScreenFeatures {
            valid: true,
            ..Default::default()
        };
        let fires = |sens: u8| {
            let mut s = live();
            s.set_sensitivity(Fx::Damage, sens);
            let t0 = Instant::now();
            s.update_screen(&quiet, t0);
            s.update_screen(&weak, ms(t0, 33));
            s.det.shot[Fx::Damage.i()].is_some()
        };
        assert!(!fires(1));
        assert!(fires(5));
        let mut s = live();
        s.set_sensitivity(Fx::Hit, 9);
        assert_eq!(s.sensitivity(Fx::Hit), 5);
        assert_eq!(s.sensitivity(Fx::Nvg), 3);
    }

    #[test]
    fn switches_calm_and_focus() {
        let now = Instant::now();
        let mut s = live();
        let t = feed(&mut s, &analyze(&synth::neutral()), now, 300);
        s.update_screen(&analyze(&synth::white()), t);
        let full = alpha_of(&s.overlays(t));
        s.fx_level.insert(Fx::Flashed, 0.5);
        assert!((alpha_of(&s.overlays(t)) - full * 0.5).abs() < 1e-4);
        s.calm = true;
        assert!(
            alpha_of(&s.overlays(t)) < full * 0.3,
            "reduce flashing softens it"
        );
        s.fx_off.insert(Fx::Flashed);
        assert!(s.overlays(t).is_empty());
        // alt-tab: everything stops at once, frames are ignored
        s.fx_off.clear();
        s.set_foreground(false);
        assert!(s.paused() && s.overlays(t).is_empty() && s.confidence(Fx::Flashed) == 0.0);
        s.update_screen(&analyze(&synth::white()), ms(t, 33));
        assert!(s.overlays(ms(t, 33)).is_empty());
        s.only_in_front = false;
        assert!(!s.paused());
        // TEST plays even while paused, at the chosen strength, then stops
        s.only_in_front = true;
        s.fire_test(Fx::Explosion, t);
        assert!(alpha_of(&s.overlays(ms(t, 100))) > 0.1);
        assert!(s.overlays(ms(t, 5000)).is_empty());
    }

    #[test]
    fn hud_effects_need_calibration() {
        let mut s = live();
        s.fx_off.clear();
        let quiet = analyze(&synth::neutral());
        let hit = analyze(&synth::hit_marker());
        assert!(!s.is_on(Fx::Hit) && !s.is_on(Fx::Kill) && !s.is_on(Fx::Streak));
        let t = feed(&mut s, &quiet, Instant::now(), 300);
        s.update_screen(&hit, t);
        assert_eq!(s.confidence(Fx::Hit), 0.0);
        assert!(s.overlays(t).is_empty());

        let mut t = t;
        s.calibration.advance(); // 1: flash
        t = feed(&mut s, &quiet, t, 300);
        t = feed(&mut s, &analyze(&synth::white()), t, 300);
        t = feed(&mut s, &quiet, t, 300);
        assert!(s.calibration.seen());
        s.calibration.advance(); // 2: damage
        t = feed(&mut s, &analyze(&synth::red_edges(40)), t, 300);
        t = feed(&mut s, &quiet, t, 600);
        assert!(s.calibration.seen());
        s.calibration.advance(); // 3: hit + kill
        let kill = ScreenFeatures {
            kill_signal: 0.3,
            ..quiet
        };
        for _ in 0..5 {
            t = feed(&mut s, &quiet, t, 500);
            t = feed(&mut s, &hit, t, 100);
            t = feed(&mut s, &kill, t, 100);
        }
        assert!(s.calibration.seen());
        assert_eq!(s.calibration.advance(), 4);
        assert!(s.is_on(Fx::Hit) && s.is_on(Fx::Kill));

        // live: a hit marker fires the tick once per appearance
        let t = feed(&mut s, &quiet, t, 500);
        s.update_screen(&hit, t);
        assert!(s.confidence(Fx::Hit) >= 0.5);
        assert!(alpha_of(&s.overlays(ms(t, 20))) > 0.2);
        let first = s.det.shot[Fx::Hit.i()];
        s.update_screen(&hit, ms(t, 33));
        assert_eq!(
            s.det.shot[Fx::Hit.i()],
            first,
            "no re-fire while it stays up"
        );

        let restored = Calibration::from_vec(&s.calibration.to_vec());
        assert_eq!(restored.profile(), s.calibration.profile());
    }

    #[test]
    fn streak_glows_once_when_icon_stays_lit() {
        let mut s = live();
        s.fx_off.clear();
        s.calibration = calibrate::tests::calibrated();
        let dark = ScreenFeatures {
            valid: true,
            saturation: 0.2,
            ..Default::default()
        };
        let lit = ScreenFeatures {
            streak_signal: 0.6,
            ..dark
        };
        let t = feed(&mut s, &dark, Instant::now(), 500);
        let t = feed(&mut s, &lit, t, 400);
        assert!(s.det.shot[Fx::Streak.i()].is_none(), "not before 0.7 s");
        let t = feed(&mut s, &lit, t, 500);
        let first = s.det.shot[Fx::Streak.i()];
        assert!(first.is_some());
        feed(&mut s, &lit, t, 3000);
        assert_eq!(s.det.shot[Fx::Streak.i()], first, "fires once while lit");
    }

    #[test]
    fn every_fx_test_produces_visible_light() {
        let now = Instant::now();
        let mut s = CodState::default(); // paused (no foreground), uncalibrated, gas/streak off
        for fx in Fx::ALL {
            s.fire_test(fx, now);
            for k in [0.1, 0.3, 0.6] {
                let o = s.overlays(now + Duration::from_secs_f32(fx.duration() * k));
                assert!(alpha_of(&o) > 0.05, "{fx:?} at {k}: {o:?}");
            }
            assert!(s
                .overlays(now + Duration::from_secs_f32(fx.duration() + 0.05))
                .is_empty());
        }
    }

    #[test]
    fn board_metadata() {
        assert!(Fx::ALL.iter().all(|f| Fx::from_id(f.id()) == Some(*f)));
        let ids: Vec<_> = Group::ALL
            .iter()
            .flat_map(|g| g.effects().iter().map(|f| f.id()))
            .collect();
        assert_eq!(
            ids,
            [
                "flashed",
                "damage",
                "lowhp",
                "nvg",
                "gas",
                "explosion",
                "firefight",
                "hit",
                "kill",
                "streak"
            ]
        );
        for g in Group::ALL {
            assert!(g.effects().iter().all(|f| f.group() == g));
        }
        assert_eq!(Fx::Streak.default_sensitivity(), 1);
        assert_eq!(Fx::Hit.source().tag(), "HUD");
        assert_eq!(
            Fx::Damage
                .color()
                .to_hex()
                .to_uppercase()
                .trim_start_matches('#'),
            "D7263D"
        );
        let s = CodState::default();
        assert!(!s.is_on(Fx::Gas) && s.is_on(Fx::Flashed) && s.only_in_front && s.paused());
    }

    #[test]
    fn recognises_cod_titles() {
        for t in [
            "Call of Duty®",
            "Call of Duty® HQ",
            "Call of Duty®: Modern Warfare® III",
            "COD",
            "Call of Duty: Black Ops 6",
            "Modern Warfare",
        ] {
            assert!(is_cod_window_title(t), "{t}");
        }
        for t in [
            "Call of Duty - YouTube - Google Chrome",
            "Battle.net",
            "Call of Duty® on Steam",
            "Discord | #cod",
            "RokSync",
            "Desktop",
            "Codex",
            "",
        ] {
            assert!(!is_cod_window_title(t), "{t}");
        }
    }
}
