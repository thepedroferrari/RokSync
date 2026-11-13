//! Dota 2 events via Valve's Game State Integration (GSI): the game POSTs JSON to the localhost
//! URL registered in a cfg file.
//!
//! One-time setup:
//! 1. [`install_cfg`] into [`cfg_dir`] (`...\dota 2 beta\game\dota\cfg\gamestate_integration`).
//! 2. Add the Steam launch option [`LAUNCH_OPTION`] (`-gamestateintegration`) to Dota 2. Dota
//!    only reads GSI configs with that flag; [`launch_option_set`] checks for it.
//! 3. Restart Dota (it reads the cfg at launch).
//!
//! A playing client only gets data about itself; spectators get `player`/`hero` keyed by
//! `team2`/`team3`, which is detected and used only for the shared map effects.
//!
//! Previous values are tracked here rather than read from `previously`/`added`: those only list
//! fields that changed, and are lost with dropped packets.

use anyhow::{Context, Result};
use lumen_engine::Overlay;
use lumen_hue::Rgb;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::f32::consts::{PI, TAU};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const CFG_NAME: &str = "gamestate_integration_roksync.cfg";
/// Also counts as installed; removed when [`CFG_NAME`] is written.
pub const LEGACY_CFG_NAME: &str = "gamestate_integration_lumen.cfg";
pub const DEFAULT_PORT: u16 = 3122;
/// Steam launch option Dota needs before it will send GSI data at all.
pub const LAUNCH_OPTION: &str = "-gamestateintegration";
const DOTA_APP_ID: &str = "570";

/// Dota counts a kill as part of a multi-kill if it lands within 18 s of the previous one.
const MULTIKILL_WINDOW: Duration = Duration::from_secs(18);
/// Roshan respawns 8 to 11 minutes after he dies; the window opens at 8.
const ROSHAN_MIN_RESPAWN_S: i64 = 8 * 60;
/// Buyback puts a long cooldown on; a jump bigger than this can only be a buyback.
const BUYBACK_CD_JUMP_S: i64 = 60;

const fn rgb_hex(c: u32) -> Rgb {
    Rgb::new(((c >> 16) & 0xff) as f32 / 255.0, ((c >> 8) & 0xff) as f32 / 255.0, (c & 0xff) as f32 / 255.0)
}

const RADIANT: Rgb = rgb_hex(0x3FA34D);
const DIRE: Rgb = rgb_hex(0x8A1C2B);
const DAY_WARM: Rgb = rgb_hex(0xE0A43A);
const NIGHT_DEEP: Rgb = rgb_hex(0x1E3A8A);
const NIGHT_BRIGHT: Rgb = rgb_hex(0x2D5BFF);
const NIGHTSTALKER_DARK: Rgb = rgb_hex(0x0A1238);
const WARM_GREY: Rgb = Rgb::new(0.79, 0.78, 0.75);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Team {
    Radiant,
    Dire,
}

impl Team {
    /// GSI sends `"radiant"`, `"dire"` (and `"none"` for no winner yet).
    pub fn parse(s: &str) -> Option<Team> {
        match s.to_ascii_lowercase().as_str() {
            "radiant" => Some(Team::Radiant),
            "dire" => Some(Team::Dire),
            _ => None,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Team::Radiant => "radiant",
            Team::Dire => "dire",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Team::Radiant => "Radiant",
            Team::Dire => "Dire",
        }
    }

    pub fn color(self) -> Rgb {
        match self {
            Team::Radiant => RADIANT,
            Team::Dire => DIRE,
        }
    }

    fn bright(self) -> Rgb {
        match self {
            Team::Radiant => rgb_hex(0x5BD46A),
            Team::Dire => rgb_hex(0xE0303F),
        }
    }

    fn victory(self) -> Rgb {
        match self {
            Team::Radiant => rgb_hex(0x7CFF6B),
            Team::Dire => rgb_hex(0xFF4B5C),
        }
    }
}

/// Coarse match phase parsed from `map.game_state` (`DOTA_GAMERULES_STATE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// No `map` in the payload: main menu, or between matches.
    #[default]
    Menu,
    /// INIT, WAIT_FOR_PLAYERS_TO_LOAD, WAIT_FOR_MAP_TO_LOAD, CUSTOM_GAME_SETUP.
    Loading,
    /// HERO_SELECTION.
    Draft,
    /// STRATEGY_TIME and TEAM_SHOWCASE.
    Strategy,
    /// PRE_GAME: heroes on the map, clock counting up to 0:00 (the horn).
    PreGame,
    /// GAME_IN_PROGRESS.
    Playing,
    /// POST_GAME (and DISCONNECT / LAST, which follow it).
    PostGame,
}

