//! Game detection and launch rules on their own thread. egui runs no frames while the window is
//! minimised or in the tray, so anything tied to frames would never see a game start.

use crate::config::{AppConfig, Mix, SoundCfg};
use crate::games_fx::FxSources;
use crate::perf::Perf;
use crate::screen_service::SharedScreen;
use crate::steam::Game;
use crate::sync::{SharedSync, SyncHandle};
use lumen_engine::Program;
use lumen_hue::{Bridge, Inventory};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct Detected {
    pub appid: u32,
    pub name: String,
    pub window: Option<(isize, String)>,
}

/// A game whose launch starts screen sync. `mode` None means guess from the game.
#[derive(Clone, Debug)]
pub struct Rule {
    pub appid: u32,
    pub mode: Option<String>,
    pub restore: bool,
    pub mix: Mix,
}

/// What going live needs, without the UI thread. Built from the config at startup and
/// refreshed by the UI whenever it runs.
#[derive(Clone)]
pub struct Plan {
    pub bridge: Bridge,
    pub client_key: String,
    pub target: Option<String>,
    pub prefer_area: Option<String>,
    pub program: Arc<Program>,
    pub fx: FxSources,
    pub eco_on_battery: bool,
    pub rules: Vec<Rule>,
    pub sound: SoundCfg,
    /// Native effects switched on per game (CS2, Dota 2, Call of Duty).
    pub games: [bool; 3],
}

pub fn plan(cfg: &AppConfig, bridge: Option<Bridge>, prefer_area: Option<String>, program: Program, fx: FxSources) -> Option<Plan> {
    let rules: Vec<Rule> = cfg
        .game_rules
        .iter()
        .filter(|(_, r)| r.sync)
        .map(|(&appid, r)| Rule { appid, mode: cfg.game_modes.get(&appid).cloned(), restore: r.restore, mix: crate::mix::for_game(r.mix) })
        .collect();
    if rules.is_empty() {
        return None;
    }
    Some(Plan {
        bridge: bridge?,
        client_key: cfg.bridge.as_ref()?.client_key.clone(),
        target: cfg.target.clone(),
        prefer_area,
        program: Arc::new(program),
        fx,
        eco_on_battery: cfg.eco_on_battery,
        rules,
        sound: cfg.sound.clone(),
        games: [cfg.cs2.enabled, cfg.dota.enabled, cfg.cod.enabled],
    })
}

#[derive(Debug, PartialEq)]
pub enum Event {
    Started { appid: u32, mode: String, area: String, mix: Mix },
    Stopped,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Start { appid: u32, name: String, mode: Option<String>, restore: bool, hwnd: Option<isize>, mix: Mix },
    Stop,
}

#[derive(Default)]
pub struct AutoLive {
    pub lib: Vec<Game>,
    pub plan: Option<Plan>,
    pub detected: Option<Detected>,
    /// A rule game that should go live; kept until it does, so a failed start is retried.
    pending: Option<u32>,
    /// The game a rule started sync for, and whether its quitting stops sync.
    started_for: Option<(u32, bool)>,
    pub events: Vec<Event>,
}

pub type SharedAutoLive = Arc<Mutex<AutoLive>>;
pub type SharedStream = Arc<Mutex<Option<SyncHandle>>>;
pub type InvSource = Arc<dyn Fn() -> Option<Arc<Inventory>> + Send + Sync>;

/// The first library game with a window whose title contains its name, ignoring case and
/// punctuation. Names under 4 characters are skipped; they match too much.
pub fn find_game(lib: &[Game], windows: &[(isize, String)]) -> Option<Detected> {
    let norm = |s: &str| s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>();
    lib.iter().find_map(|g| {
        let n = norm(&g.name);
        if n.len() < 4 {
            return None;
        }
        windows
            .iter()
            .find(|(_, t)| norm(t).contains(&n))
            .map(|w| Detected { appid: g.appid, name: g.name.clone(), window: Some(w.clone()) })
    })
}

