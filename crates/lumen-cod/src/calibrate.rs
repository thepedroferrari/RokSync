//! One-time calibration: learns what the user's flash, damage vignette and HUD look like
//! (resolution, HDR, colour-blind filters and HUD layouts change these), so the live detectors
//! use per-user thresholds.
//!
//! `step()` values:
//! * 0 — not started (or done and about to recalibrate);
//! * 1 — flash: `white_frac` peak;
//! * 2 — damage: red edge vignette (median = baseline, peak = hurt);
//! * 3 — hit marker and kill pop-up signals (baseline + peak);
//! * 4 — done: a [`HudProfile`] is stored.
//!
//! The streak icons' idle level is learned from every calibration frame (they stay dark while
//! calibrating).

use crate::screen::ScreenFeatures;
use serde::{Deserialize, Serialize};

/// Step names and descriptions (index 0 = step 1).
pub const STEPS: [(&str, &str); 3] = [
    (
        "Throw a flash at a wall",
        "Learns how white your flash gets",
    ),
    ("Take a little damage", "Learns your red edge vignette"),
    (
        "Hit and down a bot",
        "Finds your hit marker and kill pop-up",
    ),
];

/// Button text for each `step()` value.
pub const BUTTON: [&str; 5] = [
    "START CALIBRATION",
    "GOT THE FLASH · NEXT",
    "GOT THE DAMAGE · NEXT",
    "GOT THE HIT · FINISH",
    "DONE · RECALIBRATE",
];

const MAX_SAMPLES: usize = 4096;
/// Marks a value that was not learned (the user skipped the step or nothing showed up).
const UNSET: f32 = -1.0;
const VERSION: f32 = 1.0;

/// What calibration learned. Values are in the units of [`ScreenFeatures`]; `-1` = not learned.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HudProfile {
    pub flash_peak: f32,
    pub vignette_base: f32,
    pub vignette_peak: f32,
    pub hit_base: f32,
    pub hit_peak: f32,
    pub kill_base: f32,
    pub kill_peak: f32,
    pub streak_base: f32,
    pub streak_high: f32,
}

impl Default for HudProfile {
    fn default() -> Self {
        Self {
            flash_peak: UNSET,
            vignette_base: UNSET,
            vignette_peak: UNSET,
            hit_base: UNSET,
            hit_peak: UNSET,
            kill_base: UNSET,
            kill_peak: UNSET,
            streak_base: UNSET,
            streak_high: UNSET,
        }
    }
}

fn pair(base: f32, peak: f32) -> Option<(f32, f32)> {
    (base >= 0.0 && peak > base).then_some((base, peak))
}

impl HudProfile {
    /// How white the user's flash gets (fraction of white pixels), if learned.
    pub fn flash(&self) -> Option<f32> {
        (self.flash_peak > 0.0).then_some(self.flash_peak)
    }
    /// Damage vignette (baseline, hurt peak), if learned.
    pub fn vignette(&self) -> Option<(f32, f32)> {
        pair(self.vignette_base, self.vignette_peak)
    }
    /// Hit-marker signal (baseline, peak), if learned.
    pub fn hit(&self) -> Option<(f32, f32)> {
        pair(self.hit_base, self.hit_peak)
    }
    /// Kill pop-up signal (baseline, peak), if learned.
    pub fn kill(&self) -> Option<(f32, f32)> {
        pair(self.kill_base, self.kill_peak)
    }
    /// Streak icons (idle median, idle upper bound), if learned.
    pub fn streak(&self) -> Option<(f32, f32)> {
        pair(self.streak_base, self.streak_high)
    }

    fn fields(&self) -> [f32; 9] {
        [
            self.flash_peak,
            self.vignette_base,
            self.vignette_peak,
            self.hit_base,
            self.hit_peak,
            self.kill_base,
            self.kill_peak,
            self.streak_base,
            self.streak_high,
        ]
    }

    /// Flat form for config files: `[version, fields…]`.
    pub fn to_vec(&self) -> Vec<f32> {
        let mut v = vec![VERSION];
        v.extend_from_slice(&self.fields());
        v
    }

