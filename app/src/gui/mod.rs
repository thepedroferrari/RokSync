mod game_pages;
mod games;
mod home;
pub mod icons;
mod lights;
mod live_mix;
mod mood_page;
mod moods;
mod palettes;
mod settings;
mod setup;
mod sync_page;
pub mod target;
pub mod theme;
mod tray;

use crate::config::{AppConfig, BridgeCfg};
use crate::screen_service::{self, SharedScreen};
use crate::autolive::{SharedAutoLive, SharedStream};
use crate::sync::{SharedSync, SyncHandle, SyncShared};
use anyhow::Result;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Margin, Rect, RichText, Sense, Stroke, Vec2};
use icons::Icon;
use lumen_cs2::SharedCs2;
use lumen_hue::{discover::Found, Bridge, Inventory, LightState};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use theme::*;

pub static START_MINIMIZED: AtomicBool = AtomicBool::new(false);

pub fn run(cfg: AppConfig, notes: Vec<String>, demo: bool) -> Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(if demo { "RokSync · demo" } else { "RokSync" })
            .with_inner_size([1360.0, 900.0])
            .with_min_inner_size([1040.0, 640.0])
            .with_icon(egui::IconData { rgba: crate::icon_art::rgba(64, false), width: 64, height: 64 }),
        ..Default::default()
    };
    eframe::run_native("RokSync", opts, Box::new(move |cc| Ok(Box::new(LumenApp::new(cc, cfg, notes, demo)))))
        .map_err(|e| anyhow::anyhow!("{e}"))
}

pub fn local_hms() -> (u16, u16, u16) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::GetLocalTime;
        // SAFETY: GetLocalTime only writes the SYSTEMTIME we hand it.
        let st = unsafe {
            let mut st = std::mem::zeroed();
            GetLocalTime(&mut st);
            st
        };
        (st.wHour, st.wMinute, st.wSecond)
    }
    #[cfg(not(windows))]
    {
        let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        (((s / 3600) % 24) as u16, ((s / 60) % 60) as u16, (s % 60) as u16)
    }
}

pub fn clock_hm() -> String {
    let (h, m, _) = local_hms();
    format!("{h:02}:{m:02}")
}

pub fn clock_hms() -> String {
    let (h, m, s) = local_hms();
    format!("{h:02}:{m:02}:{s:02}")
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Page {
    Home,
    Lights,
    Palettes,
    Games,
    Settings,
    Game,
    Dota,
    Cod,
    Sync,
    Mood,
}

impl Page {
    pub const TABS: [Page; 5] = [Page::Home, Page::Lights, Page::Palettes, Page::Games, Page::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Page::Home | Page::Sync => "Home",
            Page::Lights => "Lights",
            Page::Palettes | Page::Mood => "Moods",
            Page::Games | Page::Game | Page::Dota | Page::Cod => "Games",
            Page::Settings => "Settings",
        }
    }

    pub fn tab(self) -> Page {
        match self {
            Page::Sync => Page::Home,
            Page::Mood => Page::Palettes,
            Page::Game | Page::Dota | Page::Cod => Page::Games,
            p => p,
        }
    }

    fn index(self) -> usize {
        Page::TABS.iter().position(|p| *p == self.tab()).unwrap_or(0)
    }
}

#[derive(Clone)]
pub struct Activity {
    pub clock: String,
    pub what: String,
    pub detail: String,
    pub color: Color32,
    pub err: bool,
}

#[derive(Default)]
pub struct Bg {
    pub inv: Option<Arc<Inventory>>,
    pub busy: usize,
    pub log: VecDeque<(Instant, String, bool)>,
    pub activity: VecDeque<Activity>,
    pub found: Vec<Found>,
    pub discovering: bool,
    pub pairing: bool,
    pub pair_status: String,
    pub paired: Option<BridgeCfg>,
    pub ping_ms: Option<u32>,
    pub link: Link,
    pub searched_at: Option<Instant>,
    /// The saved bridge answered at this address; the UI thread updates bridge.json and reconnects.
    pub relinked: Option<String>,
    pub link_note: String,
    pub moved_from: Option<String>,
}

/// The saved bridge's health. The pairing itself is never dropped because of any of these.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Link {
    #[default]
    Unknown,
    Ok,
    Unreachable,
    Searching,
}

impl Bg {
    pub fn push(&mut self, msg: String, err: bool) {
        if msg.is_empty() {
            return;
        }
        let (what, detail) = split_activity(&msg);
        self.activity.push_back(Activity {
            clock: clock_hm(),
            what,
            detail,
            color: if err { BAD } else { TEXT },
            err,
        });
        while self.activity.len() > 24 {
            self.activity.pop_front();
        }
        self.log.push_back((Instant::now(), msg, err));
        while self.log.len() > 50 {
            self.log.pop_front();
        }
    }

    pub fn note(&mut self, what: &str, detail: &str, color: Color32) {
        self.activity.push_back(Activity { clock: clock_hm(), what: what.to_uppercase(), detail: detail.to_string(), color, err: false });
        while self.activity.len() > 24 {
            self.activity.pop_front();
        }
    }
}

/// "Gradient applied to 5 lights" → ("GRADIENT", "applied to 5 lights"); quoted names lead.
fn split_activity(msg: &str) -> (String, String) {
    if let Some(rest) = msg.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return (rest[..end].to_uppercase(), rest[end + 1..].trim().to_string());
        }
    }
    match msg.split_once(": ") {
        Some((a, b)) if a.len() < 22 => (a.to_uppercase(), b.to_string()),
        _ => match msg.split_once(' ') {
            Some((a, b)) if a.len() < 16 => (a.to_uppercase(), b.to_string()),
            _ => ("ROKSYNC".into(), msg.to_string()),
        },
    }
}

pub enum Pending {
    Light(String, LightState),
    Group(String, LightState),
}

pub struct LumenApp {
    pub cfg: AppConfig,
    pub bridge: Option<Bridge>,
    pub demo: bool,
    pub bg: Arc<Mutex<Bg>>,
    pub ctx: egui::Context,
    pub started: Instant,
    logo: egui::TextureHandle,
    pub page: Page,
    last_page: Page,
    page_since: Instant,
    pub pending: Option<Pending>,
    pub inflight: Arc<AtomicBool>,
    pub lights: lights::LightsUi,
    pub sy: sync_page::SyncUi,
    pub sync_shared: SharedSync,
    pub sync: SharedStream,
    pub autolive: SharedAutoLive,
    autolive_at: Option<Instant>,
    pub cs2: SharedCs2,
    pub cs2_listening: bool,
    pub screen: SharedScreen,
    pub mood: mood_page::MoodUi,
    pub studio: palettes::Studio,
    pub games: games::GamesUi,
    pub home: home::HomeUi,
    pub moods: moods::MoodsUi,
    pub settings: settings::SettingsUi,
    pub target_ui: target::TargetUi,
    tray: Option<tray::Tray>,
    /// F pressed: start or stop screen sync on the next frame (needs the inventory).
    want_live_toggle: bool,
    panic_key_on: bool,
    first_frame_done: bool,
    pub fx_log: VecDeque<Activity>,
    last_fx: String,
    manual_ip: String,
    pub gfx: crate::games_fx::GamesFx,
    pub sound: crate::sound::SharedSound,
    pub setup: setup::SetupUi,
}