impl Phase {
    pub fn parse(game_state: &str) -> Phase {
        match game_state.trim().trim_start_matches("DOTA_GAMERULES_STATE_") {
            "HERO_SELECTION" => Phase::Draft,
            "STRATEGY_TIME" | "TEAM_SHOWCASE" => Phase::Strategy,
            "PRE_GAME" => Phase::PreGame,
            "GAME_IN_PROGRESS" => Phase::Playing,
            "POST_GAME" | "DISCONNECT" | "LAST" => Phase::PostGame,
            "" => Phase::Menu,
            _ => Phase::Loading,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Base {
    #[default]
    Cycle,
    Faction,
    /// Screen sync supplies the base; only events are layered on top.
    Screen,
}

impl Base {
    pub const ALL: [Base; 3] = [Base::Cycle, Base::Faction, Base::Screen];

    pub fn id(self) -> &'static str {
        match self {
            Base::Cycle => "cycle",
            Base::Faction => "faction",
            Base::Screen => "screen",
        }
    }

    pub fn from_id(id: &str) -> Option<Base> {
        Base::ALL.iter().copied().find(|b| b.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Base::Cycle => "Day / night",
            Base::Faction => "Faction",
            Base::Screen => "Screen sync",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    MatchFlow,
    Hero,
    Fights,
    Map,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::MatchFlow, Group::Hero, Group::Fights, Group::Map];

    pub fn label(self) -> &'static str {
        match self {
            Group::MatchFlow => "Match flow",
            Group::Hero => "Your hero",
            Group::Fights => "Fights",
            Group::Map => "Map",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Group::MatchFlow => "the match itself",
            Group::Hero => "what happens to your hero",
            Group::Fights => "kills and streaks",
            Group::Map => "Roshan and buildings",
        }
    }

    pub fn fx(self) -> impl Iterator<Item = Fx> {
        Fx::ALL.into_iter().filter(move |f| f.group() == self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fx {
    Draft,
    Horn,
    DayNight,
    Victory,
    Defeat,
    LowHp,
    Respawn,
    Buyback,
    Ult,
    Spike,
    Aghs,
    Stunned,
    Silenced,
    Hexed,
    Kill,
    MultiKill,
    Streak,
    FirstBlood,
    Roshan,
    RoshWin,
    Buildings,
}

struct Spec {
    id: &'static str,
    label: &'static str,
    blurb: &'static str,
    color: Rgb,
    level: u8,
    group: Group,
    verified: bool,
    duration: f32,
}

impl Fx {
    pub const ALL: [Fx; 21] = [
        Fx::Draft,
        Fx::Horn,
        Fx::DayNight,
        Fx::Victory,
        Fx::Defeat,
        Fx::LowHp,
        Fx::Respawn,
        Fx::Buyback,
        Fx::Ult,
        Fx::Spike,
        Fx::Aghs,
        Fx::Stunned,
        Fx::Silenced,
        Fx::Hexed,
        Fx::Kill,
        Fx::MultiKill,
        Fx::Streak,
        Fx::FirstBlood,
        Fx::Roshan,
        Fx::RoshWin,
        Fx::Buildings,
    ];

    fn spec(self) -> Spec {
        use Group::*;
        let s = |id, label, blurb, c, level, group, verified, duration| Spec {
            id,
            label,
            blurb,
            color: rgb_hex(c),
            level,
            group,
            verified,
            duration,
        };
        match self {
            // map.game_state == HERO_SELECTION, player.team_name.
            Fx::Draft => s("draft", "Draft", "Calm Radiant green or Dire red while heroes are picked", 0x3FA34D, 2, MatchFlow, true, 6.0),
            // map.game_state PRE_GAME -> GAME_IN_PROGRESS, map.clock_time counting up to 0.
            Fx::Horn => s("horn", "Strategy time to horn", "Builds over the last 10 s, hits on the horn", 0xF3F1EA, 3, MatchFlow, true, 4.0),
            // map.daytime, map.nightstalker_night.
            Fx::DayNight => s("daynight", "Day and night", "Warm by day, deep blue by night. Snaps dark when Night Stalker forces night", 0x2D5BFF, 3, MatchFlow, true, 4.0),
            // map.win_team vs player.team_name.
            Fx::Victory => s("victory", "Victory", "Long sweep in your faction colour", 0x7CFF6B, 4, MatchFlow, true, 8.0),
            Fx::Defeat => s("defeat", "Defeat", "Slow fade to dim", 0x8A1C2B, 2, MatchFlow, true, 7.0),
            // hero.health_percent.
            Fx::LowHp => s("lowhp", "Low health", "Red pulse under 25 %, faster near zero", 0xD7263D, 3, Hero, true, 4.0),
            // hero.alive, hero.respawn_seconds.
            Fx::Respawn => s("respawn", "Death and respawn", "Colour drains while dead, refills over the last 5 s", 0x3A3A40, 3, Hero, true, 6.0),
            // hero.buyback_cooldown jump / player.gold_spent_on_buybacks / early revive (see ingest).
            Fx::Buyback => s("buyback", "Buyback", "Gold flash the moment you buy back", 0xFFD400, 4, Hero, true, 2.0),
            // abilities.abilityN.{ultimate, level, cooldown}.
            Fx::Ult => s("ult", "Ultimate ready", "A short tick in your hero colour when it comes off cooldown", 0x4FD1FF, 2, Hero, true, 1.2),
            // hero.level.
            Fx::Spike => s("spike", "Level 6, 12 and 18", "Power-spike flash, bigger each time", 0xFF8A00, 3, Hero, true, 2.2),
            // hero.aghanims_scepter, hero.aghanims_shard.
            Fx::Aghs => s("aghs", "Aghanim's", "Blue shimmer when the scepter or shard lands", 0x2D7DFF, 2, Hero, true, 2.5),
            // hero.stunned / hero.silenced / hero.hexed.
            Fx::Stunned => s("stunned", "Stunned", "White freeze for as long as the stun lasts", 0xFFFFFF, 3, Hero, true, 2.0),
            Fx::Silenced => s("silenced", "Silenced", "Purple dim while you cannot cast", 0x7B2FF7, 2, Hero, true, 3.0),
            Fx::Hexed => s("hexed", "Hexed", "Green wobble while you are a critter", 0x00E676, 2, Hero, true, 3.0),
            // player.kills, player.kill_streak.
            Fx::Kill => s("kill", "Kill", "A quick tick in your faction colour", 0xF3F1EA, 2, Fights, true, 1.0),
            Fx::MultiKill => s("multikill", "Double kill to rampage", "Each one bigger. Rampage gets a full sweep", 0xFF2BD6, 4, Fights, true, 3.5),
            Fx::Streak => s("streak", "Killing spree to beyond godlike", "Slow glow that builds with the streak", 0xFF5A1F, 3, Fights, true, 5.0),
            // No first-blood flag in GSI: inferred from map.radiant_score + map.dire_score leaving 0.
            Fx::FirstBlood => s("firstblood", "First blood", "One sharp red hit", 0xD7263D, 3, Fights, false, 1.5),
            // `events` array (`event_type: "roshan_killed"`) or map.roshan_state.
            Fx::Roshan => s("roshan", "Roshan killed", "Slow, heavy pulse", 0xB0412A, 4, Map, false, 4.5),
            // Roshan kill + 8 min, or map.roshan_state turning "respawn_variable".
            Fx::RoshWin => s("roshwin", "Roshan respawn window", "Gentle reminder when he might be back", 0xE0A43A, 2, Map, false, 4.0),
            // buildings.<team>.<name>.health.
            Fx::Buildings => s("buildings", "Base under attack", "Amber flicker at the edges when your tier 3s or Ancient take damage", 0xFFB000, 3, Map, false, 3.0),
        }
    }

    /// Stable key for settings files.
    pub fn id(self) -> &'static str {
        self.spec().id
    }

    pub fn from_id(id: &str) -> Option<Fx> {
        Fx::ALL.iter().copied().find(|f| f.id() == id)
    }

    pub fn label(self) -> &'static str {
        self.spec().label
    }

    pub fn blurb(self) -> &'static str {
        self.spec().blurb
    }

    pub fn color(self) -> Rgb {
        self.spec().color
    }

    pub fn group(self) -> Group {
        self.spec().group
    }

    /// Default strength on a 1..=5 scale.
    pub fn default_level(self) -> u8 {
        self.spec().level
    }

    pub fn default_on(self) -> bool {
        !matches!(self, Fx::Aghs | Fx::Silenced)
    }

    pub fn verified(self) -> bool {
        self.spec().verified
    }

    /// One play of the effect, in seconds.
    pub fn duration(self) -> f32 {
        self.spec().duration
    }

    /// Sharp or bright effects; `calm` turns them into soft fades.
    fn flashy(self) -> bool {
        matches!(self, Fx::Horn | Fx::Buyback | Fx::Spike | Fx::Stunned | Fx::Kill | Fx::MultiKill | Fx::FirstBlood)
    }

    /// A game state that shows this effect `t` seconds into it.
    pub fn demo_state(self, t: f32, now: Instant) -> DotaState {
        let t = t.max(0.0);
        let ago = |s: f32| now.checked_sub(Duration::from_secs_f32(s.max(0.0))).unwrap_or(now);
        let mut st = DotaState {
            last_msg: Some(now),
            phase: Phase::Playing,
            game_state: "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS".into(),
            daytime: true,
            team: Some(Team::Radiant),
            hero_level: 10,
            alive: true,
            health_percent: 100,
            ..Default::default()
        };
        match self {
            Fx::Draft => {
                st.phase = Phase::Draft;
                st.game_state = "DOTA_GAMERULES_STATE_HERO_SELECTION".into();
            }
            Fx::Horn => {
                // 10 s of build-up squeezed into 2.5 s, then the horn.
                if t < 2.5 {
                    let clock = -10.0 + t * 4.0;
                    st.phase = Phase::PreGame;
                    st.clock_time = clock.floor() as i64;
                    st.clock_at = Some(ago(clock - clock.floor()));
                } else {
                    st.horn_at = Some(ago(t - 2.5));
                }
            }
            Fx::DayNight => {
                st.daytime = false;
                st.daynight_at = Some(ago(t));
            }
            Fx::Victory => st.result = Some((true, ago(t))),
            Fx::Defeat => st.result = Some((false, ago(t))),
            Fx::LowHp => st.health_percent = 12,
            Fx::Respawn => {
                let left = (self.duration() - t).max(0.0);
                st.alive = false;
                st.died_at = Some(ago(t));
                st.respawn_seconds = left.ceil() as i64;
                st.respawn_at = Some(ago(left.ceil() - left));
            }
            Fx::Buyback => st.buyback_at = Some(ago(t)),
            Fx::Ult => st.ult_at = Some(ago(t)),
            Fx::Spike => st.spike = Some((12, ago(t))),
            Fx::Aghs => st.aghs_at = Some(ago(t)),
            Fx::Stunned => st.stunned = Some(ago(t)),
            Fx::Silenced => st.silenced = Some(ago(t)),
            Fx::Hexed => st.hexed = Some(ago(t)),
            Fx::Kill => st.kill_at = Some(ago(t)),
            Fx::MultiKill => st.multi = Some((5, ago(t))),
            Fx::Streak => st.streak = Some((5, ago(t))),
            Fx::FirstBlood => st.firstblood_at = Some(ago(t)),
            Fx::Roshan => st.roshan_at = Some(ago(t)),
            Fx::RoshWin => st.roshwin_at = Some(ago(t)),
            Fx::Buildings => st.base_hit_at = Some(ago(t % 1.0)),
        }
        st
    }
}

#[derive(Debug, Default)]
pub struct DotaState {
    pub last_msg: Option<Instant>,
    /// `map.matchid`; a new id resets the per-match state.
    pub match_id: String,
    /// Raw `map.game_state`, e.g. `DOTA_GAMERULES_STATE_GAME_IN_PROGRESS`.
    pub game_state: String,
    pub phase: Phase,
    /// `map.clock_time`: the in-game clock, negative before the horn.
    pub clock_time: i64,
    /// When `clock_time` last changed, to interpolate between whole seconds.
    pub clock_at: Option<Instant>,
    /// `map.game_time`: seconds since the game loaded (event timestamps use this).
    pub game_time: i64,
    pub paused: bool,
    pub daytime: bool,
    pub nightstalker_night: bool,
    pub daynight_at: Option<Instant>,
    pub nightstalker_at: Option<Instant>,
    pub team: Option<Team>,
    pub win_team: Option<Team>,
    /// `map.radiant_score` / `map.dire_score` (hero kills), when the game sends them.
    pub score: Option<(i64, i64)>,
    /// `map.roshan_state`: "alive", "respawn_base", "respawn_variable", when sent.
    pub roshan_state: String,
    /// Watching someone else (or a replay): only shared map effects play.
    pub spectating: bool,

    pub hero_level: i64,
    pub alive: bool,
    pub respawn_seconds: i64,
    pub respawn_at: Option<Instant>,
    pub health_percent: i64,
    pub stunned: Option<Instant>,
    pub silenced: Option<Instant>,
    pub hexed: Option<Instant>,
    pub aghanims_scepter: bool,
    pub aghanims_shard: bool,
    pub buyback_cooldown: i64,
    pub gold_spent_on_buybacks: Option<i64>,
    /// The learned ultimate we follow (`abilities.abilityN` with `ultimate: true`).
    pub ult_name: String,
    pub ult_cooldown: i64,
    pub ult_can_cast: bool,
    pub kills: i64,
    pub deaths: i64,
    pub kill_streak: i64,
    pub multi_count: u8,
    pub last_kill_at: Option<Instant>,

    pub horn_at: Option<Instant>,
    /// Whether your team won, and when the result arrived.
    pub result: Option<(bool, Instant)>,
    pub died_at: Option<Instant>,
    pub buyback_at: Option<Instant>,
    pub ult_at: Option<Instant>,
    /// Level reached (6, 12 or 18) and when.
    pub spike: Option<(i64, Instant)>,
    pub aghs_at: Option<Instant>,
    pub kill_at: Option<Instant>,
    /// Multi-kill size (2 = double … 5 = rampage) and when.
    pub multi: Option<(u8, Instant)>,
    /// Kill streak reached (3..=10) and when.
    pub streak: Option<(i64, Instant)>,
    pub firstblood_at: Option<Instant>,
    pub roshan_at: Option<Instant>,
    /// `game_time` of the last Roshan kill, to dedupe and time the respawn window.
    pub roshan_kill_gt: Option<i64>,
    pub roshwin_due_gt: Option<i64>,
    pub roshwin_at: Option<Instant>,
    /// Last time one of your tier 3s, tier 4s, barracks or the Ancient lost health.
    pub base_hit_at: Option<Instant>,
    building_hp: HashMap<String, i64>,
    /// GAME_IN_PROGRESS was seen this match, so a result is a real ending, not a replay.
    played: bool,
    /// At least one payload of this match has been read, so edges are real changes.
    primed: bool,

    pub last_event: String,
    pub test: Option<(Fx, Instant)>,
    /// Live events skip these; test plays still run.
    pub fx_off: HashSet<Fx>,
    /// Per-effect strength 0..1 (missing = `default_level() / 5`).
    pub fx_level: HashMap<Fx, f32>,
    /// No full-room white or strobes; sharp hits become soft fades.
    pub calm: bool,
    pub base: Base,
}

pub type SharedDota = Arc<Mutex<DotaState>>;

fn num(v: &Value, k: &str) -> Option<i64> {
    let x = v.get(k)?;
    x.as_i64().or_else(|| x.as_f64().map(|f| f as i64)).or_else(|| x.as_str()?.trim().parse().ok())
}

fn flag(v: &Value, k: &str) -> Option<bool> {
    v.get(k)?.as_bool()
}

fn text<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k)?.as_str()
}

