//! CS2 events via Valve's Game State Integration: the game POSTs JSON to the localhost URL
//! registered in a `gamestate_integration_*.cfg` file.

use anyhow::{Context, Result};
use lumen_engine::Overlay;
use lumen_hue::Rgb;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const CFG_NAME: &str = "gamestate_integration_roksync.cfg";
/// Also counts as installed; removed when [`CFG_NAME`] is written.
pub const LEGACY_CFG_NAME: &str = "gamestate_integration_lumen.cfg";

pub fn cfg_installed(dir: &Path) -> bool {
    dir.join(CFG_NAME).exists() || dir.join(LEGACY_CFG_NAME).exists()
}
const CT_BLUE: Rgb = Rgb::new(0.25, 0.48, 1.0);
const T_GOLD: Rgb = Rgb::new(1.0, 0.62, 0.15);
const BOMB_FUSE_S: f32 = 40.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BombEnd {
    Exploded,
    Defused,
}

#[derive(Debug, Default)]
pub struct Cs2State {
    pub last_msg: Option<Instant>,
    pub map: String,
    pub map_phase: String,
    pub round_phase: String,
    pub team: String,
    pub health: i64,
    pub flashed: u8,
    pub burning: u8,
    pub spectating: bool,
    pub died_at: Option<Instant>,
    pub bomb_planted_at: Option<Instant>,
    pub bomb_end: Option<(BombEnd, Instant)>,
    pub round_end: Option<(String, Instant)>,
    pub last_event: String,
    pub test: Option<(Fx, Instant)>,
    /// Live events skip these; test plays still run.
    pub fx_off: std::collections::HashSet<Fx>,
    /// Per-effect strength 0..1 (missing = 1).
    pub fx_level: std::collections::HashMap<Fx, f32>,
    /// No full-room white; flashes become soft fades.
    pub calm: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fx {
    Flash,
    LowHp,
    Burning,
    BombPlanted,
    BombExploded,
    BombDefused,
    RoundWinCt,
    RoundWinT,
    Death,
}

impl Fx {
    pub const ALL: [Fx; 9] =
        [Fx::Flash, Fx::LowHp, Fx::Burning, Fx::BombPlanted, Fx::BombExploded, Fx::BombDefused, Fx::RoundWinCt, Fx::RoundWinT, Fx::Death];

    /// Stable key for settings files.
    pub fn id(self) -> &'static str {
        match self {
            Fx::Flash => "flash",
            Fx::LowHp => "lowhp",
            Fx::Burning => "burning",
            Fx::BombPlanted => "planted",
            Fx::BombExploded => "exploded",
            Fx::BombDefused => "defused",
            Fx::RoundWinCt => "win_ct",
            Fx::RoundWinT => "win_t",
            Fx::Death => "death",
        }
    }

    pub fn from_id(id: &str) -> Option<Fx> {
        Fx::ALL.iter().copied().find(|f| f.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Fx::Flash => "Flashbang",
            Fx::LowHp => "Low HP",
            Fx::Burning => "On fire",
            Fx::BombPlanted => "Bomb planted",
            Fx::BombExploded => "Bomb exploded",
            Fx::BombDefused => "Bomb defused",
            Fx::RoundWinCt => "Round win · CT",
            Fx::RoundWinT => "Round win · T",
            Fx::Death => "You died",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Fx::Flash => "Room whites out exactly as long as you're blind",
            Fx::LowHp => "Red heartbeat, faster and harder the lower you go",
            Fx::Burning => "Orange flicker while you stand in a molly",
            Fx::BombPlanted => "Red beacon that speeds up with the real beep",
            Fx::BombExploded => "White blast fading through orange",
            Fx::BombDefused => "Cool CT-blue wash",
            Fx::RoundWinCt => "Three soft pulses in CT blue",
            Fx::RoundWinT => "Three soft pulses in T gold",
            Fx::Death => "Colour drains from the room",
        }
    }