impl LumenApp {
    fn new(cc: &eframe::CreationContext<'_>, cfg: AppConfig, notes: Vec<String>, demo: bool) -> Self {
        theme::install(&cc.egui_ctx);
        let bridge = if demo { None } else { cfg.bridge.as_ref().and_then(|b| Bridge::new(&b.ip, &b.app_key).ok()) };
        let screen = screen_service::spawn();
        {
            let mut s = screen.lock().unwrap();
            s.settings = cfg.screen.settings.clone();
            if let Some(name) = &cfg.screen.monitor {
                s.monitor = lumen_screen::capture::monitors().ok().and_then(|m| m.into_iter().find(|m| &m.name == name));
            }
        }
        let sound = crate::sound::spawn(cfg.sound.device.clone());
        let sync_shared: SharedSync = Arc::new(Mutex::new(SyncShared { source: cfg.source, fx_gain: cfg.fx_gain, ..Default::default() }));
        let mut app = Self {
            sy: sync_page::SyncUi::new(&cfg),
            mood: mood_page::MoodUi::new(&cfg),
            studio: palettes::Studio::default(),
            games: games::GamesUi::default(),
            home: home::HomeUi::default(),
            moods: moods::MoodsUi::default(),
            settings: settings::SettingsUi::default(),
            target_ui: target::TargetUi::default(),
            tray: if std::env::var("LUMEN_SHOTS").is_ok() { None } else { tray::Tray::new(cc) },
            want_live_toggle: false,
            panic_key_on: false,
            first_frame_done: false,
            cfg,
            bridge,
            demo,
            bg: Arc::new(Mutex::new(Bg::default())),
            ctx: cc.egui_ctx.clone(),
            started: Instant::now(),
            logo: load_logo(&cc.egui_ctx),
            page: Page::Home,
            last_page: Page::Home,
            page_since: Instant::now(),
            pending: None,
            inflight: Arc::new(AtomicBool::new(false)),
            lights: lights::LightsUi::default(),
            sync_shared,
            sync: Arc::new(Mutex::new(None)),
            autolive: Default::default(),
            autolive_at: None,
            cs2: SharedCs2::default(),
            cs2_listening: false,
            screen,
            fx_log: VecDeque::new(),
            last_fx: String::new(),
            manual_ip: String::new(),
            gfx: Default::default(),
            sound,
            setup: Default::default(),
        };
        crate::games_fx::seed_defaults(&mut app.cfg);
        app.push_game_fx();
        crate::games_fx::load_cod_calibration(&app.cfg.cod, &mut app.gfx.cod.lock().unwrap());
        if demo {
            let mut bg = app.bg.lock().unwrap();
            bg.inv = Some(Arc::new(crate::demo::inventory()));
            bg.ping_ms = Some(12);
            for (what, detail, c) in [
                ("Hallway", "Turned off", EDGE),
                ("Living room", "Mood set to Savanna sunset", hex("#FF6A00")),
                ("Screen sync", "Stopped after 1 h 40 min", ACCENT),
                ("Bedroom", "Dimmed to 22%", hex("#FF7A3D")),
                ("Battlestation", "Mood set to Night raid", hex("#FF2BD6")),
            ] {
                bg.note(what, detail, c);
            }
            drop(bg);
            app.games.demo_running = true;
        }
        {
            let mut bg = app.bg.lock().unwrap();
            for n in notes {
                bg.push(n, true);
            }
            if demo {
                bg.link = Link::Ok;
            }
        }
        if demo {
        } else if app.bridge.is_some() {
            app.refresh();
        } else {
            app.discover();
        }
        if !demo && app.cfg.cs2.enabled && !app.cfg.cs2.token.is_empty() {
            app.start_cs2_listener();
        }
        app.cfg.cs2.push_to(&mut app.cs2.lock().unwrap());
        if !demo {
            let plan = crate::autolive::plan(&app.cfg, app.bridge.clone(), app.sy.area.clone(), app.sy.program(), app.fx_sources());
            app.autolive.lock().unwrap().plan = plan;
            let bg = app.bg.clone();
            let inv: crate::autolive::InvSource = Arc::new(move || bg.lock().unwrap().inv.clone());
            crate::autolive::spawn(app.autolive.clone(), app.sync.clone(), app.sync_shared.clone(), app.screen.clone(), inv);
        }
        // a second launch while running shows this window instead
        if !demo {
            match &app.tray {
                Some(t) => crate::instance::on_show(t.shower(cc.egui_ctx.clone())),
                None => {
                    let ctx = cc.egui_ctx.clone();
                    crate::instance::on_show(move || {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    });
                }
            }
        }
        app
    }

    fn model_tick(&self) -> (String, Color32) {
        match self.active_model() {
            Some(_) => {
                let st = self.model_status();
                let c = match st {
                    "ready" => GOOD,
                    "asleep" => EDGE,
                    _ => WARN,
                };
                (st.to_uppercase(), c)
            }
            None => ("QUICK MODE".into(), EDGE),
        }
    }

    pub fn save_cfg(&mut self) {
        if self.demo {
            return;
        }
        if let Err(e) = self.cfg.save() {
            self.bg.lock().unwrap().push(format!("Settings: couldn't save ({e:#})"), true);
        }
    }

    pub fn toast(&self, msg: impl Into<String>, err: bool) {
        self.bg.lock().unwrap().push(msg.into(), err);
    }

    /// Change the cached inventory right away, so the UI answers before the bridge does.
    pub fn local_edit(&self, f: impl FnOnce(&mut Inventory)) {
        let mut bg = self.bg.lock().unwrap();
        if let Some(inv) = &bg.inv {
            let mut next = (**inv).clone();
            f(&mut next);
            bg.inv = Some(Arc::new(next));
        }
    }