    /// Inverse of [`to_vec`](Self::to_vec). None for a wrong version, length or non-finite value.
    pub fn from_vec(v: &[f32]) -> Option<Self> {
        if v.len() != 10 || v[0] != VERSION || v.iter().any(|x| !x.is_finite()) {
            return None;
        }
        let f = |i: usize| if v[i] < 0.0 { UNSET } else { v[i].min(1.0) };
        Some(Self {
            flash_peak: f(1),
            vignette_base: f(2),
            vignette_peak: f(3),
            hit_base: f(4),
            hit_peak: f(5),
            kill_base: f(6),
            kill_peak: f(7),
            streak_base: f(8),
            streak_high: f(9),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Calibration {
    step: u8,
    profile: Option<HudProfile>,
    draft: HudProfile,
    flash: Vec<f32>,
    vignette: Vec<f32>,
    hit: Vec<f32>,
    kill: Vec<f32>,
    streak: Vec<f32>,
}

/// Minimum peak-over-baseline for a step to count as "seen".
const SEEN_FLASH: f32 = 0.35;
const SEEN_VIGNETTE: f32 = 0.08;
const SEEN_HUD: f32 = 0.03;

fn push(v: &mut Vec<f32>, x: f32) {
    if v.len() >= MAX_SAMPLES {
        // keep every other sample: memory stays bounded, the distribution stays representative
        let mut i = 0;
        v.retain(|_| {
            i += 1;
            i % 2 == 0
        });
    }
    v.push(x);
}

fn sorted(v: &[f32]) -> Vec<f32> {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    s
}

/// (median, robust peak = 3rd largest so one glitched frame can't set it). None if empty.
fn base_peak(v: &[f32]) -> Option<(f32, f32)> {
    let s = sorted(v);
    let n = s.len();
    (n > 0).then(|| (s[n / 2], s[n.saturating_sub(3)]))
}

fn quantile(v: &[f32], q: f32) -> Option<f32> {
    let s = sorted(v);
    let n = s.len();
    (n > 0).then(|| s[((n - 1) as f32 * q.clamp(0.0, 1.0)) as usize])
}

impl Calibration {
    pub fn from_profile(p: HudProfile) -> Self {
        Self {
            step: 4,
            profile: Some(p),
            ..Default::default()
        }
    }

    /// 0 = not started, 1..=3 = waiting on that step, 4 = done.
    pub fn step(&self) -> u8 {
        self.step
    }

    pub fn in_progress(&self) -> bool {
        (1..=3).contains(&self.step)
    }

    /// A profile exists (HUD effects may run). Kept while recalibrating, replaced on finish.
    pub fn is_done(&self) -> bool {
        self.profile.is_some()
    }

    pub fn profile(&self) -> Option<&HudProfile> {
        self.profile.as_ref()
    }

    pub fn button_label(&self) -> &'static str {
        BUTTON[self.step.min(4) as usize]
    }

    pub fn short_status(&self) -> String {
        match self.step {
            1..=3 => format!("STEP {}/3", self.step),
            _ if self.is_done() => "DONE".into(),
            _ => "NOT YET".into(),
        }
    }

    pub fn observe(&mut self, f: &ScreenFeatures) {
        if !self.in_progress() || !f.valid {
            return;
        }
        push(&mut self.streak, f.streak_signal);
        match self.step {
            1 => push(&mut self.flash, f.white_frac),
            2 => push(&mut self.vignette, f.red_vignette),
            3 => {
                push(&mut self.hit, f.hit_signal);
                push(&mut self.kill, f.kill_signal);
            }
            _ => {}
        }
    }

    /// Has the current step already caught the thing it waits for?
    pub fn seen(&self) -> bool {
        let rise = |v: &[f32], min: f32| base_peak(v).is_some_and(|(b, p)| p - b >= min);
        match self.step {
            1 => base_peak(&self.flash).is_some_and(|(_, p)| p >= SEEN_FLASH),
            2 => rise(&self.vignette, SEEN_VIGNETTE),
            3 => rise(&self.hit, SEEN_HUD) && rise(&self.kill, SEEN_HUD),
            _ => false,
        }
    }

    /// Start → next step → finish → (from done) back to 0 to recalibrate.
    /// Finishing a step learns from what it saw; a step that saw nothing leaves its values unset
    /// (the defaults stay in use for flash/damage, and that HUD effect stays inactive).
    pub fn advance(&mut self) -> u8 {
        let rise = |v: &[f32], min: f32| base_peak(v).filter(|(b, p)| p - b >= min);
        match self.step {
            0 => {
                *self = Self {
                    step: 1,
                    profile: self.profile,
                    ..Default::default()
                };
            }
            1 => {
                if let Some((_, p)) = base_peak(&self.flash).filter(|(_, p)| *p >= SEEN_FLASH) {
                    self.draft.flash_peak = p;
                }
                self.step = 2;
            }
            2 => {
                if let Some((b, p)) = rise(&self.vignette, SEEN_VIGNETTE) {
                    (self.draft.vignette_base, self.draft.vignette_peak) = (b, p);
                }
                self.step = 3;
            }
            3 => {
                if let Some((b, p)) = rise(&self.hit, SEEN_HUD) {
                    (self.draft.hit_base, self.draft.hit_peak) = (b, p);
                }
                if let Some((b, p)) = rise(&self.kill, SEEN_HUD) {
                    (self.draft.kill_base, self.draft.kill_peak) = (b, p);
                }
                if let (Some(b), Some(hi)) =
                    (quantile(&self.streak, 0.5), quantile(&self.streak, 0.95))
                {
                    self.draft.streak_base = b;
                    self.draft.streak_high = (hi + 0.05).max(b + 0.08).min(1.0);
                }
                self.profile = Some(self.draft);
                self.step = 4;
                self.drop_samples();
            }
            _ => self.step = 0,
        }
        self.step
    }

    /// Abandon a calibration in progress; any earlier profile stays.
    pub fn cancel(&mut self) {
        self.step = if self.is_done() { 4 } else { 0 };
        self.drop_samples();
    }

    /// Forget everything (HUD effects go inactive).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn drop_samples(&mut self) {
        for v in [
            &mut self.flash,
            &mut self.vignette,
            &mut self.hit,
            &mut self.kill,
            &mut self.streak,
        ] {
            *v = Vec::new();
        }
        self.draft = HudProfile::default();
    }

    /// Flat form for config files (`[]` when not calibrated).
    pub fn to_vec(&self) -> Vec<f32> {
        self.profile.map(|p| p.to_vec()).unwrap_or_default()
    }

    /// Inverse of [`to_vec`](Self::to_vec); anything invalid gives an uncalibrated state.
    pub fn from_vec(v: &[f32]) -> Self {
        HudProfile::from_vec(v)
            .map(Self::from_profile)
            .unwrap_or_default()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn feat(white: f32, vig: f32, hit: f32, kill: f32) -> ScreenFeatures {
        ScreenFeatures {
            valid: true,
            white_frac: white,
            red_vignette: vig,
            hit_signal: hit,
            kill_signal: kill,
            ..Default::default()
        }
    }

    /// Runs the whole flow with a clear event in every step.
    pub(crate) fn calibrated() -> Calibration {
        let mut c = Calibration::default();
        assert_eq!(c.advance(), 1);
        for i in 0..90 {
            c.observe(&feat(
                if (40..50).contains(&i) { 0.9 } else { 0.02 },
                0.0,
                0.0,
                0.0,
            ));
        }
        assert!(c.seen());
        assert_eq!(c.advance(), 2);
        assert!(!c.seen());
        for i in 0..90 {
            c.observe(&feat(
                0.0,
                if (30..60).contains(&i) { 0.45 } else { 0.03 },
                0.0,
                0.0,
            ));
        }
        assert!(c.seen());
        c.advance();
        for i in 0..120 {
            let ev = i % 30 < 4;
            c.observe(&feat(
                0.0,
                0.0,
                if ev { 0.12 } else { 0.004 },
                if ev { 0.2 } else { 0.01 },
            ));
        }
        assert!(c.seen());
        assert_eq!(c.advance(), 4);
        c
    }

    #[test]
    fn learns_per_user_values() {
        let c = calibrated();
        let p = c.profile().copied().expect("profile");
        assert!((p.flash().unwrap_or(0.0) - 0.9).abs() < 1e-6);
        let (b, pk) = p.vignette().expect("vignette");
        assert!(b < 0.05 && (pk - 0.45).abs() < 1e-6);
        assert!(p.hit().is_some() && p.kill().is_some() && p.streak().is_some());
        assert_eq!(c.short_status(), "DONE");
        assert_eq!(c.button_label(), "DONE · RECALIBRATE");
    }

    #[test]
    fn skipped_steps_stay_unset() {
        let mut c = Calibration::default();
        for _ in 0..4 {
            for _ in 0..30 {
                c.observe(&feat(0.01, 0.01, 0.0, 0.0));
            }
            c.advance();
        }
        let p = c.profile().copied().expect("finished");
        assert!(
            p.flash().is_none()
                && p.vignette().is_none()
                && p.hit().is_none()
                && p.kill().is_none()
        );
    }

    #[test]
    fn round_trips_through_a_vec() {
        let c = calibrated();
        let v = c.to_vec();
        let back = Calibration::from_vec(&v);
        assert_eq!(back.profile(), c.profile());
        assert_eq!(back.step(), 4);
        // invalid input gives an uncalibrated state, never a panic
        assert!(!Calibration::from_vec(&[]).is_done());
        assert!(!Calibration::from_vec(&[2.0; 10]).is_done());
        assert!(!Calibration::from_vec(&[1.0, f32::NAN, 0., 0., 0., 0., 0., 0., 0., 0.]).is_done());
        assert!(Calibration::default().to_vec().is_empty());
    }

    #[test]
    fn recalibrating_keeps_the_old_profile_until_finished() {
        let mut c = calibrated();
        assert_eq!(c.advance(), 0);
        assert_eq!(c.advance(), 1);
        assert!(c.is_done(), "old profile still active");
        c.cancel();
        assert_eq!(c.step(), 4);
        c.reset();
        assert!(!c.is_done());
    }

    #[test]
    fn samples_stay_bounded() {
        let mut c = Calibration::default();
        c.advance();
        for i in 0..20_000 {
            c.observe(&feat((i % 100) as f32 / 100.0, 0.0, 0.0, 0.0));
        }
        assert!(c.flash.len() <= MAX_SAMPLES && c.streak.len() <= MAX_SAMPLES);
    }
}
