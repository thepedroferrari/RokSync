//! Palettes page: a studio (generate, lock, edit, reorder) plus a grid of saved palettes and
//! bridge scenes.

use super::theme::*;
use super::{LumenApp, Page};
use lumen_hue::{Inventory, Rgb};
use lumen_mood::MoodPlan;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq)]
pub struct Col {
    pub c: [u8; 3],
    pub locked: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Harmony {
    Auto,
    Analogous,
    Complement,
    Triad,
    Mono,
}

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq)]
pub enum Vibe {
    Vivid,
    Neon,
    Pastel,
    Deep,
}

pub struct Studio {
    pub cols: Vec<Col>,
    pub(super) harmony: Harmony,
    pub(super) vibe: Vibe,
    pub(super) rng: u64,
    pub(super) name: String,
    pub(super) brightness: f32,
    pub(super) editing: Option<usize>,
    pub(super) history: Vec<Vec<Col>>,
}

impl Default for Studio {
    fn default() -> Self {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7) | 1;
        let mut s = Self {
            cols: Vec::new(),
            harmony: Harmony::Auto,
            vibe: Vibe::Vivid,
            rng: seed,
            name: String::new(),
            brightness: 80.0,
            editing: None,
            history: Vec::new(),
        };
        s.set_colors(&["#ff2e88", "#7b2ff7", "#2d7dff", "#00d1c1", "#ffb000"].map(|h| Rgb::from_hex(h).unwrap()));
        s
    }
}

impl Studio {
    pub(super) fn rand(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        ((self.rng.wrapping_mul(0x2545F4914F6CDD1D) >> 40) as f32) / (1u64 << 24) as f32
    }

    pub(super) fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.rand()
    }

    pub(super) fn snapshot(&mut self) {
        self.history.push(self.cols.clone());
        if self.history.len() > 50 {
            self.history.remove(0);
        }
    }

    pub(super) fn undo(&mut self) {
        if let Some(prev) = self.history.pop() {
            self.cols = prev;
            self.editing = None;
        }
    }

    pub fn colors(&self) -> Vec<Rgb> {
        self.cols.iter().map(|c| Rgb::from_u8(c.c)).collect()
    }

    /// Replaces the whole palette and unlocks every colour.
    pub fn set_colors(&mut self, colors: &[Rgb]) {
        if !self.cols.is_empty() {
            self.snapshot();
        }
        self.cols = colors.iter().take(8).map(|c| Col { c: c.to_u8(), locked: false }).collect();
        while self.cols.len() < 2 {
            self.cols.push(Col { c: [255, 214, 165], locked: false });
        }
        self.editing = None;
    }

    pub(super) fn generate(&mut self) {
        self.snapshot();
        let n = self.cols.len();
        let anchor = self.cols.iter().find(|c| c.locked).map(|c| Rgb::from_u8(c.c).to_hsv());
        let base = match anchor {
            Some((h, _, _)) => h,
            None => self.rand(),
        };
        let harmony = match self.harmony {
            Harmony::Auto => [Harmony::Analogous, Harmony::Complement, Harmony::Triad, Harmony::Analogous, Harmony::Mono]
                [(self.rand() * 5.0) as usize % 5],
            h => h,
        };
        let spread = self.range(0.045, 0.085);
        for i in 0..n {
            if self.cols[i].locked {
                continue;
            }
            let fi = i as f32;
            let off = match harmony {
                Harmony::Analogous => (fi - (n as f32 - 1.0) / 2.0) * spread,
                Harmony::Complement => (if i % 2 == 0 { 0.0 } else { 0.5 }) + (i / 2) as f32 * spread * 0.6,
                Harmony::Triad => (i % 3) as f32 / 3.0 + (i / 3) as f32 * spread * 0.5,
                Harmony::Mono | Harmony::Auto => 0.0,
            };
            let jitter = self.range(-0.015, 0.015);
            let (s, v) = match (self.vibe, harmony) {
                (_, Harmony::Mono) => {
                    let t = if n > 1 { fi / (n as f32 - 1.0) } else { 0.5 };
                    match self.vibe {
                        Vibe::Pastel => (0.45 - 0.3 * t, 0.85 + 0.15 * t),
                        Vibe::Deep => (0.95 - 0.2 * t, 0.25 + 0.4 * t),
                        _ => (1.0 - 0.45 * t, 0.45 + 0.55 * t),
                    }
                }
                (Vibe::Vivid, _) => (self.range(0.7, 1.0), self.range(0.78, 1.0)),
                (Vibe::Neon, _) => (1.0, 1.0),
                (Vibe::Pastel, _) => (self.range(0.22, 0.45), self.range(0.92, 1.0)),
                (Vibe::Deep, _) => (self.range(0.75, 1.0), self.range(0.3, 0.6)),
            };
            self.cols[i].c = Rgb::from_hsv(base + off + jitter, s, v).to_u8();
        }
    }
}


impl LumenApp {
    /// Strips get blends, bulbs get accents. If sync is live it stops first, otherwise the next
    /// streamed frame would overwrite the colours.
    pub fn apply_palette(&mut self, inv: &Inventory, colors: &[Rgb], brightness: f64, title: &str) {
        let lights = self.target(inv).lights;
        let palette: Vec<String> = colors.iter().map(|c| c.to_hex()).collect();
        if palette.is_empty() || lights.is_empty() {
            return;
        }
        let stopped = self.is_streaming();
        if stopped {
            self.stop_live();
        }
        let plan = MoodPlan {
            title: title.to_string(),
            assignments: lumen_mood::plan::arrange(&lights, &palette, brightness, None),
            palette,
            motion: "still".into(),
            speed: "slow".into(),
        };
        let ids: std::collections::HashSet<&str> = lights.iter().map(|l| l.id.as_str()).collect();
        let states: Vec<_> = plan.to_states(inv).into_iter().filter(|(id, _, _)| ids.contains(id.as_str())).collect();
        let title = title.to_string();
        let place = self.target(inv).name;
        if stopped {
            self.toast(format!("Sync stopped for {title} in {place}"), false);
        }
        self.local_edit(|inv| {
            for (id, _, st) in &states {
                super::home::apply_local(inv, id, st);
            }
        });
        let chip = colors.first().map(|c| c32(*c)).unwrap_or(TEXT);
        if self.bridge.is_none() {
            // no bridge (demo): the task below won't run, so log it here
            self.bg.lock().unwrap().note(&title, &format!("On {} lights in {place}", states.len()), chip);
        }
        self.task("Palette", true, move |b| {
            if stopped {
                // the bridge puts the area back the way it was when a stream ends; let that finish
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
            for (id, _, st) in &states {
                b.set_light(id, st)?;
                std::thread::sleep(std::time::Duration::from_millis(110));
            }
            Ok(format!("{title} on {} lights in {place}", states.len()))
        });
    }
    pub fn open_in_studio(&mut self, name: &str, colors: &[Rgb]) {
        self.studio.set_colors(colors);
        self.studio.name = name.trim().to_string();
        self.page = Page::Palettes;
    }

}