    pub fn task<F>(&self, label: &'static str, refresh_after: bool, f: F)
    where
        F: FnOnce(&Bridge) -> Result<String> + Send + 'static,
    {
        let Some(b) = self.bridge.clone() else { return };
        let bg = self.bg.clone();
        let ctx = self.ctx.clone();
        bg.lock().unwrap().busy += 1;
        std::thread::spawn(move || {
            let r = f(&b);
            let t0 = Instant::now();
            let inv = if refresh_after { Some(b.inventory()) } else { None };
            let ms = t0.elapsed().as_millis() as u32;
            let mut s = bg.lock().unwrap();
            s.busy -= 1;
            match r {
                Ok(m) => s.push(m, false),
                Err(e) => s.push(format!("{label}: {e:#}"), true),
            }
            match inv {
                Some(Ok(i)) => {
                    s.inv = Some(Arc::new(i));
                    s.ping_ms = Some(ms);
                    s.link = Link::Ok;
                    s.link_note.clear();
                }
                Some(Err(e)) => {
                    if s.link != Link::Unreachable && s.link != Link::Searching {
                        s.push(format!("Bridge: not answering ({e:#})"), true);
                    }
                    if s.link != Link::Searching {
                        s.link = Link::Unreachable;
                    }
                    s.link_note = format!("{e:#}");
                }
                None => {}
            }
            ctx.request_repaint();
        });
    }

    pub fn refresh(&self) {
        self.task("Refresh", true, |_| Ok(String::new()));
    }

    /// Scan the network for the saved bridge ID (a bridge can get a new IP after a router
    /// restart). The existing app key stays valid, so no link-button press is needed.
    pub fn relink(&self) {
        let Some(saved) = self.cfg.bridge.clone() else { return };
        if self.demo {
            return;
        }
        let bg = self.bg.clone();
        let ctx = self.ctx.clone();
        {
            let mut s = bg.lock().unwrap();
            if s.link == Link::Searching {
                return;
            }
            s.link = Link::Searching;
            s.searched_at = Some(Instant::now());
            s.link_note = "Looking for your bridge on the network…".into();
        }
        ctx.request_repaint();
        std::thread::spawn(move || {
            let mut ips: Vec<String> = vec![saved.ip.clone()];
            for f in lumen_hue::discover::discover(Duration::from_secs(4)) {
                if !ips.contains(&f.ip) {
                    ips.push(f.ip);
                }
            }
            let hit = ips.into_iter().find(|ip| {
                lumen_hue::pair::bridge_info(ip).map(|i| i.bridge_id.eq_ignore_ascii_case(&saved.bridge_id)).unwrap_or(false)
            });
            let mut s = bg.lock().unwrap();
            match hit {
                Some(ip) => {
                    s.relinked = Some(ip);
                    s.link = Link::Unknown;
                }
                None => {
                    s.link = Link::Unreachable;
                    s.link_note = "Not found yet. Check the bridge is powered and on the same network as this PC.".into();
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn relink_to(&self, ip: String) {
        let Some(saved) = self.cfg.bridge.clone() else { return };
        let bg = self.bg.clone();
        let ctx = self.ctx.clone();
        bg.lock().unwrap().link_note = format!("Checking {ip}…");
        std::thread::spawn(move || {
            let r = lumen_hue::pair::bridge_info(&ip);
            let mut s = bg.lock().unwrap();
            match r {
                Ok(i) if i.bridge_id.eq_ignore_ascii_case(&saved.bridge_id) => s.relinked = Some(ip),
                Ok(i) => s.link_note = format!("{ip} is another bridge (\"{}\", {}). Yours is {}.", i.name, i.bridge_id, saved.bridge_id),
                Err(e) => s.link_note = format!("No bridge at {ip}: {e:#}"),
            }
            ctx.request_repaint();
        });
    }

    /// Deletes bridge.json; the only code path that removes the pairing.
    pub fn forget_bridge(&mut self) {
        self.stop_live();
        if !self.demo {
            if let Err(e) = BridgeCfg::forget() {
                self.toast(format!("Bridge: couldn't forget the pairing ({e:#})"), true);
                return;
            }
        }
        self.cfg.bridge = None;
        self.bridge = None;
        self.save_cfg(); // drops the mirror copy in config.json too
        let mut bg = self.bg.lock().unwrap();
        bg.inv = None;
        bg.link = Link::Unknown;
        drop(bg);
        if !self.demo {
            self.discover();
        }
    }

    /// Apply a found new address, keep the pairing, reconnect.
    fn bridge_housekeeping(&mut self) {
        if self.demo {
            return;
        }
        let (relinked, link, searched_at) = {
            let mut s = self.bg.lock().unwrap();
            (s.relinked.take(), s.link, s.searched_at)
        };
        if let Some(ip) = relinked {
            if let Some(mut b) = self.cfg.bridge.clone() {
                let moved = b.ip != ip;
                if moved {
                    self.bg.lock().unwrap().moved_from = Some(b.ip.clone());
                }
                b.ip = ip.clone();
                if moved {
                    if let Err(e) = b.save() {
                        self.toast(format!("Bridge: couldn't save the new address ({e:#})"), true);
                    }
                }
                self.bridge = Bridge::new(&b.ip, &b.app_key).ok();
                self.cfg.bridge = Some(b);
                self.save_cfg();
                let msg = if moved { format!("Found at new address {ip}") } else { "Back online".into() };
                self.bg.lock().unwrap().note("Bridge", &msg, GOOD);
                self.refresh();
            }
            return;
        }
        if link == Link::Unreachable && self.bridge.is_some() && searched_at.map(|t| t.elapsed() > Duration::from_secs(30)).unwrap_or(true) {
            self.relink();
        }
    }

    fn discover(&self) {
        let bg = self.bg.clone();
        let ctx = self.ctx.clone();
        bg.lock().unwrap().discovering = true;
        std::thread::spawn(move || {
            let found = lumen_hue::discover::discover(Duration::from_secs(4));
            let mut s = bg.lock().unwrap();
            s.found = found;
            s.discovering = false;
            ctx.request_repaint();
        });
    }

    fn start_pair(&self, ip: String) {
        let bg = self.bg.clone();
        let ctx = self.ctx.clone();
        {
            let mut s = bg.lock().unwrap();
            s.pairing = true;
            s.pair_status = format!("Contacting {ip}…");
        }
        std::thread::spawn(move || {
            let set = |m: String| {
                bg.lock().unwrap().pair_status = m;
                ctx.request_repaint();
            };
            let info = match lumen_hue::pair::bridge_info(&ip) {
                Ok(i) => i,
                Err(e) => {
                    set(format!("{e:#}"));
                    bg.lock().unwrap().pairing = false;
                    return;
                }
            };
            set(format!("Found \"{}\". Press the link button on top of the bridge…", info.name));
            let deadline = Instant::now() + Duration::from_secs(60);
            while Instant::now() < deadline {
                match lumen_hue::pair::try_pair(&ip, "lumen#windows") {
                    Ok(Some(r)) => {
                        let mut s = bg.lock().unwrap();
                        s.paired = Some(BridgeCfg {
                            ip: ip.clone(),
                            bridge_id: info.bridge_id.clone(),
                            name: info.name.clone(),
                            app_key: r.app_key,
                            client_key: r.client_key,
                        });
                        s.pairing = false;
                        s.pair_status = "Paired".into();
                        ctx.request_repaint();
                        return;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        set(format!("{e:#}"));
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(1500));
            }
            let mut s = bg.lock().unwrap();
            s.pairing = false;
            if !s.pair_status.starts_with("Paired") {
                s.pair_status = "No button press within a minute. Try again.".into();
            }
        });
    }

    pub fn start_cs2_listener(&mut self) {
        if self.cs2_listening || self.demo {
            return;
        }
        match lumen_cs2::spawn_server(self.cfg.cs2.port, self.cfg.cs2.token.clone(), self.cs2.clone()) {
            Ok(()) => self.cs2_listening = true,
            Err(e) => self.toast(format!("CS2: can't receive game events ({e:#})"), true),
        }
    }

    /// Sends only the latest pending state, one request in flight at a time, so slider drags
    /// don't queue up on the bridge.
    fn pump_pending(&mut self) {
        if self.pending.is_none() {
            return;
        }
        if self.demo {
            self.pending = None;
            return;
        }
        if self.inflight.load(Ordering::Relaxed) {
            self.ctx.request_repaint_after(Duration::from_millis(40));
            return;
        }
        let Some(b) = self.bridge.clone() else { return };
        let p = self.pending.take().unwrap();
        let inflight = self.inflight.clone();
        let bg = self.bg.clone();
        inflight.store(true, Ordering::Relaxed);
        std::thread::spawn(move || {
            let r = match &p {
                Pending::Light(id, s) => b.set_light(id, s),
                Pending::Group(id, s) => b.set_grouped_light(id, s),
            };
            if let Err(e) = r {
                bg.lock().unwrap().push(format!("{e:#}"), true);
            }
            std::thread::sleep(Duration::from_millis(90)); // stay under the bridge's ~10 cmd/s
            inflight.store(false, Ordering::Relaxed);
        });
    }

    pub fn is_streaming(&self) -> bool {
        let s = self.sync_shared.lock().unwrap();
        s.running || s.connecting
    }

    pub fn go_live(&mut self, inv: &Inventory) {
        if self.is_streaming() {
            return;
        }
        if self.demo {
            self.toast("Demo: screen sync needs a real bridge", false);
            return;
        }
        // only an area wholly inside the target, so sync never lights a room outside it
        let t = self.target(inv);
        let fit = target::fit(inv, &t, self.sy.area.as_deref());
        let area = fit.area().cloned();
        if area.is_none() {
            let why = match &fit {
                target::Fit::Spills { area, rooms, .. } => format!("No sync in {}: {} also covers {}", t.name, area.metadata.name.trim(), target::join(rooms)),
                target::Fit::Home => "Sync: pick a room or zone, not the whole home".into(),
                _ => format!("{} has no entertainment area. Make one in the Hue app.", t.name),
            };
            self.toast(why, true);
            return;
        }
        match (area, self.bridge.clone(), self.cfg.bridge.clone()) {
            (Some(a), Some(b), Some(bc)) => {
                self.sy.area = Some(a.id.clone());
                self.cfg.last_area = Some(a.id.clone());
                self.save_cfg();
                self.bg.lock().unwrap().note("Screen sync", &format!("Live on {}", a.metadata.name.trim()), ACCENT);
                *self.sync.lock().unwrap() = Some(SyncHandle::start(
                    b,
                    bc.client_key,
                    a,
                    self.sync_shared.clone(),
                    self.fx_sources(),
                    self.screen.clone(),
                    Arc::new(self.sy.program()),
                    self.sy.source,
                ));
            }
            _ => self.toast("No entertainment area on this bridge yet. Create one in the Hue app.", true),
        }
    }

    pub fn stop_live(&mut self) {
        let handle = self.sync.lock().unwrap().take();
        if let Some(mut s) = handle {
            s.stop();
            self.bg.lock().unwrap().note("Screen sync", "Stopped", ACCENT);
        }
    }

    pub fn apply_mode(&mut self, id: &str) {
        let Some(m) = crate::modes::find(id) else { return };
        m.apply_to(&mut self.sy.screen);
        self.cfg.screen.settings = self.sy.screen.clone();
        self.cfg.game_mode = Some(m.id.to_string());
        self.cfg.fx_gain = m.fx_gain;
        self.sy.source = crate::sync::Source::Screen;
        self.cfg.source = crate::sync::Source::Screen;
        self.set_perf(m.perf);
        self.save_cfg();
        self.toast(format!("{} mode · {} preset", m.name, m.perf.label()), false);
    }

    fn track_fx(&mut self) {
        let first = |s: &str| s.to_lowercase().split(' ').next().unwrap_or("").to_string();
        let cs2 = self.cs2.lock().unwrap().last_event.clone();
        let cs2_c = lumen_cs2::Fx::ALL.iter().find(|f| cs2.to_lowercase().contains(&first(f.label()))).map(|f| f.color());
        let dota = self.gfx.dota.lock().unwrap().last_event.clone();
        let dota_c = lumen_dota::Fx::ALL.iter().find(|f| dota.to_lowercase().contains(&first(f.label()))).map(|f| f.color());
        let cod = self.gfx.cod.lock().unwrap().last_event.clone();
        let cod_c = lumen_cod::Fx::ALL.iter().find(|f| cod.to_lowercase().contains(&first(f.label()))).map(|f| f.color());
        for (game, last, c) in [("CS2", cs2, cs2_c), ("DOTA 2", dota, dota_c), ("COD", cod, cod_c)] {
            let key = format!("{game}:{last}");
            if last.is_empty() || self.last_fx.split('\n').any(|k| k == key) {
                continue;
            }
            // last_fx holds one "GAME:event" key per game, newline-separated
            let mut keys: Vec<String> = self.last_fx.split('\n').filter(|k| !k.starts_with(&format!("{game}:")) && !k.is_empty()).map(String::from).collect();
            keys.push(key);
            self.last_fx = keys.join("\n");
            let color = c.map(c32).unwrap_or(TEXT);
            self.fx_log.push_back(Activity { clock: clock_hms(), what: last.to_uppercase(), detail: game.to_string(), color, err: false });
            while self.fx_log.len() > 24 {
                self.fx_log.pop_front();
            }
        }
    }

    fn status_bar(&mut self, ctx: &egui::Context, inv: Option<&Inventory>) {
        let frame = egui::Frame::none().fill(BG);
        egui::TopBottomPanel::top("status").exact_height(44.0).frame(frame).show_separator_line(false).show(ctx, |ui| {
            let full = ui.max_rect();
            let p = ui.painter().clone();
            rule(&p, full.bottom() - 1.0, full.left(), full.right());
            let mut x = full.left();
            let size = self.logo.size_vec2() * (30.0 / self.logo.size_vec2().y);
            let lr = Rect::from_min_size(pos2(x + 14.0, full.center().y - size.y / 2.0), size);
            p.image(self.logo.id(), lr, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            x = lr.right() + 14.0;
            p.line_segment([pos2(x, full.top()), pos2(x, full.bottom() - 1.0)], Stroke::new(1.0_f32, RULE));
            let streaming = self.is_streaming();
            let t = anim_time(ui);
            if streaming {
                let g = p.layout_job(spaced("ON AIR", display(15.0), Color32::PLACEHOLDER, 0.6));
                let r = Rect::from_min_max(pos2(x, full.top()), pos2(x + g.size().x + 54.0, full.bottom() - 1.0));
                p.rect_filled(r, 0.0, ACCENT);
                let blink = if motion() && (t * 1.25).fract() > 0.5 { 0.25 } else { 1.0 };
                p.rect_filled(Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), Vec2::splat(8.0)), 0.0, INK.gamma_multiply(blink));
                p.galley(pos2(r.left() + 34.0, r.center().y - g.size().y / 2.0), g, INK);
                x = r.right();
                if motion() {
                    ui.ctx().request_repaint_after(Duration::from_millis(400));
                }
            }
            for item in self.ticker(inv) {
                x = ticker_item(&p, x, full, &item);
            }
            let (h, m, s) = local_hms();
            let colon = if motion() && s % 2 == 1 { MUTED.gamma_multiply(0.35) } else { MUTED };
            let right = full.right() - 20.0;
            let gm = p.layout_job(spaced(&format!("{m:02}"), display(15.0), Color32::PLACEHOLDER, 0.6));
            let gc = p.layout_job(spaced(":", display(15.0), Color32::PLACEHOLDER, 0.6));
            let gh = p.layout_job(spaced(&format!("{h:02}"), display(15.0), Color32::PLACEHOLDER, 0.6));
            let y = full.center().y - gm.size().y / 2.0;
            let xm = right - gm.size().x;
            let xc = xm - gc.size().x;
            let xh = xc - gh.size().x;
            p.galley(pos2(xh, y), gh, MUTED);
            p.galley(pos2(xc, y), gc, colon);
            p.galley(pos2(xm, y), gm, MUTED);
            if self.demo {
                text_at(&p, pos2(xh - 18.0, full.center().y), Align2::RIGHT_CENTER, "DEMO · NOTHING IS SENT", display(15.0), ACCENT, 0.6);
            }
        });
    }

    fn ticker(&self, inv: Option<&Inventory>) -> Vec<Tick> {
        let mut out = Vec::new();
        let detected = self.games.detected_name();
        match self.page {
            Page::Games => {
                let n = self.games.library_len();
                out.push(Tick::chip("BRIDGE", self.bridge_label(), self.bridge_color()));
                out.push(Tick::plain("LIBRARY", &format!("{n} GAMES · STEAM")));
                out.push(match &detected {
                    Some(g) => Tick::chip("RUNNING", &g.to_uppercase(), INFO),
                    None => Tick::chip("RUNNING", "NOTHING", EDGE),
                });
            }
            Page::Game => {
                let (on, total) = self.fx_counts();
                let now = Instant::now();
                let (connected, last) = {
                    let s = self.cs2.lock().unwrap();
                    (s.connected(now), s.last_msg)
                };
                out.push(if self.cfg.cs2.enabled && connected {
                    Tick::block("RECEIVING", GOOD)
                } else if self.cfg.cs2.enabled {
                    Tick::block("WAITING", SURFACE3)
                } else {
                    Tick::block("PAUSED", SURFACE3)
                });
                out.push(Tick::plain("GAME", "COUNTER-STRIKE 2"));
                out.push(Tick::plain("EFFECTS ON", &format!("{on} / {total}")));
                let ago = last.map(|t| format!("{} S AGO", t.elapsed().as_secs())).unwrap_or_else(|| "—".into());
                out.push(Tick::plain("LAST EVENT", &ago));
            }
            Page::Dota => {
                let (on, total) = self.dota_counts();
                let now = Instant::now();
                let (connected, clock, in_game) = {
                    let s = self.gfx.dota.lock().unwrap();
                    (s.connected(now), s.clock(now), s.in_game())
                };
                let enabled = self.cfg.dota.enabled && !self.cfg.fx_paused;
                out.push(if self.demo {
                    Tick::block("DEMO", SURFACE3)
                } else if enabled && connected {
                    Tick::block("RECEIVING", GOOD)
                } else if enabled {
                    Tick::block("WAITING", SURFACE3)
                } else {
                    Tick::block("PAUSED", SURFACE3)
                });
                out.push(Tick::plain("GAME", "DOTA 2"));
                out.push(Tick::plain("EFFECTS ON", &format!("{on} / {total}")));
                let c = clock.max(0.0) as u32;
                out.push(Tick::plain("MATCH CLOCK", &if in_game { format!("{}:{:02}", c / 60, c % 60) } else { "—".into() }));
            }
            Page::Cod => {
                let (on, total) = self.cod_counts();
                let (paused, cal) = {
                    let s = self.gfx.cod.lock().unwrap();
                    (s.paused(), s.calibration.short_status())
                };
                let enabled = self.cfg.cod.enabled && !self.cfg.fx_paused;
                out.push(if self.demo {
                    Tick::block("DEMO", SURFACE3)
                } else if enabled && !paused {
                    Tick::block("WATCHING", GOOD)
                } else if enabled {
                    Tick::block("WAITING FOR COD", SURFACE3)
                } else {
                    Tick::block("PAUSED", SURFACE3)
                });
                out.push(Tick::plain("GAME", "CALL OF DUTY"));
                out.push(Tick::plain("EFFECTS ON", &format!("{on} / {total}")));
                out.push(Tick::plain("CALIBRATION", &cal));
            }
            Page::Palettes | Page::Mood => {
                out.push(Tick::chip("BRIDGE", self.bridge_label(), self.bridge_color()));
                out.push(Tick::plain("MOODS", &format!("{} SAVED", self.cfg.palettes.len() + inv.map(|i| i.scenes.len()).unwrap_or(0))));
                let (label, c) = self.model_tick();
                out.push(Tick::chip("LOCAL MODEL", &label, c));
            }
            _ => {
                out.push(Tick::chip("BRIDGE", self.bridge_label(), self.bridge_color()));
                if let Some(inv) = inv {
                    let on = inv.lights.iter().filter(|l| l.on.on).count();
                    out.push(Tick::unit("LIGHTS", &on.to_string(), &format!("/{} ON", inv.lights.len()), TEXT));
                }
                let running = self.sync_shared.lock().unwrap().running;
                if running {
                    let hz = crate::perf::profile().stream_hz;
                    out.push(Tick::unit("SCREEN SYNC", &format!("LIVE · {hz}"), " HZ", ACCENT));
                } else {
                    out.push(Tick::chip("SCREEN SYNC", "OFF", EDGE));
                }
                out.push(match &detected {
                    Some(g) => Tick::chip("GAME", &g.to_uppercase(), INFO),
                    None => Tick::chip("GAME", "NONE", EDGE),
                });
            }
        }
        out
    }

    fn bridge_label(&self) -> &'static str {
        let link = self.bg.lock().unwrap().link;
        if self.demo {
            "DEMO"
        } else if self.bridge.is_none() {
            "NOT LINKED"
        } else {
            match link {
                Link::Ok => "CONNECTED",
                Link::Unknown => "CONNECTING",
                Link::Unreachable => "OFFLINE",
                Link::Searching => "SEARCHING",
            }
        }
    }

    fn bridge_color(&self) -> Color32 {
        let link = self.bg.lock().unwrap().link;
        if self.demo {
            GOOD
        } else if self.bridge.is_none() {
            BAD
        } else {
            match link {
                Link::Ok => GOOD,
                Link::Unknown | Link::Searching => WARN,
                Link::Unreachable => BAD,
            }
        }
    }

    fn offline_view(&mut self, ui: &mut egui::Ui) {
        let (link, note) = {
            let s = self.bg.lock().unwrap();
            (s.link, s.link_note.clone())
        };
        let Some(saved) = self.cfg.bridge.clone() else { return };
        ui.add_space(24.0);
        ui.vertical(|ui| {
            ui.set_max_width(700.0);
            kicker(ui, "Bridge");
            ui.label(headline(if link == Link::Searching { "Looking for your bridge" } else { "Bridge not answering" }, 76.0));
            ui.label(
                RichText::new(format!(
                    "Still paired with \"{}\". Check it's powered and on the same network as this PC.",
                    saved.name
                ))
                .font(body(15.0))
                .color(SOFT),
            );
            ui.add_space(18.0);
            for (k, v) in [("LAST ADDRESS", saved.ip.as_str()), ("BRIDGE ID", saved.bridge_id.as_str())] {
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
                row_rule(ui.painter(), row.bottom(), row.left(), row.right());
                text_at(ui.painter(), pos2(row.left(), row.center().y), Align2::LEFT_CENTER, k, display(14.0), MUTED, 1.0);
                text_at(ui.painter(), pos2(row.right(), row.center().y), Align2::RIGHT_CENTER, v, display(18.0), TEXT, 0.5);
            }
            ui.add_space(16.0);
            if !note.is_empty() {
                ui.label(RichText::new(note).color(if link == Link::Searching { ACCENT } else { MUTED }));
                ui.add_space(12.0);
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if ui.add_enabled(link != Link::Searching, primary("Retry now").height(44.0)).clicked() {
                    self.bg.lock().unwrap().link = Link::Unknown;
                    self.refresh();
                    self.relink();
                }
                ui.add_space(16.0);
                ui.add(text_input(&mut self.manual_ip, "New address, e.g. 192.168.1.20").desired_width(260.0));
                if ui.add_enabled(!self.manual_ip.trim().is_empty(), ghost("Use address").height(44.0)).clicked() {
                    self.relink_to(self.manual_ip.trim().to_string());
                }
            });
            if link == Link::Searching {
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            }
        });
    }

    fn nav(&mut self, ctx: &egui::Context) {
        let frame = egui::Frame::none().fill(BG).inner_margin(Margin { left: PAD_X, right: PAD_X, top: 16.0, bottom: 0.0 });
        egui::TopBottomPanel::top("nav").exact_height(64.0).frame(frame).show_separator_line(false).show(ctx, |ui| {
            let full = ui.max_rect();
            let p = ui.painter().clone();
            rule(&p, full.bottom() - 1.0, full.left() - PAD_X, full.right() + PAD_X);
            let mut x = full.left();
            let cur = self.page.tab();
            for page in Page::TABS {
                let label = page.label().to_uppercase();
                let g = p.layout_job(spaced(&label, display(22.0), Color32::PLACEHOLDER, 0.5));
                let r = Rect::from_min_max(pos2(x, full.top()), pos2(x + g.size().x, full.bottom()));
                let resp = ui.interact(r, ui.id().with(("tab", label.as_str())), Sense::click());
                let sel = cur == page;
                let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
                let col = if sel { TEXT } else { mix(MUTED, TEXT, hov) };
                p.galley(pos2(r.left(), r.top() + (r.height() - 12.0 - g.size().y) / 2.0), g, col);
                if sel {
                    p.rect_filled(Rect::from_min_max(pos2(r.left(), r.bottom() - 5.0), pos2(r.right(), r.bottom() - 1.0)), 0.0, ACCENT);
                }
                if resp.clicked() {
                    self.page = page;
                }
                x = r.right() + 34.0;
            }
            let label = format!("PERFORMANCE · {}", self.cfg.perf.label().to_uppercase());
            let g = p.layout_job(spaced(&label, display(15.0), Color32::PLACEHOLDER, 1.0));
            let r = Rect::from_min_max(pos2(full.right() - g.size().x, full.top()), pos2(full.right(), full.bottom() - 12.0));
            let resp = ui.interact(r, ui.id().with("perf"), Sense::click()).on_hover_text("How much of your PC RokSync may use");
            let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
            p.galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, mix(MUTED, TEXT, hov));
            let id = ui.make_persistent_id("perf-popup");
            if resp.clicked() {
                ui.memory_mut(|m| m.toggle_popup(id));
            }
            let mut pick = None;
            egui::popup_below_widget(ui, id, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                ui.set_min_width(220.0);
                for q in crate::perf::Perf::ALL {
                    if ui.add(ghost(q.label()).selected(q == self.cfg.perf).min_size(vec2(220.0, 40.0))).clicked() {
                        pick = Some(q);
                    }
                }
            });
            if let Some(q) = pick {
                self.set_perf(q);
            }
        });
    }

    fn crawl(&mut self, ctx: &egui::Context) {
        let game = matches!(self.page, Page::Game | Page::Dota | Page::Cod) || (self.page == Page::Sync && !self.fx_log.is_empty());
        let items: Vec<Activity> = if game { self.fx_log.iter().cloned().collect() } else { self.bg.lock().unwrap().activity.iter().cloned().collect() };
        let frame = egui::Frame::none().fill(BG);
        egui::TopBottomPanel::bottom("crawl").exact_height(40.0).frame(frame).show_separator_line(false).show(ctx, |ui| {
            let full = ui.max_rect();
            let p = ui.painter().clone();
            rule(&p, full.top(), full.left(), full.right());
            let (label, fill) = if game { ("GAME FX", ACCENT) } else { ("ACTIVITY", TEXT) };
            let g = p.layout_job(spaced(label, display_bold(15.0), Color32::PLACEHOLDER, 1.2));
            let lr = Rect::from_min_max(pos2(full.left(), full.top() + 1.0), pos2(full.left() + g.size().x + 32.0, full.bottom()));
            p.rect_filled(lr, 0.0, fill);
            p.galley(lr.center() - g.size() / 2.0, g, INK);
            let rr = Rect::from_min_max(pos2(full.right(), full.top() + 1.0), full.max);
            let feed = Rect::from_min_max(pos2(lr.right(), full.top() + 1.0), pos2(rr.left(), full.bottom()));
            let resp = ui.interact(feed, ui.id().with("feed"), Sense::hover());
            if items.is_empty() {
                let msg = if game { "Waiting for game events" } else { "Nothing yet. Your changes show up here." };
                text_at(&p, pos2(feed.left() + 22.0, feed.center().y), Align2::LEFT_CENTER, msg, medium(14.0), MUTED, 0.0);
                return;
            }
            // newest first, repeated so the scroll loops seamlessly
            let fp = p.with_clip_rect(feed);
            let lay: Vec<_> = items
                .iter()
                .rev()
                .map(|a| {
                    let gt = fp.layout_job(spaced(&a.clock, display(15.0), Color32::PLACEHOLDER, 0.6));
                    let ge = fp.layout_job(spaced(&a.what, display(15.0), Color32::PLACEHOLDER, 0.6));
                    let gd = fp.layout_job(spaced(&a.detail, medium(14.0), Color32::PLACEHOLDER, 0.0));
                    let w = 22.0 + gt.size().x + 10.0 + 8.0 + 10.0 + ge.size().x + 10.0 + gd.size().x + 22.0;
                    (a.color, a.err, gt, ge, gd, w)
                })
                .collect();
            let total: f32 = lay.iter().map(|l| l.5).sum();
            let paused = resp.hovered() || !motion();
            let off_id = ui.id().with("crawl-off");
            let mut off: f32 = ui.ctx().data(|d| d.get_temp(off_id)).unwrap_or(0.0);
            if !paused && total > feed.width() * 0.6 {
                off += ui.input(|i| i.stable_dt).min(0.1) * 26.0;
                off %= total;
                ui.ctx().request_repaint_after(Duration::from_millis(33));
            }
            ui.ctx().data_mut(|d| d.insert_temp(off_id, off));
            let reps = if total > feed.width() * 0.6 { 2 + (feed.width() / total) as usize } else { 1 };
            let mut x = feed.left() - off;
            for _ in 0..reps {
                for (c, err, gt, ge, gd, w) in &lay {
                    if x > feed.right() {
                        break;
                    }
                    if x + w > feed.left() {
                        let y = feed.center().y;
                        let mut cx = x + 22.0;
                        fp.galley(pos2(cx, y - gt.size().y / 2.0), gt.clone(), MUTED);
                        cx += gt.size().x + 10.0;
                        chip(&fp, pos2(cx + 4.0, y), *c);
                        cx += 18.0;
                        fp.galley(pos2(cx, y - ge.size().y / 2.0), ge.clone(), if *err { BAD } else { TEXT });
                        cx += ge.size().x + 10.0;
                        fp.galley(pos2(cx, y - gd.size().y / 2.0), gd.clone(), SOFT);
                        fp.line_segment([pos2(x + w, feed.top()), pos2(x + w, feed.bottom())], Stroke::new(1.0_f32, RULE));
                    }
                    x += w;
                }
            }
            fade_h(&p, Rect::from_min_size(feed.min, vec2(48.0, feed.height())), BG, Color32::TRANSPARENT);
            fade_h(&p, Rect::from_min_max(pos2(feed.right() - 48.0, feed.top()), feed.max), Color32::TRANSPARENT, BG);
        });
    }

    fn toasts(&mut self, ctx: &egui::Context) {
        let recent: Vec<(String, bool, f32)> = {
            let s = self.bg.lock().unwrap();
            s.log
                .iter()
                .rev()
                .take(3)
                .filter(|(t, _, err)| t.elapsed() < Duration::from_secs(if *err { 10 } else { 4 }))
                .map(|(t, m, e)| (m.clone(), *e, t.elapsed().as_secs_f32()))
                .collect()
        };
        if recent.is_empty() {
            return;
        }
        // Top right and click-through, so a toast never covers or swallows a click on the
        // bottom-bar actions.
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(Align2::RIGHT_TOP, vec2(-PAD_X, 196.0))
            .interactable(false)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
            ui.set_max_width(440.0);
            for (m, err, age) in recent {
                let col = if err { BAD } else { ACCENT };
                let k = if motion() { ease((age / 0.22).min(1.0)) } else { 1.0 };
                ui.set_opacity(k);
                egui::Frame::none().fill(SURFACE2).inner_margin(Margin { left: 18.0, right: 16.0, top: 12.0, bottom: 12.0 }).show(ui, |ui| {
                    let r = ui.max_rect();
                    ui.painter().rect_filled(Rect::from_min_size(r.min - vec2(18.0, 12.0), vec2(4.0, r.height() + 24.0)), 0.0, col);
                    ui.label(RichText::new(m).font(medium(14.0)).color(if err { BAD } else { TEXT }));
                });
                ui.add_space(6.0);
            }
        });
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|m| m.focused().is_some());
        if typing || self.setup_due() {
            return;
        }
        let keys = [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5];
        ctx.input(|i| {
            for (k, p) in keys.iter().zip(Page::TABS) {
                if i.key_pressed(*k) && !i.modifiers.any() {
                    self.page = p;
                }
            }
            let idx = self.page.index();
            let n = Page::TABS.len();
            if i.key_pressed(egui::Key::Q) && !i.modifiers.any() {
                self.page = Page::TABS[(idx + n - 1) % n];
            }
            if i.key_pressed(egui::Key::F) && !i.modifiers.any() {
                self.want_live_toggle = true;
            }
            if i.key_pressed(egui::Key::E) && !i.modifiers.any() {
                self.page = Page::TABS[(idx + 1) % n];
            }
        });
    }

    fn pairing_view(&mut self, ui: &mut egui::Ui) {
        let (found, discovering, pairing, status) = {
            let s = self.bg.lock().unwrap();
            (s.found.clone(), s.discovering, s.pairing, s.pair_status.clone())
        };
        ui.add_space(24.0);
        ui.vertical(|ui| {
            ui.set_max_width(640.0);
            kicker(ui, "First run");
            ui.label(headline("Link your bridge", 76.0));
            ui.label(RichText::new("Pick yours below, then press the round button on top of the bridge.").font(body(15.0)).color(SOFT));
            ui.add_space(22.0);
            ui.horizontal(|ui| {
                section(ui, "Bridges on your network");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if discovering {
                        ui.label(label_caps("Scanning…", 15.0, ACCENT));
                        ui.ctx().request_repaint_after(Duration::from_millis(200));
                    } else if ui.add(ghost("Scan again").icon(Icon::Refresh)).clicked() {
                        self.discover();
                    }
                });
            });
            ui.add_space(4.0);
            if found.is_empty() && !discovering {
                ui.label(RichText::new("Nothing found. Check the bridge is powered and on the same network, or type its IP below.").color(MUTED));
            }
            for f in &found {
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
                row_rule(ui.painter(), row.top(), row.left(), row.right());
                text_at(ui.painter(), pos2(row.left(), row.center().y), Align2::LEFT_CENTER, &f.ip, display(22.0), TEXT, 0.5);
                text_at(ui.painter(), pos2(row.left() + 170.0, row.center().y), Align2::LEFT_CENTER, if f.source == "mDNS" { "" } else { "VIA HUE SYNC" }, display(14.0), MUTED, 1.0);
                let br = Rect::from_min_max(pos2(row.right() - 120.0, row.top() + 8.0), pos2(row.right(), row.bottom() - 8.0));
                if ui.put(br, primary("Link").height(40.0)).clicked() && !pairing {
                    self.start_pair(f.ip.clone());
                }
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add(text_input(&mut self.manual_ip, "Bridge IP, e.g. 192.168.1.2").desired_width(260.0));
                if ui.add_enabled(!pairing && !self.manual_ip.trim().is_empty(), primary("Link").height(44.0)).clicked() {
                    self.start_pair(self.manual_ip.trim().to_string());
                }
            });
            if !status.is_empty() {
                ui.add_space(18.0);
                ui.label(RichText::new(status.to_uppercase()).font(display(20.0)).color(if pairing { ACCENT } else { TEXT }).extra_letter_spacing(0.5));
            }
        });
    }

    /// Demo + LUMEN_SHOTS=<dir>: visit every page, save a PNG of each, then quit.
    fn self_shots(&mut self, ctx: &egui::Context) {
        let Ok(dir) = std::env::var("LUMEN_SHOTS") else { return };
        // (page, file name, prep): prep 10 + n scrolls Settings to section n; 20/21 and 30/31
        // set up the Lights and Moods views before the shot
        const ALL: &[(Page, &str, u8)] = &[
            (Page::Home, "home", 0),
            (Page::Lights, "lights", 0),
            (Page::Lights, "lights-gradient", 20),
            (Page::Lights, "lights-white-map", 21),
            (Page::Palettes, "moods", 0),
            (Page::Palettes, "moods-list", 30),
            (Page::Palettes, "moods-map", 31),
            (Page::Games, "games", 0),
            (Page::Game, "game", 0),
            (Page::Dota, "dota", 0),
            (Page::Cod, "cod", 0),
            (Page::Settings, "settings", 10),
            (Page::Settings, "settings-setup", 11),
            (Page::Settings, "settings-games", 14),
            (Page::Settings, "settings-keys", 16),
            (Page::Settings, "settings-privacy", 17),
            (Page::Sync, "live", 0),
            (Page::Home, "setup-screen", 40),
            (Page::Home, "setup-room", 41),
            (Page::Home, "setup-area", 42),
            (Page::Home, "setup-sound", 43),
            (Page::Home, "setup-done", 44),
        ];
        // LUMEN_SHOTS_ONLY=home,settings-keys limits the run to those shots
        let only = std::env::var("LUMEN_SHOTS_ONLY").unwrap_or_default();
        let plan: Vec<(Page, &str, u8)> =
            ALL.iter().copied().filter(|(_, n, _)| only.is_empty() || only.split(',').any(|o| o.trim() == *n)).collect();
        let id = egui::Id::new("self-shots");
        let (mut step, mut since): (usize, f64) = ctx.data(|d| d.get_temp(id)).unwrap_or((0, 0.0));
        let now = ctx.input(|i| i.time);
        if since == 0.0 {
            since = now;
        }
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = &shot {
            if let Some((_, name, _)) = plan.get(step) {
                let [w, h] = img.size;
                let px: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, px) {
                    let _ = buf.save(std::path::Path::new(&dir).join(format!("{name}.png")));
                }
            }
            step += 1;
            since = now;
        }
        match plan.get(step) {
            Some((page, _, prep)) => {
                self.setup.open = *prep >= 40;
                if *prep >= 40 {
                    self.setup.step = (*prep - 40) as usize;
                }
                if (30..40).contains(prep) && shot.is_none() && now - since < 0.2 {
                    self.moods.view = if *prep == 31 { crate::gui::moods::LightView::Map } else { crate::gui::moods::LightView::List };
                    self.moods.reveal = true;
                } else                 if *prep >= 20 && shot.is_none() && now - since < 0.2 {
                    let id = if *prep == 20 { "l3" } else { "l9" };
                    self.lights.sel = crate::gui::lights::Sel::Light(id.into());
                    self.lights.show_map = *prep == 21;
                } else if *prep >= 10 && *prep < 20 {
                    self.settings.jump = Some((*prep - 10) as usize);
                }
                self.page = *page;
                let settle = if step == 0 { 4.5 } else { 1.6 };
                if now - since > settle && shot.is_none() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                    since = now + 10.0; // wait for the Screenshot event
                }
            }
            None => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
        ctx.data_mut(|d| d.insert_temp(id, (step, since)));
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn loading_view(&self, ui: &mut egui::Ui) {
        ui.add_space(120.0);
        ui.vertical(|ui| {
            kicker(ui, "Bridge");
            ui.label(headline("Reading your lights", 64.0));
            let (r, _) = ui.allocate_exact_size(vec2(320.0, 6.0), Sense::hover());
            let t = ui.input(|i| i.time) as f32;
            ui.painter().rect_filled(r, 0.0, SURFACE3);
            let x = r.left() + (t * 0.9).fract() * (r.width() - 60.0);
            ui.painter().rect_filled(Rect::from_min_size(pos2(x, r.top()), vec2(60.0, 6.0)), 0.0, ACCENT);
            ui.add_space(8.0);
            ui.label(RichText::new("Connecting to your bridge").color(MUTED));
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        });
    }
}