impl AutoLive {
    /// Records the running game and returns what the launch rules want done. `streaming` is
    /// true while any stream is running or connecting; a rule never replaces or stops a stream
    /// it didn't start.
    pub fn step(&mut self, now: Option<Detected>, streaming: bool) -> Option<Action> {
        let was = self.detected.as_ref().map(|d| d.appid);
        let id = now.as_ref().map(|d| d.appid);
        self.detected = now.clone();
        if id != was {
            self.pending = None;
            if let Some((auto, restore)) = self.started_for {
                if id != Some(auto) {
                    self.started_for = None;
                    if restore {
                        return Some(Action::Stop);
                    }
                }
            }
            if let Some(d) = &now {
                let has_rule = self.plan.as_ref().is_some_and(|p| p.rules.iter().any(|r| r.appid == d.appid));
                if has_rule && !streaming {
                    self.pending = Some(d.appid);
                }
            }
        }
        let d = now?;
        if self.pending != Some(d.appid) {
            return None;
        }
        if streaming {
            self.pending = None;
            return None;
        }
        let rule = self.plan.as_ref()?.rules.iter().find(|r| r.appid == d.appid)?.clone();
        Some(Action::Start { appid: d.appid, name: d.name, mode: rule.mode, restore: rule.restore, hwnd: d.window.map(|w| w.0), mix: rule.mix })
    }

    pub fn started(&mut self, appid: u32, restore: bool, mode: String, area: String, mix: Mix) {
        self.pending = None;
        self.started_for = Some((appid, restore));
        self.events.push(Event::Started { appid, mode, area, mix });
    }
}