fn multikill_name(n: u8) -> &'static str {
    match n {
        0 | 1 => "Kill",
        2 => "Double kill",
        3 => "Triple kill",
        4 => "Ultra kill",
        _ => "Rampage",
    }
}

fn streak_name(n: i64) -> &'static str {
    match n {
        ..=3 => "Killing spree",
        4 => "Dominating",
        5 => "Mega kill",
        6 => "Unstoppable",
        7 => "Wicked sick",
        8 => "Monster kill",
        9 => "Godlike",
        _ => "Beyond godlike",
    }
}

impl DotaState {
    pub fn with_defaults() -> Self {
        DotaState { fx_off: Fx::ALL.into_iter().filter(|f| !f.default_on()).collect(), ..Default::default() }
    }

    pub fn connected(&self, now: Instant) -> bool {
        self.last_msg.is_some_and(|t| now.saturating_duration_since(t) < Duration::from_secs(35))
    }

    pub fn in_game(&self) -> bool {
        matches!(self.phase, Phase::PreGame | Phase::Playing)
    }

    /// The in-game clock, interpolated between the whole seconds GSI sends.
    pub fn clock(&self, now: Instant) -> f32 {
        let drift = match (self.paused, self.clock_at) {
            (false, Some(t)) => now.saturating_duration_since(t).as_secs_f32().min(1.0),
            _ => 0.0,
        };
        self.clock_time as f32 + drift
    }