pub struct Tick {
    key: String,
    value: String,
    unit: String,
    chip: Option<Color32>,
    block: Option<Color32>,
}

impl Tick {
    fn plain(k: &str, v: &str) -> Self {
        Self { key: k.into(), value: v.into(), unit: String::new(), chip: None, block: None }
    }
    fn chip(k: &str, v: &str, c: Color32) -> Self {
        Self { key: k.into(), value: v.into(), unit: String::new(), chip: Some(c), block: None }
    }
    fn unit(k: &str, v: &str, u: &str, c: Color32) -> Self {
        Self { key: k.into(), value: v.into(), unit: u.into(), chip: Some(c), block: None }
    }
    fn block(v: &str, fill: Color32) -> Self {
        Self { key: String::new(), value: v.into(), unit: String::new(), chip: None, block: Some(fill) }
    }
}

fn ticker_item(p: &egui::Painter, x: f32, bar: Rect, t: &Tick) -> f32 {
    let font = display(15.0);
    let y = bar.center().y;
    if let Some(fill) = t.block {
        let g = p.layout_job(spaced(&t.value, font, Color32::PLACEHOLDER, 0.6));
        let r = Rect::from_min_max(pos2(x, bar.top()), pos2(x + g.size().x + 54.0, bar.bottom() - 1.0));
        p.rect_filled(r, 0.0, fill);
        let ink = if fill == SURFACE3 { SOFT } else { INK };
        chip(p, pos2(r.left() + 22.0, y), ink);
        p.galley(pos2(r.left() + 34.0, y - g.size().y / 2.0), g, ink);
        return r.right();
    }
    let gk = p.layout_job(spaced(&t.key, font.clone(), Color32::PLACEHOLDER, 0.6));
    let gv = p.layout_job(spaced(&t.value, font.clone(), Color32::PLACEHOLDER, 0.6));
    let mut cx = x + 18.0;
    p.galley(pos2(cx, y - gk.size().y / 2.0), gk.clone(), MUTED);
    cx += gk.size().x + 8.0;
    if let Some(c) = t.chip {
        chip(p, pos2(cx + 4.0, y), c);
        cx += 14.0;
    }
    p.galley(pos2(cx, y - gv.size().y / 2.0), gv.clone(), TEXT);
    cx += gv.size().x;
    if !t.unit.is_empty() {
        let gu = p.layout_job(spaced(&t.unit, display(11.0), Color32::PLACEHOLDER, 0.5));
        p.galley(pos2(cx + 1.0, y + gv.size().y / 2.0 - gu.size().y - 1.0), gu.clone(), MUTED);
        cx += gu.size().x + 1.0;
    }
    let end = cx + 18.0;
    p.line_segment([pos2(end, bar.top()), pos2(end, bar.bottom() - 1.0)], Stroke::new(1.0_f32, RULE));
    end
}