    pub fn color(self) -> Rgb {
        match self {
            Fx::Flash => Rgb::WHITE,
            Fx::LowHp | Fx::BombPlanted => Rgb::new(1.0, 0.0, 0.05),
            Fx::Burning => Rgb::new(1.0, 0.35, 0.0),
            Fx::BombExploded => Rgb::new(1.0, 0.55, 0.1),
            Fx::BombDefused | Fx::RoundWinCt => CT_BLUE,
            Fx::RoundWinT => T_GOLD,
            Fx::Death => Rgb::new(0.45, 0.45, 0.5),
        }
    }

    /// One play of the effect, in seconds.
    pub fn duration(self) -> f32 {
        match self {
            Fx::Flash => 2.6,
            Fx::LowHp => 4.0,
            Fx::Burning => 2.5,
            Fx::BombPlanted => 5.0,
            Fx::BombExploded => 3.0,
            Fx::BombDefused => 2.5,
            Fx::RoundWinCt | Fx::RoundWinT => 3.6,
            Fx::Death => 3.0,
        }
    }

    /// A game state that shows this effect `t` seconds into it (the bomb plays as a 40 s time-lapse).
    pub fn demo_state(self, t: f32, now: Instant) -> Cs2State {
        let ago = |s: f32| now.checked_sub(Duration::from_secs_f32(s.max(0.0))).unwrap_or(now);
        let mut st = Cs2State { last_msg: Some(now), health: 100, ..Default::default() };
        match self {
            Fx::Flash => st.flashed = (255.0 * (1.0 - t / self.duration()).clamp(0.0, 1.0).powf(0.7)) as u8,
            Fx::LowHp => st.health = 14,
            Fx::Burning => st.burning = 220,
            Fx::BombPlanted => st.bomb_planted_at = Some(ago(t / self.duration() * BOMB_FUSE_S)),
            Fx::BombExploded => st.bomb_end = Some((BombEnd::Exploded, ago(t))),
            Fx::BombDefused => st.bomb_end = Some((BombEnd::Defused, ago(t))),
            Fx::RoundWinCt => st.round_end = Some(("CT".into(), ago(t))),
            Fx::RoundWinT => st.round_end = Some(("T".into(), ago(t))),
            Fx::Death => {
                st.health = 0;
                st.died_at = Some(ago(t));
            }
        }
        st
    }
}

pub type SharedCs2 = Arc<Mutex<Cs2State>>;

impl Cs2State {
    pub fn connected(&self, now: Instant) -> bool {
        self.last_msg.map_or(false, |t| now.duration_since(t) < Duration::from_secs(35))
    }

    pub fn ingest(&mut self, v: &Value, now: Instant) {
        self.last_msg = Some(now);
        let s = |v: &Value, path: &[&str]| -> Option<String> {
            let mut cur = v;
            for p in path {
                cur = cur.get(*p)?;
            }
            cur.as_str().map(str::to_string)
        };
        let n = |v: &Value, path: &[&str]| -> Option<i64> {
            let mut cur = v;
            for p in path {
                cur = cur.get(*p)?;
            }
            cur.as_i64()
        };

        self.map = s(v, &["map", "name"]).unwrap_or_default();
        self.map_phase = s(v, &["map", "phase"]).unwrap_or_default();
        let phase = s(v, &["round", "phase"]).unwrap_or_default();

        // provider = you; player = whoever you're watching (yourself while alive)
        let me = s(v, &["provider", "steamid"]);
        let watched = s(v, &["player", "steamid"]);
        self.spectating = me.is_some() && watched.is_some() && me != watched;

        if !self.spectating {
            if let Some(team) = s(v, &["player", "team"]) {
                self.team = team;
            }
            let hp = n(v, &["player", "state", "health"]).unwrap_or(self.health);
            if self.health > 0 && hp == 0 && phase != "freezetime" {
                self.died_at = Some(now);
                self.last_event = "you died".into();
            }
            if hp > 0 {
                self.died_at = None;
            }
            self.health = hp;
            self.flashed = n(v, &["player", "state", "flashed"]).unwrap_or(0).clamp(0, 255) as u8;
            self.burning = n(v, &["player", "state", "burning"]).unwrap_or(0).clamp(0, 255) as u8;
        } else {
            self.flashed = 0;
            self.burning = 0;
        }

        match s(v, &["round", "bomb"]).as_deref() {
            Some("planted") => {
                if self.bomb_planted_at.is_none() {
                    self.bomb_planted_at = Some(now);
                    self.last_event = "bomb planted".into();
                }
            }
            Some("exploded") => {
                if self.bomb_planted_at.take().is_some() || self.bomb_end.is_none() {
                    self.bomb_end = Some((BombEnd::Exploded, now));
                    self.last_event = "bomb exploded".into();
                }
            }
            Some("defused") => {
                if self.bomb_planted_at.take().is_some() || self.bomb_end.is_none() {
                    self.bomb_end = Some((BombEnd::Defused, now));
                    self.last_event = "bomb defused".into();
                }
            }
            _ => {}
        }

        if phase == "over" && self.round_phase != "over" {
            if let Some(win) = s(v, &["round", "win_team"]) {
                self.last_event = format!("round won by {win}");
                self.round_end = Some((win, now));
            }
        }
        if phase == "freezetime" && self.round_phase != "freezetime" {
            self.bomb_planted_at = None;
            self.bomb_end = None;
            self.died_at = None;
        }
        self.round_phase = phase;
    }

