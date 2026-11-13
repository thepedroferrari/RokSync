use lumen_engine::{sample_linear, Motion};
use lumen_hue::{Inventory, Light, LightState, Rgb};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Assignment {
    /// Name of a light, room or zone (exactly as on the bridge).
    pub target: String,
    /// 1..5 colours. Gradient lights use them as a gradient; groups spread them across their lights.
    pub colors: Vec<String>,
    /// 0..100
    pub brightness: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MoodPlan {
    pub title: String,
    pub palette: Vec<String>,
    pub assignments: Vec<Assignment>,
    /// still | breathe | drift | flicker
    pub motion: String,
    /// slow | medium | fast
    pub speed: String,
}

pub fn hex(s: &str) -> Option<Rgb> {
    Rgb::from_hex(s)
}

impl MoodPlan {
    /// Keeps the last assignment per target.
    pub fn dedup(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<Assignment> = Vec::new();
        for a in self.assignments.drain(..).rev() {
            if seen.insert(a.target.clone()) {
                out.push(a);
            }
        }
        out.reverse();
        self.assignments = out;
    }

    pub fn palette_rgb(&self) -> Vec<Rgb> {
        self.palette.iter().filter_map(|s| hex(s)).collect()
    }

    pub fn motion(&self) -> Motion {
        let k = match self.speed.as_str() {
            "fast" => 0.4,
            "medium" => 1.0,
            _ => 2.0,
        };
        match self.motion.as_str() {
            "breathe" => Motion::Breathe { period_s: 4.0 * k, depth: 0.6, spread: 0.35 },
            "drift" => Motion::Drift { period_s: 25.0 * k },
            "flicker" => Motion::Flicker { amount: 0.35, speed: 7.0 / k },
            _ => Motion::Static,
        }
    }

    /// Turns the plan into concrete per-light states. Later assignments override earlier ones.
    pub fn to_states(&self, inv: &Inventory) -> Vec<(String, String, LightState)> {
        let mut out: Vec<(String, String, LightState)> = Vec::new();
        let mut set = |l: &Light, st: LightState| {
            out.retain(|(id, _, _)| id != &l.id);
            out.push((l.id.clone(), l.metadata.name.clone(), st));
        };
        // groups first, then individual lights, so specific beats general
        let mut ordered: Vec<&Assignment> = self.assignments.iter().collect();
        ordered.sort_by_key(|a| matches!(inv.find_target(&a.target), Some(lumen_hue::api::Target::Light(_))));
        for a in ordered {
            let cols: Vec<Rgb> = a.colors.iter().filter_map(|s| hex(s)).collect();
            if cols.is_empty() {
                continue;
            }
            let bri = Some(a.brightness.clamp(1.0, 100.0));
            match inv.find_target(&a.target) {
                Some(lumen_hue::api::Target::Light(l)) => set(l, state_for(l, &cols, 0.0, 1.0, bri)),
                Some(lumen_hue::api::Target::Group(g)) => {
                    let mut ls = inv.group_lights(g);
                    ls.sort_by_key(|l| l.metadata.name.to_lowercase());
                    let n = ls.len();
                    for (i, l) in ls.into_iter().enumerate() {
                        let (u0, u1) = if n > 1 { (i as f32 / n as f32, (i + 1) as f32 / n as f32) } else { (0.0, 1.0) };
                        set(l, state_for(l, &cols, u0, u1, bri));
                    }
                }
                None => {}
            }
        }
        out
    }
}

/// Colour for one light covering the [u0,u1] slice of the colour list.
fn state_for(l: &Light, cols: &[Rgb], u0: f32, u1: f32, bri: Option<f64>) -> LightState {
    let k = l.gradient_points() as usize;
    let mut st = if !l.supports_color() {
        LightState { on: Some(true), ..Default::default() }
    } else if k >= 2 && cols.len() > 1 {
        let pts = (0..k).map(|j| sample_linear(cols, u0 + (u1 - u0) * j as f32 / (k - 1) as f32)).collect();
        LightState::gradient(pts)
    } else {
        LightState::color(sample_linear(cols, (u0 + u1) / 2.0))
    };
    st.brightness = bri;
    st.transition_ms = Some(800);
    st
}

/// The model's output: the art direction. `arrange` does the placement.
#[derive(Debug, Clone, Deserialize)]
pub struct RawPlan {
    /// what the description brings to mind, with real colours (the model's scratchpad)
    #[serde(default)]
    pub evokes: String,
    pub title: String,
    /// colour words as people say them, most recognisable first
    pub palette: Vec<String>,
    pub brightness: f64,
    /// light that should be the visual focus, or "none"
    pub focus: String,
    pub motion: String,
    pub speed: String,
}

impl RawPlan {
    pub fn into_plan(self, lights: &[&Light]) -> MoodPlan {
        let mut palette: Vec<String> = Vec::new();
        for n in &self.palette {
            if let Some(h) = crate::colors::resolve(n) {
                if !palette.contains(&h) {
                    palette.push(h);
                }
            }
        }
        if palette.len() < 3 {
            // the model wrote non-colours ("pastel tones", "creativity"); its notes usually name real ones
            for h in crate::colors::extract(&self.evokes) {
                if palette.len() < 5 && !palette.contains(&h) {
                    palette.push(h);
                }
            }
        }
        if palette.is_empty() {
            palette.push("#ffd8a8".into());
        }
        let focus = if self.focus == "none" { None } else { Some(self.focus.as_str()) };
        // flicker is kept only when `evokes` mentions something that flickers
        let flickers = ["fire", "flame", "candle", "storm", "lightning", "thunder", "explosion", "spark", "torch", "glitch", "horror"];
        let motion = if self.motion == "flicker" && !flickers.iter().any(|w| self.evokes.to_lowercase().contains(w)) {
            "still".to_string()
        } else {
            self.motion
        };
        MoodPlan {
            assignments: arrange(lights, &palette, self.brightness, focus),
            title: self.title,
            palette,
            motion,
            speed: self.speed,
        }
    }
}

/// Places a palette on real lights so it looks designed: the focus light carries the key colour,
/// gradient strips get rotating blends, bulbs alternate accents, white-only lights stay warm and low.
pub fn arrange(lights: &[&Light], palette: &[String], brightness: f64, focus: Option<&str>) -> Vec<Assignment> {
    let n = palette.len().max(1);
    let base = brightness.clamp(1.0, 100.0);
    let r = |v: f64| v.round().clamp(1.0, 100.0);
    let mut sorted: Vec<&&Light> = lights.iter().collect();
    sorted.sort_by_key(|l| l.metadata.name.to_lowercase());
    let mut strip_i = 0usize;
    let mut bulb_i = 0usize;
    sorted
        .into_iter()
        .map(|l| {
            let name = l.metadata.name.clone();
            let is_focus = focus.map_or(false, |f| f.trim().eq_ignore_ascii_case(name.trim()));
            if !l.supports_color() {
                return Assignment { target: name, colors: vec!["#ffd8a8".into()], brightness: r(base * 0.5) };
            }
            if is_focus {
                let colors: Vec<String> = palette.iter().take(3).cloned().collect();
                let colors = if l.gradient_points() >= 2 { colors } else { vec![palette[0].clone()] };
                return Assignment { target: name, colors, brightness: r(base * 1.25) };
            }
            if l.gradient_points() >= 2 {
                let k = n.min(4).max(2);
                let colors: Vec<String> = (0..k).map(|j| palette[(strip_i + j) % n].clone()).collect();
                strip_i += 1;
                let vary = [1.0, 0.85, 0.95, 0.75][strip_i % 4];
                Assignment { target: name, colors, brightness: r(base * vary) }
            } else {
                // accents first (skip the key colour when there are alternatives), fill every third bulb
                let idx = if n == 1 {
                    0
                } else if bulb_i % 3 == 2 {
                    n - 1
                } else {
                    1 + bulb_i % (n - 1)
                };
                bulb_i += 1;
                let vary = [0.9, 0.7, 0.6][bulb_i % 3];
                Assignment { target: name, colors: vec![palette[idx].clone()], brightness: r(base * vary) }
            }
        })
        .collect()
}

/// JSON schema the model must follow; light and colour names are enums so it can't invent either.
pub fn schema(targets: &[String]) -> Value {
    // plain colour words, resolved by colors::resolve
    let color = json!({ "type": "string", "minLength": 3, "maxLength": 24 });
    let mut focus: Vec<String> = vec!["none".into()];
    focus.extend(targets.iter().cloned());
    json!({
        "type": "object",
        "properties": {
            // free text first: the model describes the subject before it is constrained to colour names
            "evokes": { "type": "string", "minLength": 20, "maxLength": 240 },
            "palette": { "type": "array", "items": color, "minItems": 3, "maxItems": 6, "uniqueItems": true },
            "title": { "type": "string", "minLength": 3, "maxLength": 32 },
            "brightness": { "type": "integer", "minimum": 1, "maximum": 100 },
            "focus": { "type": "string", "enum": focus },
            "motion": { "type": "string", "enum": ["still", "breathe", "drift", "flicker"] },
            "speed": { "type": "string", "enum": ["slow", "medium", "fast"] }
        },
        "required": ["evokes", "palette", "title", "brightness", "focus", "motion", "speed"]
    })
}

/// Lights covered by `scope` (room/zone name) or every light.
pub fn scope_lights<'a>(inv: &'a Inventory, scope: Option<&str>) -> Vec<&'a Light> {
    match scope.and_then(|s| inv.find_target(s)) {
        Some(lumen_hue::api::Target::Group(g)) => inv.group_lights(g),
        Some(lumen_hue::api::Target::Light(l)) => vec![l],
        None => inv.lights.iter().collect(),
    }
}