fn load_logo(ctx: &egui::Context) -> egui::TextureHandle {
    let img = image::load_from_memory_with_format(include_bytes!("../../assets/brand/roksync-wordmark.png"), image::ImageFormat::Png)
        .expect("bundled logo")
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    ctx.load_texture("roksync-logo", egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), egui::TextureOptions::LINEAR)
}

fn fade_h(p: &egui::Painter, rect: Rect, left: Color32, right: Color32) {
    let mut m = egui::Mesh::default();
    m.colored_vertex(rect.left_top(), left);
    m.colored_vertex(rect.right_top(), right);
    m.colored_vertex(rect.left_bottom(), left);
    m.colored_vertex(rect.right_bottom(), right);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 3, 2);
    p.add(egui::Shape::mesh(m));
}

impl LumenApp {
    pub fn ensure_panic_key(&mut self) {
        if self.panic_key_on || self.demo || !self.cfg.panic_key {
            return;
        }
        match crate::system::spawn_panic_key(self.ctx.clone()) {
            Ok(()) => self.panic_key_on = true,
            Err(e) => self.toast(format!("Stop-all key: {e:#}"), true),
        }
    }

    fn panic_tick(&mut self) {
        if !crate::system::take_panic() || !self.cfg.panic_key {
            return;
        }
        self.stop_live();
        self.cfg.fx_paused = true;
        self.save_cfg();
        self.bg.lock().unwrap().note("Stop all", "Screen sync stopped, game effects paused (Ctrl+Alt+L)", BAD);
        self.toast("Stop all: turn game events back on from Live", false);
    }