pub fn spawn(state: SharedAutoLive, stream: SharedStream, shared: SharedSync, screen: SharedScreen, inv: InvSource) {
    let _ = std::thread::Builder::new().name("auto-live".into()).spawn(move || {
        let mut lib_at: Option<Instant> = None;
        loop {
            if lib_at.map_or(true, |t| t.elapsed() > Duration::from_secs(300)) {
                lib_at = Some(Instant::now());
                let lib = crate::steam::installed();
                state.lock().unwrap().lib = lib;
            }
            let windows: Vec<(isize, String)> = lumen_screen::windows_list::list_with_minimized()
                .into_iter()
                .filter(|w| !w.title.starts_with("RokSync"))
                .map(|w| (w.hwnd, w.title))
                .collect();
            let streaming = {
                let s = shared.lock().unwrap();
                s.running || s.connecting
            };
            let (action, plan) = {
                let mut st = state.lock().unwrap();
                let now = find_game(&st.lib, &windows);
                (st.step(now, streaming), st.plan.clone())
            };
            match (action, plan) {
                (Some(Action::Start { appid, name, mode, restore, hwnd, mix }), Some(plan)) => {
                    if let Some((mode, area)) = go_live(&plan, appid, &name, mode, hwnd, mix, &inv, &stream, &shared, &screen) {
                        state.lock().unwrap().started(appid, restore, mode, area, mix);
                    }
                }
                (Some(Action::Stop), _) => {
                    let h = stream.lock().unwrap().take();
                    if let Some(mut h) = h {
                        h.stop();
                        state.lock().unwrap().events.push(Event::Stopped);
                    }
                }
                _ => {}
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
}

/// Starts screen sync for a game on the target's area. None when the inventory isn't loaded
/// yet or no area fits the target; the caller retries on its next tick.
#[allow(clippy::too_many_arguments)]
fn go_live(
    plan: &Plan,
    appid: u32,
    name: &str,
    mode: Option<String>,
    hwnd: Option<isize>,
    mix: Mix,
    inv: &InvSource,
    stream: &SharedStream,
    shared: &SharedSync,
    screen: &SharedScreen,
) -> Option<(String, String)> {
    let inv = inv()?;
    let t = crate::gui::target::resolve(&inv, plan.target.as_deref());
    let area = crate::gui::target::fit(&inv, &t, plan.prefer_area.as_deref()).area()?.clone();
    let mode = mode.unwrap_or_else(|| crate::modes::guess(appid, name).to_string());
    {
        let [cs2, dota, cod] = plan.games;
        let mut sh = shared.lock().unwrap();
        sh.cs2_enabled = mix.game && cs2;
        sh.dota_enabled = mix.game && dota;
        sh.cod_enabled = mix.game && cod;
        sh.sound = mix.sound.then(|| plan.sound.layer(mix.source));
    }
    if let Some(m) = crate::modes::find(&mode) {
        {
            let mut s = screen.lock().unwrap();
            m.apply_to(&mut s.settings);
            s.follow_window = hwnd;
        }
        shared.lock().unwrap().fx_gain = m.fx_gain;
        let eco = plan.eco_on_battery && crate::system::on_battery() == Some(true);
        crate::perf::set(if eco { Perf::Low } else { m.perf });
    }
    let area_id = area.id.clone();
    let h = SyncHandle::start(plan.bridge.clone(), plan.client_key.clone(), area, shared.clone(), plan.fx.clone(), screen.clone(), plan.program.clone(), mix.source);
    *stream.lock().unwrap() = Some(h);
    Some((mode, area_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> Vec<Game> {
        vec![Game { appid: 730, name: "Counter-Strike 2".into() }, Game { appid: 1, name: "GRIS".into() }]
    }

    fn with_cs2_rule() -> AutoLive {
        let plan = Plan {
            bridge: Bridge::new("127.0.0.1:9", "k").unwrap(),
            client_key: String::new(),
            target: None,
            prefer_area: None,
            program: Arc::new(Program::default()),
            fx: FxSources { cs2: Default::default(), dota: Default::default(), cod: Default::default(), sound: crate::sound::SoundCtl::idle() },
            eco_on_battery: false,
            rules: vec![Rule { appid: 730, mode: Some("competitive".into()), restore: true, mix: crate::mix::LiveMode::Game.mix() }],
            sound: Default::default(),
            games: [true, false, false],
        };
        AutoLive { lib: lib(), plan: Some(plan), ..Default::default() }
    }

    fn cs2(a: &AutoLive) -> Option<Detected> {
        find_game(&a.lib, &[(7, "Counter-Strike 2".to_string())])
    }

    fn start() -> Option<Action> {
        Some(Action::Start { appid: 730, name: "Counter-Strike 2".into(), mode: Some("competitive".into()), restore: true, hwnd: Some(7), mix: crate::mix::LiveMode::Game.mix() })
    }

    #[test]
    fn finds_a_game_by_its_window_title() {
        let w = vec![(1, "Steam".to_string()), (2, "Counter-Strike 2".to_string())];
        assert_eq!(find_game(&lib(), &w).map(|d| d.appid), Some(730));
        assert_eq!(find_game(&lib(), &[(1, "Home - File Explorer".to_string())]), None);
    }

    #[test]
    fn a_launch_rule_starts_and_stops_sync_with_the_game() {
        let mut a = with_cs2_rule();
        assert_eq!(a.step(cs2(&a), false), start());
        a.started(730, true, "competitive".into(), "area".into(), crate::mix::LiveMode::Game.mix());
        assert_eq!(a.step(cs2(&a), true), None);
        assert_eq!(a.step(None, true), Some(Action::Stop));
    }

    #[test]
    fn a_start_that_could_not_happen_is_retried() {
        let mut a = with_cs2_rule();
        assert_eq!(a.step(cs2(&a), false), start());
        assert_eq!(a.step(cs2(&a), false), start());
    }

    #[test]
    fn a_stream_the_rule_did_not_start_is_left_alone() {
        let mut a = with_cs2_rule();
        assert_eq!(a.step(cs2(&a), true), None);
        assert_eq!(a.step(cs2(&a), false), None);
        assert_eq!(a.step(None, true), None);
    }

    #[test]
    fn games_without_a_rule_only_get_detected() {
        let mut a = with_cs2_rule();
        let gris = find_game(&a.lib, &[(3, "GRIS".to_string())]);
        assert_eq!(a.step(gris, false), None);
        assert_eq!(a.detected.as_ref().map(|d| d.appid), Some(1));
    }
}