    pub fn respawn_remaining(&self, now: Instant) -> f32 {
        let drift = self.respawn_at.map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f32().min(1.0));
        (self.respawn_seconds as f32 - drift).max(0.0)
    }

    fn note(&mut self, s: impl Into<String>) {
        self.last_event = s.into();
    }

    fn reset_match(&mut self) {
        *self = DotaState {
            last_msg: self.last_msg,
            last_event: std::mem::take(&mut self.last_event),
            test: self.test,
            fx_off: std::mem::take(&mut self.fx_off),
            fx_level: std::mem::take(&mut self.fx_level),
            calm: self.calm,
            base: self.base,
            ..Default::default()
        };
    }

    /// Reads one GSI payload. Edges (level-ups, kills, buyback…) only fire from the second
    /// payload of a match on, so connecting mid-game doesn't replay what already happened.
    pub fn ingest(&mut self, v: &Value, now: Instant) {
        self.last_msg = Some(now);
        let Some(map) = v.get("map").filter(|m| m.is_object()) else {
            if self.phase != Phase::Menu || self.primed {
                self.reset_match();
            }
            return;
        };
        let match_id = map
            .get("matchid")
            .map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string))
            .unwrap_or_default();
        if match_id != self.match_id {
            self.reset_match();
            self.match_id = match_id;
        }
        let primed = self.primed;

        self.ingest_map(map, primed, now);

        // Spectators get every player keyed by team ("team2": {"player0": …}).
        let player = v.get("player").filter(|p| p.is_object());
        self.spectating = player.is_some_and(|p| p.get("team2").is_some() || p.get("team3").is_some());
        if !self.spectating {
            if let Some(p) = player {
                self.ingest_player(p, primed, now);
            }
            if let Some(h) = v.get("hero").filter(|h| h.get("level").is_some()) {
                self.ingest_hero(h, primed, now);
            }
            if let Some(a) = v.get("abilities") {
                self.ingest_abilities(a, primed, now);
            }
            if let Some(b) = v.get("buildings") {
                self.ingest_buildings(b, primed, now);
            }
        }
        if let Some(events) = v.get("events").and_then(Value::as_array) {
            for e in events {
                self.ingest_event(e, now);
            }
        }
        self.check_result(primed, now);
        self.primed = true;
    }

    fn ingest_map(&mut self, m: &Value, primed: bool, now: Instant) {
        let gs = text(m, "game_state").unwrap_or_default();
        let phase = Phase::parse(gs);
        if primed && phase != self.phase {
            match phase {
                Phase::Draft => self.note("Draft"),
                Phase::Playing if matches!(self.phase, Phase::PreGame | Phase::Strategy) => {
                    self.horn_at = Some(now);
                    self.note("Horn");
                }
                _ => {}
            }
        }
        if phase == Phase::Playing {
            self.played = true;
        }
        self.phase = phase;
        self.game_state = gs.to_string();

        if let Some(c) = num(m, "clock_time") {
            if c != self.clock_time || self.clock_at.is_none() {
                self.clock_time = c;
                self.clock_at = Some(now);
            }
        }
        self.game_time = num(m, "game_time").unwrap_or(self.game_time);
        self.paused = flag(m, "paused").unwrap_or(false);

        if let Some(day) = flag(m, "daytime") {
            if primed && day != self.daytime && self.in_game() {
                self.daynight_at = Some(now);
                self.note(if day { "Day" } else { "Night" });
            }
            self.daytime = day;
        }
        if let Some(ns) = flag(m, "nightstalker_night") {
            if ns && !self.nightstalker_night {
                self.nightstalker_at = Some(now);
                if primed {
                    self.note("Night Stalker's night");
                }
            }
            self.nightstalker_night = ns;
        }
        self.win_team = text(m, "win_team").and_then(Team::parse);

        // the first hero kill of the match moves the score off 0 - 0
        if let (Some(r), Some(d)) = (num(m, "radiant_score"), num(m, "dire_score")) {
            if primed && self.phase == Phase::Playing && self.score == Some((0, 0)) && r + d > 0 {
                self.firstblood_at = Some(now);
                self.note("First blood");
            }
            self.score = Some((r, d));
        }

        if let Some(rs) = text(m, "roshan_state") {
            let rs = rs.to_ascii_lowercase();
            if primed && rs != self.roshan_state {
                match (self.roshan_state.as_str(), rs.as_str()) {
                    ("alive", "respawn_base") => self.roshan_killed(self.game_time, now),
                    (_, "respawn_variable") => self.roshan_window(now),
                    _ => {}
                }
            }
            self.roshan_state = rs;
        }
        if self.roshwin_due_gt.is_some_and(|due| self.game_time >= due) {
            self.roshan_window(now);
        }
    }

    fn ingest_player(&mut self, p: &Value, primed: bool, now: Instant) {
        if let Some(t) = text(p, "team_name").and_then(Team::parse) {
            self.team = Some(t);
        }
        self.deaths = num(p, "deaths").unwrap_or(self.deaths);

        // Buyback signal 1 of 3: gold spent on buybacks went up.
        if let Some(spent) = num(p, "gold_spent_on_buybacks") {
            if primed && self.gold_spent_on_buybacks.is_some_and(|prev| spent > prev) {
                self.buyback(now);
            }
            self.gold_spent_on_buybacks = Some(spent);
        }

        if let Some(kills) = num(p, "kills") {
            let streak = num(p, "kill_streak").unwrap_or(self.kill_streak);
            if primed && kills > self.kills {
                let delta = (kills - self.kills).clamp(0, 10) as u8;
                let chained = self.last_kill_at.is_some_and(|t| now.saturating_duration_since(t) <= MULTIKILL_WINDOW);
                self.multi_count = if chained { self.multi_count.saturating_add(delta) } else { delta };
                self.last_kill_at = Some(now);
                self.kill_at = Some(now);
                let mut name = "Kill";
                if (3..=10).contains(&streak) && streak > self.kill_streak {
                    self.streak = Some((streak, now));
                    name = streak_name(streak);
                }
                if self.multi_count >= 2 {
                    let n = self.multi_count.min(5);
                    self.multi = Some((n, now));
                    name = multikill_name(n);
                }
                self.note(name);
            }
            self.kills = kills;
            self.kill_streak = streak;
        }
    }

    fn ingest_hero(&mut self, h: &Value, primed: bool, now: Instant) {
        let level = num(h, "level").unwrap_or(self.hero_level);
        // Before the first spawn GSI reports alive=false at level 0; that isn't a death.
        let alive = flag(h, "alive").unwrap_or(true);
        let dead = !alive && level > 0;
        let respawn = num(h, "respawn_seconds").unwrap_or(0);

        // Buyback: GSI has no buyback flag, so any of three signals counts (deduped):
        //  1. player.gold_spent_on_buybacks rises (in ingest_player),
        //  2. hero.buyback_cooldown jumps by more than a minute (only a buyback does that),
        //  3. the hero comes back to life while the respawn timer still had 3 s or more
        //     (a natural respawn always counts down to 0 first; aegis/reincarnation do too).
        if let Some(cd) = num(h, "buyback_cooldown") {
            if primed && cd > self.buyback_cooldown + BUYBACK_CD_JUMP_S {
                self.buyback(now);
            }
            self.buyback_cooldown = cd;
        }
        let was_dead = self.died_at.is_some();
        if dead {
            if !was_dead {
                self.died_at = Some(now);
                if primed {
                    self.note("Died");
                }
            }
        } else {
            if primed && was_dead && alive && self.respawn_seconds >= 3 {
                self.buyback(now);
            }
            self.died_at = None;
        }
        if respawn != self.respawn_seconds || self.respawn_at.is_none() {
            self.respawn_at = Some(now);
        }
        self.respawn_seconds = respawn;
        self.alive = !dead;

        if primed {
            if let Some(&th) = [18, 12, 6].iter().find(|&&th| self.hero_level < th && level >= th) {
                self.spike = Some((th, now));
                self.note(format!("Level {th}"));
            }
        }
        self.hero_level = level;
        self.health_percent = num(h, "health_percent").unwrap_or(self.health_percent);

        let since = |on: Option<bool>, cur: Option<Instant>| match on {
            Some(true) => cur.or(Some(now)),
            _ => None,
        };
        self.stunned = since(flag(h, "stunned"), self.stunned);
        self.silenced = since(flag(h, "silenced"), self.silenced);
        self.hexed = since(flag(h, "hexed"), self.hexed);

        let scepter = flag(h, "aghanims_scepter").unwrap_or(false);
        let shard = flag(h, "aghanims_shard").unwrap_or(false);
        if primed && ((scepter && !self.aghanims_scepter) || (shard && !self.aghanims_shard)) {
            self.aghs_at = Some(now);
            self.note(if scepter && !self.aghanims_scepter { "Aghanim's Scepter" } else { "Aghanim's Shard" });
        }
        self.aghanims_scepter = scepter;
        self.aghanims_shard = shard;
    }

    /// Ultimate ready = the learned ultimate's cooldown goes from >0 to 0. (`can_cast` also
    /// flips on mana, so it is not used as the trigger.)
    fn ingest_abilities(&mut self, a: &Value, primed: bool, now: Instant) {
        let Some(list) = a.as_object() else { return };
        let ult = list
            .values()
            .find(|x| flag(x, "ultimate") == Some(true) && num(x, "level").unwrap_or(0) > 0);
        let Some(ult) = ult else { return };
        let name = text(ult, "name").unwrap_or_default();
        let cd = num(ult, "cooldown").unwrap_or(0);
        if primed && name == self.ult_name && self.ult_cooldown > 0 && cd == 0 {
            self.ult_at = Some(now);
            self.note("Ultimate ready");
        }
        if name != self.ult_name {
            self.ult_name = name.to_string();
        }
        self.ult_cooldown = cd;
        self.ult_can_cast = flag(ult, "can_cast").unwrap_or(false);
    }

    /// `buildings.<your team>.<name>.health` dropping on a tier 3/4 tower, barracks or Ancient.
    fn ingest_buildings(&mut self, b: &Value, primed: bool, now: Instant) {
        let Some(team) = self.team else { return };
        let Some(list) = b.get(team.key()).and_then(Value::as_object) else { return };
        let mut hit = false;
        for (name, info) in list {
            if !(name.contains("tower3") || name.contains("tower4") || name.contains("rax_") || name.contains("_fort")) {
                continue;
            }
            let Some(hp) = num(info, "health") else { continue };
            if let Some(prev) = self.building_hp.insert(name.clone(), hp) {
                hit |= hp < prev;
            }
        }
        if primed && hit {
            let quiet = self.base_hit_at.is_none_or(|t| now.saturating_duration_since(t) > Duration::from_secs(10));
            self.base_hit_at = Some(now);
            if quiet {
                self.note("Base under attack");
            }
        }
    }

    /// One entry of the `events` array. The array can hold older events, so only ones stamped
    /// within 20 s of the current `map.game_time` count.
    fn ingest_event(&mut self, e: &Value, now: Instant) {
        let kind = text(e, "event_type").unwrap_or_default();
        let gt = num(e, "game_time").unwrap_or(self.game_time);
        if kind.eq_ignore_ascii_case("roshan_killed") && gt >= self.game_time - 20 {
            self.roshan_killed(gt, now);
        }
    }

    fn roshan_killed(&mut self, gt: i64, now: Instant) {
        if self.roshan_kill_gt.is_some_and(|prev| gt - prev < 60) {
            return; // same kill seen again (event list and roshan_state both report it)
        }
        self.roshan_kill_gt = Some(gt);
        self.roshwin_due_gt = Some(gt + ROSHAN_MIN_RESPAWN_S);
        self.roshan_at = Some(now);
        self.note("Roshan killed");
    }

    fn roshan_window(&mut self, now: Instant) {
        if self.roshwin_at.is_some_and(|t| now.saturating_duration_since(t) < Duration::from_secs(60)) {
            return;
        }
        self.roshwin_due_gt = None;
        self.roshwin_at = Some(now);
        self.note("Roshan may be back");
    }

    fn buyback(&mut self, now: Instant) {
        if self.buyback_at.is_some_and(|t| now.saturating_duration_since(t) < Duration::from_secs(10)) {
            return;
        }
        self.buyback_at = Some(now);
        self.note("Buyback");
    }

    /// `map.win_team` set (POST_GAME) after GAME_IN_PROGRESS was seen.
    fn check_result(&mut self, primed: bool, now: Instant) {
        if self.result.is_some() || !primed || !self.played {
            return;
        }
        if let (Some(win), Some(team)) = (self.win_team, self.team) {
            let won = win == team;
            self.result = Some((won, now));
            self.note(if won { "Victory" } else { "Defeat" });
        }
    }

    /// Plays once, in place of live game events, even with no game connected.
    pub fn fire_test(&mut self, fx: Fx, now: Instant) {
        self.test = Some((fx, now));
        self.last_event = format!("test: {}", fx.label().to_lowercase());
    }

    pub fn level(&self, fx: Fx) -> f32 {
        self.fx_level.get(&fx).copied().unwrap_or(fx.default_level() as f32 / 5.0).clamp(0.0, 1.0)
    }

    /// The colour the room rests on between events, or None to leave it to the normal program
    /// (not connected, not in a match, or `Base::Screen`).
    pub fn base_color(&self, now: Instant) -> Option<Rgb> {
        if !self.connected(now) || !matches!(self.phase, Phase::Strategy | Phase::PreGame | Phase::Playing) {
            return None;
        }
        match self.base {
            Base::Screen => None,
            Base::Faction => self.team.map(Team::color),
            Base::Cycle => {
                let day = DAY_WARM.scale(0.7);
                if self.nightstalker_night {
                    return Some(NIGHTSTALKER_DARK);
                }
                let target = if self.daytime { day } else { NIGHT_DEEP };
                let from = if self.daytime { NIGHT_DEEP } else { day };
                let k = self.daynight_at.map_or(1.0, |t| now.saturating_duration_since(t).as_secs_f32() / 3.0);
                Some(if k >= 1.0 { target } else { from.lerp(target, smoothstep(k)) })
            }
        }
    }

    /// Later overlays win. Applies per-effect switches, strengths and `calm`.
    pub fn overlays(&self, now: Instant) -> Vec<Overlay> {
        if let Some((fx, t0)) = self.test {
            let t = now.saturating_duration_since(t0).as_secs_f32();
            if t < fx.duration() {
                let k = self.level(fx);
                let mut demo = fx.demo_state(t, now);
                demo.calm = self.calm;
                return demo.tagged(now).into_iter().map(|(f, o)| self.shape(f, o, k)).collect();
            }
        }
        self.tagged(now)
            .into_iter()
            .filter(|(f, _)| !self.fx_off.contains(f))
            .map(|(f, o)| self.shape(f, o, self.level(f)))
            .collect()
    }

    fn shape(&self, fx: Fx, o: Overlay, k: f32) -> Overlay {
        let calm = self.calm && fx.flashy();
        match o {
            Overlay::Tint { color, alpha } if calm => Overlay::Tint { color: color.lerp(WARM_GREY, 0.6), alpha: alpha * k * 0.45 },
            Overlay::Tint { color, alpha } => Overlay::Tint { color, alpha: alpha * k },
            Overlay::Desaturate(a) => Overlay::Desaturate(a * k),
            Overlay::Dim(a) => Overlay::Dim(a * k),
        }
    }

    /// 0..1 envelope of a one-shot effect `e` seconds in: a sharp hit decaying with `decay`,
    /// or with `calm` on a flashy effect, a soft rise and fall over `dur`.
    fn envelope(&self, fx: Fx, e: f32, dur: f32, decay: f32) -> f32 {
        if !(0.0..dur).contains(&e) {
            return 0.0;
        }
        if self.calm && fx.flashy() {
            (PI * e / dur).sin().max(0.0)
        } else {
            (-e / decay).exp() * (1.0 - e / dur).sqrt()
        }
    }

    /// Each overlay tagged with the effect that made it. Later ones win.
    fn tagged(&self, now: Instant) -> Vec<(Fx, Overlay)> {
        let mut out = Vec::new();
        if !self.connected(now) {
            return out;
        }
        let since = |t: Instant| now.saturating_duration_since(t).as_secs_f32();
        let clock = now_secs(now);
        let tint = |fx: Fx, color: Rgb, alpha: f32| (fx, Overlay::Tint { color, alpha: alpha.clamp(0.0, 1.0) });
        let me = self.in_game() && !self.spectating;
        let calm = self.calm;

        if self.phase == Phase::Draft {
            let col = self.team.map_or(Fx::Draft.color(), Team::color);
            let breathe = 0.5 - 0.5 * (clock / 6.0 * TAU).cos();
            out.push(tint(Fx::Draft, col, 0.45 + 0.2 * breathe));
        }

        if self.phase == Phase::PreGame {
            let c = self.clock(now);
            if (-10.0..=0.5).contains(&c) {
                let k = ((c + 10.0) / 10.0).clamp(0.0, 1.0);
                out.push(tint(Fx::Horn, Fx::Horn.color(), 0.08 + 0.5 * k * k));
            }
        }

        if let Some(t) = self.daynight_at {
            let e = since(t);
            if e < 4.0 {
                let col = if self.daytime { DAY_WARM } else { NIGHT_BRIGHT };
                out.push(tint(Fx::DayNight, col, 0.55 * (PI * e / 4.0).sin()));
            }
        }
        if self.nightstalker_night && self.in_game() {
            let e = self.nightstalker_at.map_or(10.0, since);
            out.push((Fx::DayNight, Overlay::Dim(0.3 + 0.4 * (-e / 1.5).exp())));
        }

        // colour refills over the last 5 s of the respawn timer
        if me && self.died_at.is_some() {
            let e = self.died_at.map_or(1.0, since);
            let left = self.respawn_remaining(now);
            let a = (e / 0.8).min(1.0) * (left / 5.0).clamp(0.0, 1.0);
            out.push((Fx::Respawn, Overlay::Desaturate(0.85 * a)));
            out.push((Fx::Respawn, Overlay::Dim(0.55 * a)));
        }

        if let Some((false, t)) = self.result {
            let e = since(t);
            if e < 7.0 {
                let a = (e / 3.0).min(1.0) * ((7.0 - e) / 2.0).min(1.0);
                out.push((Fx::Defeat, Overlay::Desaturate(0.5 * a)));
                out.push(tint(Fx::Defeat, Fx::Defeat.color(), 0.3 * a));
                out.push((Fx::Defeat, Overlay::Dim(0.65 * a)));
            }
        }

        if let (true, Some(t)) = (me, self.silenced) {
            let f = (since(t) / 0.4).min(1.0);
            out.push(tint(Fx::Silenced, Fx::Silenced.color(), 0.4 * f));
            out.push((Fx::Silenced, Overlay::Dim(0.2 * f)));
        }

        if let (true, Some(t)) = (me, self.hexed) {
            let f = (since(t) / 0.3).min(1.0);
            out.push(tint(Fx::Hexed, Fx::Hexed.color(), f * (0.35 + 0.2 * (clock * TAU * 1.5).sin())));
        }

        if me && self.alive && self.health_percent > 0 && self.health_percent < 25 {
            let sev = (25 - self.health_percent) as f32 / 25.0;
            let hz = 0.9 + 1.6 * sev;
            let beat = heartbeat((clock * hz).fract());
            out.push(tint(Fx::LowHp, Fx::LowHp.color(), 0.2 + 0.55 * beat * (0.5 + 0.5 * sev)));
        }

        if let Some((n, t)) = self.streak {
            let e = since(t);
            if e < 5.0 {
                let a = (e / 1.5).min(1.0) * (1.0 - ((e - 1.5) / 3.5).max(0.0));
                out.push(tint(Fx::Streak, Fx::Streak.color(), (0.3 + 0.07 * (n - 3) as f32) * a));
            }
        }

        if let Some(t) = self.base_hit_at {
            let e = since(t);
            if e < 3.0 {
                let fl = if calm { 0.6 + 0.4 * (clock * TAU * 0.7).sin().abs() } else { 0.5 + 0.5 * (clock * 17.0).sin().abs() };
                out.push(tint(Fx::Buildings, Fx::Buildings.color(), 0.5 * fl * (1.0 - e / 3.0)));
            }
        }

        if let Some(t) = self.roshwin_at {
            let e = since(t);
            if e < 4.0 {
                out.push(tint(Fx::RoshWin, Fx::RoshWin.color(), 0.4 * (0.5 - 0.5 * (e / 2.0 * TAU).cos())));
            }
        }

        if let Some(t) = self.roshan_at {
            let e = since(t);
            if e < 4.5 {
                let beat = (-((e % 1.5) / 1.5) * 4.0).exp();
                out.push(tint(Fx::Roshan, Fx::Roshan.color(), 0.8 * beat * (1.0 - e / 4.5).sqrt()));
            }
        }

        if let Some(t) = self.aghs_at {
            let e = since(t);
            if e < 2.5 {
                let shimmer = 0.65 + 0.35 * (e * 18.0).sin();
                out.push(tint(Fx::Aghs, Fx::Aghs.color(), 0.6 * shimmer * (PI * e / 2.5).sin()));
            }
        }

        if let Some(t) = self.ult_at {
            let a = self.envelope(Fx::Ult, since(t), 1.2, 0.3);
            if a > 0.0 {
                out.push(tint(Fx::Ult, Fx::Ult.color(), 0.65 * a));
            }
        }

        if let Some(t) = self.kill_at {
            let a = self.envelope(Fx::Kill, since(t), 1.0, 0.25);
            if a > 0.0 {
                out.push(tint(Fx::Kill, self.team.map_or(Fx::Kill.color(), Team::bright), 0.7 * a));
            }
        }

        // one hit per kill, each bigger; rampage (5) is a full sweep
        if let Some((n, t)) = self.multi {
            let e = since(t);
            let dur = Fx::MultiKill.duration();
            if e < dur {
                let a = if calm {
                    0.8 * (PI * e / dur).sin()
                } else if n >= 5 {
                    (PI * e / dur).sin().powf(0.5) * (0.75 + 0.25 * (e * 14.0).cos())
                } else {
                    let gap = 0.45;
                    let k = ((e / gap) as u8).min(n.saturating_sub(1));
                    let local = e - k as f32 * gap;
                    let amp = 0.5 + 0.15 * k as f32;
                    amp * (-local / 0.2).exp() * (1.0 - e / dur).sqrt()
                };
                out.push(tint(Fx::MultiKill, Fx::MultiKill.color(), a));
            }
        }

        if let Some(t) = self.firstblood_at {
            let a = self.envelope(Fx::FirstBlood, since(t), 1.5, 0.3);
            if a > 0.0 {
                out.push(tint(Fx::FirstBlood, Fx::FirstBlood.color(), 0.95 * a));
            }
        }

        if let Some((lvl, t)) = self.spike {
            let e = since(t);
            let a = self.envelope(Fx::Spike, e, 2.2, 0.5);
            if a > 0.0 {
                let amp = match lvl {
                    ..=6 => 0.6,
                    7..=12 => 0.8,
                    _ => 1.0,
                };
                let col = if lvl >= 18 { Rgb::WHITE.lerp(Fx::Spike.color(), e / 0.35) } else { Fx::Spike.color() };
                out.push(tint(Fx::Spike, col, amp * a));
            }
        }

        if let Some(t) = self.buyback_at {
            let a = self.envelope(Fx::Buyback, since(t), 2.0, 0.45);
            if a > 0.0 {
                out.push(tint(Fx::Buyback, Fx::Buyback.color(), a));
            }
        }

        if let Some(t) = self.horn_at {
            let a = self.envelope(Fx::Horn, since(t), 3.0, 0.8);
            if a > 0.0 {
                out.push(tint(Fx::Horn, Fx::Horn.color(), 0.9 * a));
            }
        }

        if let Some((true, t)) = self.result {
            let e = since(t);
            let dur = Fx::Victory.duration();
            if e < dur {
                let col = self.team.map_or(Fx::Victory.color(), Team::victory);
                let a = 0.85 * (PI * e / dur).sin() * (0.75 + 0.25 * (e / 2.0 * TAU).cos());
                out.push(tint(Fx::Victory, col, a));
            }
        }

        // stunned last so it wins
        if let (true, Some(t)) = (me, self.stunned) {
            let f = (since(t) / if calm { 0.8 } else { 0.12 }).min(1.0);
            out.push(tint(Fx::Stunned, Rgb::new(0.92, 0.95, 1.0), 0.75 * f));
        }
        out
    }
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn now_secs(now: Instant) -> f32 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    now.saturating_duration_since(*START.get_or_init(Instant::now)).as_secs_f32()
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