    /// Plays once, in place of live game events, even with no game connected.
    pub fn fire_test(&mut self, fx: Fx, now: Instant) {
        self.test = Some((fx, now));
        self.last_event = format!("test: {}", fx.label().to_lowercase());
    }

    /// Later overlays win. Applies per-effect switches, strengths and `calm`.
    pub fn overlays(&self, now: Instant) -> Vec<Overlay> {
        if let Some((fx, t0)) = self.test {
            let t = now.duration_since(t0).as_secs_f32();
            if t < fx.duration() {
                let k = self.level(fx);
                return fx.demo_state(t, now).tagged(now).into_iter().map(|(f, o)| self.shape(f, o, k)).collect();
            }
        }
        self.tagged(now)
            .into_iter()
            .filter(|(f, _)| !self.fx_off.contains(f))
            .map(|(f, o)| self.shape(f, o, self.level(f)))
            .collect()
    }

    pub fn level(&self, fx: Fx) -> f32 {
        self.fx_level.get(&fx).copied().unwrap_or(1.0).clamp(0.0, 1.0)
    }

    fn shape(&self, fx: Fx, o: Overlay, k: f32) -> Overlay {
        let calm = self.calm && matches!(fx, Fx::Flash | Fx::BombExploded);
        match o {
            Overlay::Tint { color, alpha } if calm => Overlay::Tint { color: color.lerp(Rgb::new(0.79, 0.78, 0.75), 0.6), alpha: alpha * k * 0.45 },
            Overlay::Tint { color, alpha } => Overlay::Tint { color, alpha: alpha * k },
            Overlay::Desaturate(a) => Overlay::Desaturate(a * k),
            Overlay::Dim(a) => Overlay::Dim(a * k),
        }
    }

