//! Game modes: one pick tunes screen sync, game-effect strength and performance for a genre.

use crate::perf::Perf;
use lumen_screen::{SampleMode, ScreenSettings};

#[allow(dead_code)]

pub struct Mode {
    pub id: &'static str,
    pub name: &'static str,
    pub games: &'static str,
    pub blurb: &'static str,
    pub art: [[u8; 3]; 3],
    pub perf: Perf,
    /// How hard game events hit the lights, 0..1.
    pub fx_gain: f32,
    /// Card stat bars: reaction, immersion, intensity (0..1).
    pub stats: [f32; 3],
    smoothing: f32,
    saturation: f32,
    min_brightness: f32,
    max_brightness: f32,
    zone: f32,
    sample: SampleMode,
    bars: bool,
}

pub const MODES: &[Mode] = &[
    Mode {
        id: "competitive",
        name: "Competitive",
        games: "CS2 · Valorant · Overwatch",
        blurb: "Low glare, true colour. Only the callouts that matter reach the room.",
        art: [[0, 229, 255], [220, 240, 255], [8, 24, 64]],
        perf: Perf::High,
        fx_gain: 0.55,
        stats: [0.9, 0.3, 0.3],
        smoothing: 0.15,
        saturation: 1.0,
        min_brightness: 0.0,
        max_brightness: 0.55,
        zone: 0.3,
        sample: SampleMode::Average,
        bars: true,
    },
    Mode {
        id: "shooter",
        name: "Fast Shooter",
        games: "Doom · Apex · Call of Duty",
        blurb: "Every muzzle flash hits the room. Fast, saturated, all effects on.",
        art: [[255, 84, 0], [255, 0, 48], [255, 206, 0]],
        perf: Perf::Extreme,
        fx_gain: 1.0,
        stats: [1.0, 0.7, 1.0],
        smoothing: 0.04,
        saturation: 1.45,
        min_brightness: 0.04,
        max_brightness: 1.0,
        zone: 0.25,
        sample: SampleMode::Vibrant,
        bars: true,
    },
    Mode {
        id: "cinematic",
        name: "Cinematic",
        games: "Cyberpunk · Death Stranding · films",
        blurb: "Smooth, natural colour that follows the shot. Ignores black bars.",
        art: [[0, 150, 160], [255, 140, 40], [10, 20, 30]],
        perf: Perf::High,
        fx_gain: 0.8,
        stats: [0.35, 1.0, 0.6],
        smoothing: 0.72,
        saturation: 1.1,
        min_brightness: 0.02,
        max_brightness: 0.85,
        zone: 0.4,
        sample: SampleMode::Average,
        bars: true,
    },
    Mode {
        id: "rpg",
        name: "RPG & Worlds",
        games: "Expedition 33 · Final Fantasy · Hades",
        blurb: "Saturated colour. The room shifts with every biome and spell.",
        art: [[130, 60, 255], [255, 190, 60], [0, 200, 130]],
        perf: Perf::Medium,
        fx_gain: 0.9,
        stats: [0.55, 0.95, 0.75],
        smoothing: 0.5,
        saturation: 1.35,
        min_brightness: 0.03,
        max_brightness: 0.9,
        zone: 0.45,
        sample: SampleMode::Vibrant,
        bars: true,
    },
    Mode {
        id: "horror",
        name: "Horror",
        games: "Resident Evil · The Complex · Silent Hill",
        blurb: "Darkness stays dark. Light only bleeds in when something is really there.",
        art: [[190, 0, 24], [30, 4, 8], [90, 130, 40]],
        perf: Perf::Medium,
        fx_gain: 1.0,
        stats: [0.6, 1.0, 0.35],
        smoothing: 0.6,
        saturation: 0.95,
        min_brightness: 0.0,
        max_brightness: 0.5,
        zone: 0.3,
        sample: SampleMode::Average,
        bars: true,
    },
    Mode {
        id: "racing",
        name: "Racing",
        games: "Forza · Need for Speed · F1",
        blurb: "Reads the screen edges fast, so the track streaks past.",
        art: [[255, 0, 170], [40, 90, 255], [240, 240, 255]],
        perf: Perf::High,
        fx_gain: 1.0,
        stats: [0.9, 0.8, 0.9],
        smoothing: 0.12,
        saturation: 1.3,
        min_brightness: 0.03,
        max_brightness: 1.0,
        zone: 0.55,
        sample: SampleMode::Vibrant,
        bars: true,
    },
    Mode {
        id: "cozy",
        name: "Cozy",
        games: "Stardew · PlateUp! · KeyWe",
        blurb: "Soft, warm colour that never flashes. Good for late nights.",
        art: [[255, 170, 120], [140, 205, 140], [255, 236, 200]],
        perf: Perf::Low,
        fx_gain: 0.4,
        stats: [0.2, 0.6, 0.4],
        smoothing: 0.85,
        saturation: 1.05,
        min_brightness: 0.08,
        max_brightness: 0.7,
        zone: 0.5,
        sample: SampleMode::Average,
        bars: true,
    },
];

pub fn find(id: &str) -> Option<&'static Mode> {
    MODES.iter().find(|m| m.id == id)
}

impl Mode {
    /// Leaves region, monitor and aspect lock unchanged.
    pub fn apply_to(&self, s: &mut ScreenSettings) {
        s.smoothing = self.smoothing;
        s.saturation = self.saturation;
        s.min_brightness = self.min_brightness;
        s.max_brightness = self.max_brightness;
        s.zone = self.zone;
        s.mode = self.sample;
        s.detect_bars = self.bars;
    }

    /// True when the settings still match this mode (the user hasn't tweaked them).
    pub fn matches(&self, s: &ScreenSettings) -> bool {
        let eq = |a: f32, b: f32| (a - b).abs() < 0.005;
        eq(s.smoothing, self.smoothing)
            && eq(s.saturation, self.saturation)
            && eq(s.min_brightness, self.min_brightness)
            && eq(s.max_brightness, self.max_brightness)
            && eq(s.zone, self.zone)
            && s.mode == self.sample
    }
}

pub fn guess(appid: u32, name: &str) -> &'static str {
    let n = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    if appid == 730 || has(&["counter-strike", "valorant", "overwatch", "rainbow six", "deadlock", "dota", "league of"]) {
        "competitive"
    } else if has(&["need for speed", "forza", "f1 ", "racing", "rally", "dirt", "crew", "trackmania"]) {
        "racing"
    } else if has(&["horror", "resident evil", "silent hill", "outlast", "complex", "seance", "fear", "dead space", "amnesia", "alien"]) {
        "horror"
    } else if has(&["doom", "apex", "call of duty", "battlefield", "halo", "titanfall", "arc raiders", "splitgate", "the finals", "high on life"]) {
        "shooter"
    } else if has(&["final fantasy", "expedition", "elden", "baldur", "witcher", "hades", "dragon", "souls", "palworld", "everspace"]) {
        "rpg"
    } else if has(&["stardew", "plateup", "keywe", "farm", "cozy", "possum", "chicken police", "sackboy", "fall guys", "worms"]) {
        "cozy"
    } else {
        "cinematic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_apply_and_match() {
        for m in MODES {
            let mut s = ScreenSettings::default();
            m.apply_to(&mut s);
            assert!(m.matches(&s), "{}", m.id);
            assert!(find(m.id).is_some());
        }
        assert_eq!(guess(730, "Counter-Strike 2"), "competitive");
        assert_eq!(guess(1, "Need for Speed™ Heat "), "racing");
        assert_eq!(guess(1, "Cyberpunk 2077"), "cinematic");
        assert!(find(guess(1, "anything")).is_some());
    }
}