fn steam_roots() -> Vec<PathBuf> {
    [r"C:\Program Files (x86)\Steam", r"C:\Program Files\Steam"].into_iter().map(PathBuf::from).filter(|p| p.is_dir()).collect()
}

/// Every Steam library folder (`libraryfolders.vdf` lists the ones on other drives).
fn steam_libraries() -> Vec<PathBuf> {
    let mut libs = steam_roots();
    for root in steam_roots() {
        let Ok(vdf) = std::fs::read_to_string(root.join(r"steamapps\libraryfolders.vdf")) else { continue };
        for line in vdf.lines() {
            if let Some(rest) = line.trim().strip_prefix("\"path\"") {
                let p = PathBuf::from(rest.trim().trim_matches('"').replace("\\\\", "\\"));
                if !libs.contains(&p) {
                    libs.push(p);
                }
            }
        }
    }
    libs
}

/// `<library>\steamapps\common\dota 2 beta\game\dota\cfg`, if Dota is installed.
fn dota_cfg() -> Option<PathBuf> {
    steam_libraries().into_iter().map(|l| l.join(r"steamapps\common\dota 2 beta\game\dota\cfg")).find(|p| p.is_dir())
}

/// Dota's `cfg\gamestate_integration` folder, created if missing. None if Dota isn't found.
pub fn cfg_dir() -> Option<PathBuf> {
    let dir = dota_cfg()?.join("gamestate_integration");
    if !dir.is_dir() {
        std::fs::create_dir_all(&dir).ok()?;
    }
    Some(dir)
}