/// Describes only the lights in `scope` (a room or zone name), or the whole home if None.
pub fn describe_scope(inv: &Inventory, scope: Option<&str>) -> (String, Vec<String>) {
    let Some(name) = scope else { return describe_home(inv) };
    let Some(lumen_hue::api::Target::Group(g)) = inv.find_target(name) else { return describe_home(inv) };
    let mut s = format!("Area \"{}\":\n", g.metadata.name);
    let mut targets = vec![g.metadata.name.clone()];
    for l in inv.group_lights(g) {
        s.push_str(&format!("  - light \"{}\" ({})\n", l.metadata.name, light_kind(l)));
        targets.push(l.metadata.name.clone());
    }
    targets.sort();
    targets.dedup();
    (s, targets)
}

fn light_kind(l: &Light) -> String {
    if !l.supports_color() {
        "white only".to_string()
    } else if l.gradient_points() >= 2 {
        format!("gradient strip, {} colour points", l.gradient_points())
    } else {
        "colour".to_string()
    }
}

/// Describes the home to the model: rooms, zones, lights and what each light can do.
pub fn describe_home(inv: &Inventory) -> (String, Vec<String>) {
    let mut targets = Vec::new();
    let mut s = String::new();
    let kind = |l: &Light| {
        if !l.supports_color() {
            "white only".to_string()
        } else if l.gradient_points() >= 2 {
            format!("gradient strip, {} colour points", l.gradient_points())
        } else {
            "colour".to_string()
        }
    };
    for r in &inv.rooms {
        s.push_str(&format!("Room \"{}\":\n", r.metadata.name));
        targets.push(r.metadata.name.clone());
        for l in inv.group_lights(r) {
            s.push_str(&format!("  - light \"{}\" ({})\n", l.metadata.name, kind(l)));
            targets.push(l.metadata.name.clone());
        }
    }
    for z in &inv.zones {
        let names: Vec<_> = inv.group_lights(z).iter().map(|l| format!("\"{}\"", l.metadata.name)).collect();
        s.push_str(&format!("Zone \"{}\": {}\n", z.metadata.name, names.join(", ")));
        targets.push(z.metadata.name.clone());
    }
    for l in &inv.lights {
        if !targets.contains(&l.metadata.name) {
            s.push_str(&format!("Light \"{}\" ({})\n", l.metadata.name, kind(l)));
            targets.push(l.metadata.name.clone());
        }
    }
    targets.sort();
    targets.dedup();
    (s, targets)
}