    fn first_frame(&mut self, ctx: &egui::Context) {
        if self.first_frame_done {
            return;
        }
        self.first_frame_done = true;
        if START_MINIMIZED.load(Ordering::Relaxed) {
            match &self.tray {
                Some(t) if self.cfg.to_tray => t.hide(),
                _ => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            }
        }
        self.ensure_panic_key();
    }

    fn live_toggle(&mut self, inv: &Inventory) {
        if !std::mem::take(&mut self.want_live_toggle) {
            return;
        }
        if self.is_streaming() {
            self.stop_live();
        } else {
            self.go_live(inv);
        }
    }
}

impl eframe::App for LumenApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if !self.demo {
            self.shutdown();
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [BG.r() as f32 / 255.0, BG.g() as f32 / 255.0, BG.b() as f32 / 255.0, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.demo {
            self.self_shots(ctx);
        }
        if tray::QUIT.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // with to_tray, minimise and close hide to the tray and everything keeps running; Quit is in the tray menu
        if let Some(t) = &self.tray {
            if self.cfg.to_tray && ctx.input(|i| i.viewport().close_requested()) && !tray::QUIT.load(Ordering::Relaxed) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                t.hide();
            }
            if self.cfg.to_tray && !t.is_hidden() && ctx.input(|i| i.viewport().minimized == Some(true)) {
                t.hide();
            }
            t.set_live(self.is_streaming());
        }
        let paired = self.bg.lock().unwrap().paired.take();
        if let Some(b) = paired {
            self.bridge = Bridge::new(&b.ip, &b.app_key).ok();
            if !self.demo {
                if let Err(e) = b.save() {
                    self.toast(format!("Bridge: couldn't save the pairing ({e:#})"), true);
                }
            }
            self.bg.lock().unwrap().note("Bridge", &format!("Paired with \"{}\"", b.name), GOOD);
            self.cfg.bridge = Some(b);
            self.save_cfg();
            self.refresh();
        }
        self.bridge_housekeeping();
        self.pump_pending();
        // minimised or in the tray: unload the mood model to free RAM/VRAM
        let away = self.is_away();
        if self.mood_housekeeping(away) {
            ctx.request_repaint_after(std::time::Duration::from_secs(1)); // mid-answer: try again shortly
        }
        if crate::perf::profile().model_preload && !away {
            self.keep_warm();
        }
        self.keyboard(ctx);
        self.track_fx();
        self.games_fx_tick();
        self.apply_perf();
        self.panic_tick();
        self.first_frame(ctx);
        self.games.detect(&self.autolive);
        if self.page != self.last_page {
            self.last_page = self.page;
            self.page_since = Instant::now();
            self.settings_recheck(); // a page that shows probed state reads it fresh on entry
        }
        if !matches!(self.page, Page::Games | Page::Game | Page::Dota | Page::Cod) {
            self.games.release();
        }