pub fn is_installed() -> bool {
    dota_cfg().is_some_and(|c| {
        let dir = c.join("gamestate_integration");
        dir.join(CFG_NAME).is_file() || dir.join(LEGACY_CFG_NAME).is_file()
    })
}

/// Writes the GSI cfg. Dota reads it only at launch.
pub fn install_cfg(dir: &Path, port: u16, token: &str) -> Result<PathBuf> {
    let path = dir.join(CFG_NAME);
    std::fs::write(&path, cfg_body(port, token)).with_context(|| format!("writing {}", path.display()))?;
    // one cfg only, or Dota would send every event twice
    let _ = std::fs::remove_file(dir.join(LEGACY_CFG_NAME));
    Ok(path)
}

fn cfg_body(port: u16, token: &str) -> String {
    format!(
        r#""RokSync lights"
{{
  "uri"       "http://127.0.0.1:{port}/dota"
  "timeout"   "5.0"
  "buffer"    "0.0"
  "throttle"  "0.1"
  "heartbeat" "10.0"
  "auth"
  {{
    "token" "{token}"
  }}
  "data"
  {{
    "provider"  "1"
    "map"       "1"
    "player"    "1"
    "hero"      "1"
    "abilities" "1"
    "items"     "1"
    "events"    "1"
    "buildings" "1"
  }}
}}
"#
    )
}

/// Is `-gamestateintegration` in Dota's Steam launch options? Reads every Steam user's
/// `userdata\<id>\config\localconfig.vdf`. None if no Steam user config was found.
pub fn launch_option_set() -> Option<bool> {
    let mut found = None;
    for root in steam_roots() {
        let Ok(users) = std::fs::read_dir(root.join("userdata")) else { continue };
        for user in users.flatten() {
            let Ok(vdf) = std::fs::read_to_string(user.path().join(r"config\localconfig.vdf")) else { continue };
            let set = vdf_launch_options(&vdf, DOTA_APP_ID).is_some_and(|o| o.split_whitespace().any(|w| w.eq_ignore_ascii_case(LAUNCH_OPTION)));
            found = Some(found.unwrap_or(false) || set);
        }
    }
    found
}