    /// Each overlay tagged with the effect that made it. Later ones win.
    fn tagged(&self, now: Instant) -> Vec<(Fx, Overlay)> {
        let mut out = Vec::new();
        if !self.connected(now) {
            return out;
        }
        let since = |t: Instant| now.duration_since(t).as_secs_f32();

        if let Some(t) = self.died_at {
            let a = (since(t) / 0.8).min(1.0);
            out.push((Fx::Death, Overlay::Desaturate(0.85 * a)));
            out.push((Fx::Death, Overlay::Dim(0.55 * a)));
        }

        if !self.spectating && self.health > 0 && self.health <= 40 {
            let sev = (40 - self.health) as f32 / 40.0;
            let hz = 0.9 + 1.6 * sev;
            let beat = heartbeat((now_secs(now) * hz).fract());
            out.push((Fx::LowHp, Overlay::Tint { color: Rgb::new(1.0, 0.0, 0.02), alpha: 0.2 + 0.55 * beat * (0.5 + 0.5 * sev) }));
        }

        if self.burning > 0 {
            let fl = 0.75 + 0.25 * (now_secs(now) * 23.0).sin().abs();
            out.push((Fx::Burning, Overlay::Tint { color: Rgb::new(1.0, 0.35, 0.0), alpha: self.burning as f32 / 255.0 * 0.8 * fl }));
        }

        // the bomb's beep interval shortens over the 40 s fuse
        if let Some(t) = self.bomb_planted_at {
            let e = since(t).min(BOMB_FUSE_S);
            let interval = (1.0 - 0.88 * e / BOMB_FUSE_S).max(0.12);
            let ph = (e % interval) / interval;
            let pulse = (-ph * 9.0).exp();
            out.push((Fx::BombPlanted, Overlay::Tint { color: Rgb::new(1.0, 0.0, 0.0), alpha: 0.12 + 0.6 * pulse }));
        }

        if let Some((kind, t)) = self.bomb_end {
            let e = since(t);
            match kind {
                BombEnd::Exploded if e < 3.0 => {
                    let a = (1.0 - e / 3.0).powf(1.5);
                    let col = Rgb::WHITE.lerp(Rgb::new(1.0, 0.4, 0.0), (e / 0.6).min(1.0));
                    out.push((Fx::BombExploded, Overlay::Tint { color: col, alpha: a }));
                }
                BombEnd::Defused if e < 2.5 => {
                    out.push((Fx::BombDefused, Overlay::Tint { color: CT_BLUE, alpha: 0.8 * (1.0 - e / 2.5) }));
                }
                _ => {}
            }
        }

        if let Some((team, t)) = &self.round_end {
            let e = since(*t);
            if e < 3.6 {
                let col = if team.eq_ignore_ascii_case("CT") { CT_BLUE } else { T_GOLD };
                let pulse = 0.5 - 0.5 * (e / 1.2 * std::f32::consts::TAU).cos();
                out.push((if team.eq_ignore_ascii_case("CT") { Fx::RoundWinCt } else { Fx::RoundWinT }, Overlay::Tint { color: col, alpha: 0.75 * pulse * (1.0 - e / 3.6).max(0.0).sqrt() }));
            }
        }

        // flashbang last so it wins
        if self.flashed > 0 {
            out.push((Fx::Flash, Overlay::Tint { color: Rgb::WHITE, alpha: self.flashed as f32 / 255.0 }));
        }
        out
    }
}

fn now_secs(now: Instant) -> f32 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    now.duration_since(*START.get_or_init(Instant::now)).as_secs_f32()
}

/// Two-bump "lub-dub" shape over one beat, 0..1
fn heartbeat(p: f32) -> f32 {
    let bump = |c: f32, w: f32| (-((p - c) / w).powi(2)).exp();
    (bump(0.08, 0.05) + 0.6 * bump(0.28, 0.06)).min(1.0)
}

pub fn new_token() -> String {
    let mut b = [0u8; 16];
    let _ = getrandom::getrandom(&mut b);
    hex::encode(b)
}

/// Finds `<CS2>\game\csgo\cfg` across all Steam libraries.
pub fn find_cfg_dir() -> Option<PathBuf> {
    let mut libs = vec![PathBuf::from(r"C:\Program Files (x86)\Steam")];
    if let Ok(vdf) = std::fs::read_to_string(r"C:\Program Files (x86)\Steam\steamapps\libraryfolders.vdf") {
        for line in vdf.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("\"path\"") {
                let p = rest.trim().trim_matches('"').replace("\\\\", "\\");
                libs.push(PathBuf::from(p));
            }
        }
    }
    libs.into_iter()
        .map(|l| l.join(r"steamapps\common\Counter-Strike Global Offensive\game\csgo\cfg"))
        .find(|p| p.is_dir())
}