        {
            let want = (self.page == Page::Sync && self.sy.source == crate::sync::Source::Screen)
                || (self.is_streaming() && self.sy.source == crate::sync::Source::Screen);
            self.screen.lock().unwrap().wanted = want;
        }
        if self.bg.lock().unwrap().inv.is_some() {
            self.game_rules_tick();
        }
        {
            let mut sh = self.sync_shared.lock().unwrap();
            sh.sound = self.cfg.sound.on.then(|| self.cfg.sound.layer(self.sy.source));
            sh.source = self.sy.source;
            sh.fx_gain = self.cfg.fx_gain;
            sh.cs2_enabled = self.cfg.cs2.enabled && !self.cfg.fx_paused;
            sh.dota_enabled = self.cfg.dota.enabled && !self.cfg.fx_paused;
            sh.cod_enabled = self.cfg.cod.enabled && !self.cfg.fx_paused;
        }

        self.setup_frame_end();
        let inv = self.bg.lock().unwrap().inv.clone();
        self.status_bar(ctx, inv.as_deref());
        if let (Some(i), true) = (inv.as_ref(), self.setup_due() && (self.bridge.is_some() || self.demo)) {
            let i = i.clone();
            self.toasts(ctx);
            self.setup_view(ctx, &i);
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        if let (Some(i), true) = (inv.as_deref(), self.bridge.is_some() || self.demo) {
            self.target_bar(ctx, i);
        }
        if self.bridge.is_some() || self.demo {
            self.nav(ctx);
        }
        self.crawl(ctx);
        self.toasts(ctx);

        let content_frame = egui::Frame::none().fill(BG).inner_margin(Margin { left: PAD_X, right: PAD_X, top: 22.0, bottom: 20.0 });
        if self.bridge.is_none() && !self.demo {
            egui::CentralPanel::default().frame(content_frame).show(ctx, |ui| self.pairing_view(ui));
            ctx.request_repaint_after(Duration::from_millis(250));
            return;
        }
        let Some(inv) = inv else {
            let link = self.bg.lock().unwrap().link;
            if matches!(link, Link::Unreachable | Link::Searching) {
                egui::CentralPanel::default().frame(content_frame).show(ctx, |ui| self.offline_view(ui));
                ctx.request_repaint_after(Duration::from_millis(500));
            } else {
                egui::CentralPanel::default().frame(content_frame).show(ctx, |ui| self.loading_view(ui));
            }
            return;
        };
        self.refresh_autolive();
        self.live_toggle(&inv);

        let k = if motion() { (self.page_since.elapsed().as_secs_f32() / 0.5).min(1.0) } else { 1.0 };
        let e = ease(k);
        if k < 1.0 {
            ctx.request_repaint();
        }
        let content_frame = content_frame.inner_margin(Margin { left: PAD_X, right: PAD_X, top: 22.0 + (1.0 - e) * 6.0, bottom: 20.0 });
        let fade = move |ui: &mut egui::Ui| ui.set_opacity(e);
        match self.page {
            Page::Home => {
                egui::CentralPanel::default().frame(content_frame).show(ctx, |ui| {
                    fade(ui);
                    self.home_page(ui, &inv)
                });
            }
            Page::Lights => self.lights_page(ctx, &inv, content_frame, e),
            Page::Palettes => {
                egui::CentralPanel::default().frame(content_frame).show(ctx, |ui| {
                    fade(ui);
                    self.moods_page(ui, &inv)
                });
            }
            Page::Sync => self.sync_page(ctx, &inv, content_frame, e),
            Page::Games => {
                egui::CentralPanel::default().frame(content_frame).show(ctx, |ui| {
                    fade(ui);
                    self.games_page(ui, &inv)
                });
            }
            Page::Game => {
                egui::CentralPanel::default().frame(content_frame).show(ctx, |ui| {
                    fade(ui);
                    self.game_page(ui, &inv)
                });
            }
            Page::Dota => {
                egui::CentralPanel::default().frame(content_frame).show(ctx, |ui| {
                    fade(ui);
                    self.dota_page(ui, &inv)
                });
            }
            Page::Cod => {
                egui::CentralPanel::default().frame(content_frame).show(ctx, |ui| {
                    fade(ui);
                    self.cod_page(ui, &inv)
                });
            }
            Page::Mood => {
                egui::CentralPanel::default().frame(content_frame).show(ctx, |ui| {
                    fade(ui);
                    self.mood_page(ui, &inv)
                });
            }
            Page::Settings => {
                egui::CentralPanel::default().frame(content_frame).show(ctx, |ui| {
                    fade(ui);
                    self.settings_page(ui)
                });
            }
        }
        // slow repaint, slower still while another window (a game) has focus; sync has its own thread
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        let prof = crate::perf::profile();
        let beat = if focused { prof.ui_focused.min(Duration::from_millis(1000)) } else { prof.ui_background };
        ctx.request_repaint_after(beat);
    }
}

impl Drop for LumenApp {
    fn drop(&mut self) {
        let handle = self.sync.lock().unwrap().take();
        if let Some(mut s) = handle {
            s.stop();
        }
    }
}

impl LumenApp {
    /// Stops the stream (releasing the entertainment area) and the model server, then exits the
    /// process outright so no listener, download or answer in flight keeps a hidden instance alive.
    fn shutdown(&mut self) {
        let handle = self.sync.lock().unwrap().take();
        if let Some(mut s) = handle {
            s.stop();
        }
        if let Ok(mut srv) = self.mood.server.try_lock() {
            *srv = None;
        } // mid-answer: the server is in a job object and dies with this process anyway
        std::process::exit(0);
    }
}