/// `LaunchOptions` of `app_id` inside an `apps` block of a Steam VDF text.
fn vdf_launch_options(vdf: &str, app_id: &str) -> Option<String> {
    let mut stack: Vec<String> = Vec::new();
    let mut key: Option<String> = None;
    let mut chars = vdf.chars();
    while let Some(c) = chars.next() {
        match c {
            '{' => stack.push(key.take().unwrap_or_default()),
            '}' => {
                stack.pop();
                key = None;
            }
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(n) = chars.next() {
                                s.push(n);
                            }
                        }
                        '"' => break,
                        _ => s.push(c),
                    }
                }
                match key.take() {
                    None => key = Some(s),
                    Some(k) => {
                        let in_app = stack.last().is_some_and(|x| x == app_id) && stack.iter().any(|x| x.eq_ignore_ascii_case("apps"));
                        if in_app && k.eq_ignore_ascii_case("LaunchOptions") {
                            return Some(s);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Listens on 127.0.0.1 only. Requests without the matching auth token are ignored.
pub fn spawn_server(port: u16, token: String, state: SharedDota) -> Result<()> {
    use std::io::Read;
    let server = tiny_http::Server::http(("127.0.0.1", port)).map_err(|e| anyhow::anyhow!("can't listen on 127.0.0.1:{port}: {e}"))?;
    std::thread::Builder::new().name("dota-gsi".into()).spawn(move || {
        for mut req in server.incoming_requests() {
            let mut body = String::new();
            let _ = req.as_reader().take(4 << 20).read_to_string(&mut body);
            if let Ok(v) = serde_json::from_str::<Value>(&body) {
                let ok = v.get("auth").and_then(|a| a.get("token")).and_then(Value::as_str) == Some(token.as_str());
                if ok {
                    if let Ok(mut s) = state.lock() {
                        s.ingest(&v, Instant::now());
                    }
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

    fn secs(t0: Instant, s: f32) -> Instant {
        t0 + Duration::from_secs_f32(s)
    }

    /// A playing client's payload, trimmed to the fields read (names as Dota sends them).
    fn game(state: &str, clock: i64, day: bool) -> Value {
        json!({
            "provider": {"name": "Dota 2", "appid": 570, "version": 47, "timestamp": 1700000000},
            "map": {
                "name": "start", "matchid": "7412345678", "game_time": clock + 90, "clock_time": clock,
                "daytime": day, "nightstalker_night": false, "game_state": state,
                "paused": false, "win_team": "none", "customgamename": ""
            },
            "player": {
                "steamid": "1", "name": "me", "activity": "playing",
                "kills": 0, "deaths": 0, "assists": 0, "last_hits": 0, "denies": 0, "kill_streak": 0,
                "team_name": "radiant", "gold": 600, "gold_reliable": 0, "gold_unreliable": 600
            },
            "hero": {
                "id": 42, "name": "npc_dota_hero_skeleton_king", "level": 1, "xp": 0, "alive": true,
                "respawn_seconds": 0, "buyback_cost": 200, "buyback_cooldown": 0,
                "health": 640, "max_health": 640, "health_percent": 100,
                "silenced": false, "stunned": false, "hexed": false,
                "aghanims_scepter": false, "aghanims_shard": false
            },
            "abilities": {
                "ability0": {"name": "skeleton_king_hellfire_blast", "level": 1, "can_cast": true, "passive": false, "cooldown": 0, "ultimate": false},
                "ability3": {"name": "skeleton_king_reincarnation", "level": 0, "can_cast": false, "passive": true, "cooldown": 0, "ultimate": true}
            },
            "auth": {"token": "t"}
        })
    }

    fn playing(clock: i64) -> Value {
        game("DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", clock, true)
    }

    fn has(s: &DotaState, fx: Fx, now: Instant) -> bool {
        s.tagged(now).iter().any(|(f, _)| *f == fx)
    }

    fn last_alpha(o: &[Overlay]) -> f32 {
        match o.last() {
            Some(Overlay::Tint { alpha, .. }) => *alpha,
            _ => 0.0,
        }
    }

    #[test]
    fn horn_and_day_night() {
        let t0 = Instant::now();
        let mut s = DotaState::default();
        s.ingest(&game("DOTA_GAMERULES_STATE_STRATEGY_TIME", -90, true), t0);
        s.ingest(&game("DOTA_GAMERULES_STATE_PRE_GAME", -30, true), secs(t0, 1.0));
        assert!(!has(&s, Fx::Horn, secs(t0, 1.0)), "no build-up 30 s out");
        s.ingest(&game("DOTA_GAMERULES_STATE_PRE_GAME", -4, true), secs(t0, 2.0));
        assert!(has(&s, Fx::Horn, secs(t0, 2.0)), "builds in the last 10 s");
        s.ingest(&playing(0), secs(t0, 3.0));
        assert_eq!(s.last_event, "Horn");
        assert!(s.horn_at.is_some());
        assert_eq!(s.base_color(secs(t0, 3.0)), Some(DAY_WARM.scale(0.7)));

        s.ingest(&game("DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", 300, false), secs(t0, 4.0));
        assert_eq!(s.last_event, "Night");
        assert!(has(&s, Fx::DayNight, secs(t0, 5.0)));
        let night = s.base_color(secs(t0, 9.0)).unwrap();
        assert!(night.b > night.r, "night is blue");

        let mut ns = game("DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", 301, false);
        ns["map"]["nightstalker_night"] = json!(true);
        s.ingest(&ns, secs(t0, 9.0));
        assert_eq!(s.base_color(secs(t0, 9.0)), Some(NIGHTSTALKER_DARK));
        assert!(matches!(s.overlays(secs(t0, 9.0)).last(), Some(Overlay::Dim(_))));

        s.base = Base::Faction;
        assert_eq!(s.base_color(secs(t0, 9.0)), Some(RADIANT));
        s.base = Base::Screen;
        assert_eq!(s.base_color(secs(t0, 9.0)), None);
    }

    #[test]
    fn connecting_mid_game_does_not_replay() {
        let t0 = Instant::now();
        let mut s = DotaState::default();
        let mut v = playing(900);
        v["hero"]["level"] = json!(14);
        v["player"]["kills"] = json!(7);
        v["player"]["kill_streak"] = json!(5);
        s.ingest(&v, t0);
        assert!(s.spike.is_none() && s.kill_at.is_none() && s.streak.is_none() && s.horn_at.is_none());
    }

    #[test]
    fn level_six_spike() {
        let t0 = Instant::now();
        let mut s = DotaState::default();
        let mut v = playing(400);
        v["hero"]["level"] = json!(5);
        s.ingest(&v, t0);
        v["hero"]["level"] = json!(6);
        s.ingest(&v, secs(t0, 1.0));
        assert_eq!(s.last_event, "Level 6");
        assert_eq!(s.spike.map(|(l, _)| l), Some(6));
        assert!(has(&s, Fx::Spike, secs(t0, 1.2)));
        let at = s.spike.map(|(_, t)| t);
        v["hero"]["level"] = json!(7);
        s.ingest(&v, secs(t0, 2.0));
        assert_eq!(s.spike.map(|(_, t)| t), at);
    }

    #[test]
    fn buyback_vs_natural_respawn() {
        let t0 = Instant::now();
        let mut s = DotaState::default();
        let mut v = playing(1500);
        v["hero"]["level"] = json!(18);
        s.ingest(&v, t0);
        v["hero"]["alive"] = json!(false);
        v["hero"]["respawn_seconds"] = json!(52);
        s.ingest(&v, secs(t0, 1.0));
        assert!(s.died_at.is_some());
        assert!(has(&s, Fx::Respawn, secs(t0, 3.0)));
        v["hero"]["alive"] = json!(true);
        v["hero"]["respawn_seconds"] = json!(0);
        v["hero"]["buyback_cooldown"] = json!(480);
        s.ingest(&v, secs(t0, 3.0));
        assert_eq!(s.last_event, "Buyback");
        assert!(s.died_at.is_none());
        assert!(has(&s, Fx::Buyback, secs(t0, 3.2)));

        let mut s = DotaState::default();
        let mut v = playing(1600);
        v["hero"]["level"] = json!(18);
        v["hero"]["alive"] = json!(false);
        v["hero"]["respawn_seconds"] = json!(2);
        s.ingest(&v, t0);
        v["hero"]["respawn_seconds"] = json!(1);
        s.ingest(&v, secs(t0, 1.0));
        v["hero"]["alive"] = json!(true);
        v["hero"]["respawn_seconds"] = json!(0);
        s.ingest(&v, secs(t0, 2.0));
        assert!(s.buyback_at.is_none());
        assert!(s.died_at.is_none());
    }

    #[test]
    fn kills_chain_into_multikill_and_streak() {
        let t0 = Instant::now();
        let mut s = DotaState::default();
        let mut v = playing(1200);
        s.ingest(&v, t0);
        let kill = |s: &mut DotaState, v: &mut Value, kills: i64, at: f32| {
            v["player"]["kills"] = json!(kills);
            v["player"]["kill_streak"] = json!(kills);
            s.ingest(v, secs(t0, at));
        };
        kill(&mut s, &mut v, 1, 1.0);
        assert_eq!(s.last_event, "Kill");
        assert!(has(&s, Fx::Kill, secs(t0, 1.1)));
        kill(&mut s, &mut v, 2, 6.0);
        assert_eq!(s.last_event, "Double kill");
        kill(&mut s, &mut v, 3, 12.0);
        assert_eq!(s.last_event, "Triple kill", "multi-kill name wins over the streak");
        assert_eq!(s.streak.map(|(n, _)| n), Some(3));
        kill(&mut s, &mut v, 5, 20.0);
        assert_eq!(s.last_event, "Rampage");
        assert_eq!(s.multi.map(|(n, _)| n), Some(5));
        assert!(has(&s, Fx::MultiKill, secs(t0, 21.0)));
        kill(&mut s, &mut v, 6, 60.0);
        assert_eq!(s.multi_count, 1);
        assert_eq!(s.last_event, "Unstoppable");
    }

    #[test]
    fn ultimate_ready_and_status_effects() {
        let t0 = Instant::now();
        let mut s = DotaState::default();
        let mut v = playing(700);
        v["hero"]["level"] = json!(6);
        v["abilities"]["ability3"]["level"] = json!(1);
        v["abilities"]["ability3"]["cooldown"] = json!(2);
        s.ingest(&v, t0);
        v["abilities"]["ability3"]["cooldown"] = json!(0);
        s.ingest(&v, secs(t0, 2.0));
        assert_eq!(s.last_event, "Ultimate ready");

        v["hero"]["stunned"] = json!(true);
        s.ingest(&v, secs(t0, 3.0));
        assert!(s.stunned.is_some());
        let o = s.overlays(secs(t0, 3.5));
        assert!(last_alpha(&o) > 0.3, "stun freezes the room");
        v["hero"]["stunned"] = json!(false);
        s.ingest(&v, secs(t0, 4.0));
        assert!(!has(&s, Fx::Stunned, secs(t0, 4.0)));
    }

    #[test]
    fn victory_and_defeat_follow_your_team() {
        let t0 = Instant::now();
        for (winner, won) in [("radiant", true), ("dire", false)] {
            let mut s = DotaState::default();
            s.ingest(&playing(2400), t0);
            let mut end = game("DOTA_GAMERULES_STATE_POST_GAME", 2465, true);
            end["map"]["win_team"] = json!(winner);
            s.ingest(&end, secs(t0, 1.0));
            assert_eq!(s.result.map(|(w, _)| w), Some(won), "{winner}");
            assert_eq!(s.last_event, if won { "Victory" } else { "Defeat" });
            let fx = if won { Fx::Victory } else { Fx::Defeat };
            assert!(has(&s, fx, secs(t0, 3.0)));
        }
        let mut s = DotaState::default();
        let mut end = game("DOTA_GAMERULES_STATE_POST_GAME", 2465, true);
        end["map"]["win_team"] = json!("radiant");
        s.ingest(&end, t0);
        s.ingest(&end, secs(t0, 1.0));
        assert!(s.result.is_none());
    }

    #[test]
    fn roshan_from_events_and_window() {
        let t0 = Instant::now();
        let mut s = DotaState::default();
        let mut v = playing(1290);
        s.ingest(&v, t0);
        v["events"] = json!([
            {"game_time": 400, "event_type": "aegis_picked_up", "player_id": 3, "snatched": false},
            {"game_time": 1379, "event_type": "roshan_killed", "killed_by_team": "dire", "killer_player_id": 7}
        ]);
        s.ingest(&v, secs(t0, 1.0));
        assert_eq!(s.last_event, "Roshan killed");
        assert!(has(&s, Fx::Roshan, secs(t0, 1.5)));
        let first = s.roshan_at;
        s.ingest(&v, secs(t0, 2.0));
        assert_eq!(s.roshan_at, first);
        let mut later = playing(1290 + 480);
        later["map"]["game_time"] = json!(1379 + 480);
        s.ingest(&later, secs(t0, 3.0));
        assert_eq!(s.last_event, "Roshan may be back");
        let mut s = DotaState::default();
        let mut v = playing(2000);
        v["events"] = json!([{"game_time": 900, "event_type": "roshan_killed", "killed_by_team": "radiant"}]);
        s.ingest(&v, t0);
        s.ingest(&v, secs(t0, 1.0));
        assert!(s.roshan_at.is_none());
    }

    #[test]
    fn base_under_attack_and_first_blood() {
        let t0 = Instant::now();
        let mut s = DotaState::default();
        let mut v = playing(180);
        v["map"]["radiant_score"] = json!(0);
        v["map"]["dire_score"] = json!(0);
        v["buildings"] = json!({"radiant": {
            "dota_goodguys_tower1_top": {"health": 1800, "max_health": 1800},
            "dota_goodguys_tower3_mid": {"health": 2500, "max_health": 2500},
            "dota_goodguys_fort": {"health": 4500, "max_health": 4500}
        }});
        s.ingest(&v, t0);
        v["buildings"]["radiant"]["dota_goodguys_tower1_top"]["health"] = json!(1500);
        s.ingest(&v, secs(t0, 1.0));
        assert!(s.base_hit_at.is_none());
        v["buildings"]["radiant"]["dota_goodguys_tower3_mid"]["health"] = json!(2300);
        s.ingest(&v, secs(t0, 2.0));
        assert_eq!(s.last_event, "Base under attack");

        v["map"]["dire_score"] = json!(1);
        s.ingest(&v, secs(t0, 3.0));
        assert_eq!(s.last_event, "First blood");
        assert!(has(&s, Fx::FirstBlood, secs(t0, 3.1)));
    }

    #[test]
    fn spectating_skips_hero_effects() {
        let t0 = Instant::now();
        let mut s = DotaState::default();
        let mut v = playing(600);
        v["player"] = json!({"team2": {"player0": {"kills": 0}}, "team3": {"player5": {"kills": 0}}});
        v["hero"] = json!({"team2": {"player0": {"level": 3, "health_percent": 5, "alive": true}}});
        s.ingest(&v, t0);
        s.ingest(&v, secs(t0, 1.0));
        assert!(s.spectating);
        assert_eq!(s.hero_level, 0);
        assert!(!has(&s, Fx::LowHp, secs(t0, 1.0)));
    }

    #[test]
    fn menu_resets_match_but_keeps_settings() {
        let t0 = Instant::now();
        let mut s = DotaState { calm: true, ..Default::default() };
        s.fx_off.insert(Fx::Kill);
        s.ingest(&playing(100), t0);
        s.ingest(&json!({"provider": {"name": "Dota 2", "appid": 570}, "auth": {"token": "t"}}), secs(t0, 1.0));
        assert_eq!(s.phase, Phase::Menu);
        assert!(s.match_id.is_empty());
        assert!(s.calm && s.fx_off.contains(&Fx::Kill));
        assert!(s.connected(secs(t0, 1.0)));
    }

    #[test]
    fn every_fx_demo_produces_light() {
        let now = Instant::now();
        for fx in Fx::ALL {
            for k in [0.1, 0.3, 0.6] {
                let t = fx.duration() * k;
                let d = fx.demo_state(t, now);
                assert!(has(&d, fx, now), "{fx:?} shows nothing at {t:.2}s");
                assert!(!d.overlays(now).is_empty());
            }
        }
        let mut s = DotaState::default();
        assert!(s.overlays(now).is_empty());
        s.fire_test(Fx::Buyback, now);
        assert!(!s.overlays(now + Duration::from_millis(100)).is_empty());
        assert!(s.overlays(now + Duration::from_secs(10)).is_empty());
    }

    #[test]
    fn user_switches_strength_and_calm() {
        let now = Instant::now();
        let at = now + Duration::from_millis(50);
        let mut s = Fx::Kill.demo_state(0.0, now);
        s.last_msg = Some(at);
        let default = last_alpha(&s.overlays(at));
        s.fx_level.insert(Fx::Kill, 1.0);
        let full = last_alpha(&s.overlays(at));
        assert!(full > 0.5);
        assert!((default - full * 0.4).abs() < 1e-4);
        s.fx_level.insert(Fx::Kill, 0.5);
        assert!((last_alpha(&s.overlays(at)) - full * 0.5).abs() < 1e-4);
        s.calm = true;
        assert!(last_alpha(&s.overlays(at)) < full * 0.1);
        let peak = (0..20).map(|i| now + Duration::from_millis(i * 50)).map(|t| last_alpha(&s.overlays(t))).fold(0.0, f32::max);
        assert!(peak < full * 0.5);
        s.fx_off.insert(Fx::Kill);
        assert!(s.overlays(at).is_empty());
        s.fire_test(Fx::Kill, now);
        assert!(!s.overlays(now + Duration::from_millis(300)).is_empty());
    }

    #[test]
    fn catalogue_matches_the_board() {
        assert_eq!(Fx::ALL.len(), 21);
        assert!(Fx::ALL.iter().all(|f| Fx::from_id(f.id()) == Some(*f)));
        assert!(Fx::ALL.iter().all(|f| (1..=5).contains(&f.default_level())));
        let ids: Vec<_> = Group::ALL.iter().flat_map(|g| g.fx().map(Fx::id)).collect();
        assert_eq!(
            ids,
            [
                "draft", "horn", "daynight", "victory", "defeat", "lowhp", "respawn", "buyback", "ult", "spike", "aghs", "stunned", "silenced", "hexed",
                "kill", "multikill", "streak", "firstblood", "roshan", "roshwin", "buildings"
            ]
        );
        let unverified: Vec<_> = Fx::ALL.iter().filter(|f| !f.verified()).map(|f| f.id()).collect();
        assert_eq!(unverified, ["firstblood", "roshan", "roshwin", "buildings"]);
        assert_eq!(Fx::Buyback.color(), Rgb::from_hex("#FFD400").unwrap());
        let d = DotaState::with_defaults();
        assert!(d.fx_off.contains(&Fx::Aghs) && d.fx_off.contains(&Fx::Silenced) && d.fx_off.len() == 2);
        assert!(Base::ALL.iter().all(|b| Base::from_id(b.id()) == Some(*b)));
    }

    #[test]
    fn cfg_and_launch_options() {
        let dir = std::env::temp_dir().join(format!("roksync-dota-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(LEGACY_CFG_NAME), "old").unwrap();
        let path = install_cfg(&dir, DEFAULT_PORT, "abc123").unwrap();
        assert!(path.ends_with(CFG_NAME) && !dir.join(LEGACY_CFG_NAME).exists(), "the pre-rename cfg is replaced");
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("\"uri\"       \"http://127.0.0.1:3122/dota\""));
        assert!(body.contains("\"token\" \"abc123\""));
        assert!(body.contains("\"hero\"      \"1\""));
        let _ = std::fs::remove_dir_all(&dir);

        let vdf = r#""UserLocalConfigStore"
{
  "Software" { "Valve" { "Steam" { "apps" {
    "730" { "LaunchOptions" "-novid" }
    "570" { "LastPlayed" "1700000000" "LaunchOptions" "-novid -gamestateintegration" }
  } } } }
}"#;
        assert_eq!(vdf_launch_options(vdf, "570").as_deref(), Some("-novid -gamestateintegration"));
        assert_eq!(vdf_launch_options(vdf, "730").as_deref(), Some("-novid"));
        assert_eq!(vdf_launch_options(vdf, "440"), None);
    }
}