/// Writes the GSI cfg. CS2 reads it only at launch.
pub fn install_cfg(dir: &Path, port: u16, token: &str) -> Result<PathBuf> {
    let path = dir.join(CFG_NAME);
    let body = format!(
        r#""RokSync lights"
{{
  "uri"       "http://127.0.0.1:{port}/cs2"
  "timeout"   "1.0"
  "buffer"    "0.0"
  "throttle"  "0.0"
  "heartbeat" "10.0"
  "auth"
  {{
    "token" "{token}"
  }}
  "data"
  {{
    "provider"     "1"
    "map"          "1"
    "round"        "1"
    "player_id"    "1"
    "player_state" "1"
  }}
}}
"#
    );
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    // one cfg only, or CS2 would send every event twice
    let _ = std::fs::remove_file(dir.join(LEGACY_CFG_NAME));
    Ok(path)
}

/// Listens on 127.0.0.1 only. Requests without the matching auth token are ignored.
pub fn spawn_server(port: u16, token: String, state: SharedCs2) -> Result<()> {
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .map_err(|e| anyhow::anyhow!("can't listen on 127.0.0.1:{port}: {e}"))?;
    std::thread::Builder::new().name("cs2-gsi".into()).spawn(move || {
        for mut req in server.incoming_requests() {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            if let Ok(v) = serde_json::from_str::<Value>(&body) {
                let ok = v.get("auth").and_then(|a| a.get("token")).and_then(|t| t.as_str()) == Some(token.as_str());
                if ok {
                    state.lock().unwrap().ingest(&v, Instant::now());
                }
            }
            let _ = req.respond(tiny_http::Response::empty(200));
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_plant_flash_and_death() {
        let mut s = Cs2State { health: 100, ..Default::default() };
        let t0 = Instant::now();
        let base = |hp: i64, flashed: i64, bomb: Option<&str>| {
            let mut v = json!({
                "provider": {"steamid": "1"},
                "player": {"steamid": "1", "team": "CT", "state": {"health": hp, "flashed": flashed, "burning": 0}},
                "round": {"phase": "live"},
                "map": {"name": "de_dust2", "phase": "live"}
            });
            if let Some(b) = bomb {
                v["round"]["bomb"] = json!(b);
            }
            v
        };
        s.ingest(&base(100, 200, None), t0);
        assert!(matches!(s.overlays(t0).last(), Some(Overlay::Tint { alpha, .. }) if *alpha > 0.7));
        s.ingest(&base(30, 0, Some("planted")), t0);
        assert!(s.bomb_planted_at.is_some());
        s.ingest(&base(0, 0, Some("planted")), t0);
        assert!(s.died_at.is_some());
    }

    #[test]
    fn every_fx_demo_produces_light() {
        let now = Instant::now();
        for fx in Fx::ALL {
            let t = fx.duration() * 0.3;
            assert!(!fx.demo_state(t, now).overlays(now).is_empty(), "{fx:?} shows nothing");
        }
        let mut s = Cs2State::default();
        assert!(s.overlays(now).is_empty());
        s.fire_test(Fx::Flash, now);
        assert!(!s.overlays(now).is_empty());
        assert!(s.overlays(now + Duration::from_secs(10)).is_empty());
    }

    #[test]
    fn user_switches_and_strength_shape_effects() {
        let now = Instant::now();
        let alpha = |o: &[Overlay]| match o.last() {
            Some(Overlay::Tint { alpha, .. }) => *alpha,
            _ => 0.0,
        };
        let mut s = Cs2State { last_msg: Some(now), health: 100, flashed: 255, ..Default::default() };
        let full = alpha(&s.overlays(now));
        assert!(full > 0.9);
        s.fx_level.insert(Fx::Flash, 0.5);
        assert!((alpha(&s.overlays(now)) - full * 0.5).abs() < 1e-4);
        s.calm = true;
        assert!(alpha(&s.overlays(now)) < full * 0.5);
        s.fx_off.insert(Fx::Flash);
        assert!(s.overlays(now).is_empty());
        s.fire_test(Fx::Flash, now);
        assert!(!s.overlays(now).is_empty());
        assert!(Fx::ALL.iter().all(|f| Fx::from_id(f.id()) == Some(*f)));
    }
}
