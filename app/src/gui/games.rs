//! Games library (Steam games with cover art, per-game mode and launch rules) and the CS2
//! effects page.

use super::icons::Icon;
use super::theme::*;
use super::{LumenApp, Page};
use crate::autolive::{self, Event, SharedAutoLive};
use crate::config::GameRules;
use crate::games_fx::{native_for, Native};
use crate::modes::{self, MODES};
use crate::steam::{self, Art, Game};
use eframe::egui::{self, pos2, vec2, Align2, Color32, ColorImage, Rect, RichText, Sense, Stroke, TextureHandle, TextureOptions, UiBuilder, Vec2};
use lumen_cs2::Fx;
use lumen_engine::ChannelPos;
use lumen_hue::{Inventory, Rgb};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

enum Loaded {
    Cover(u32, Option<ColorImage>),
    Hero(u32, Option<ColorImage>),
}

pub use crate::autolive::Detected;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum Filter {
    #[default]
    All,
    Native,
    Yours,
}

#[derive(Default)]
pub struct GamesUi {
    lib: Option<Vec<Game>>,
    covers: HashMap<u32, TextureHandle>,
    no_art: HashSet<u32>,
    heroes: HashMap<u32, Option<TextureHandle>>,
    hero_loading: HashSet<u32>,
    inbox: Arc<Mutex<Vec<Loaded>>>,
    cancel: Arc<AtomicBool>,
    loading: bool,
    filter: Filter,
    last_scan: Option<Instant>,
    pub sel: Option<u32>,
    pub detected: Option<Detected>,
    /// Demo mode: simulate CS2 running.
    pub demo_running: bool,
    last_detected: Option<u32>,
}

const CS2: u32 = 730;

impl GamesUi {
    /// Frees the cover textures (≈20 MB); they reload on demand.
    pub fn release(&mut self) {
        if !self.loading && self.covers.is_empty() && self.heroes.is_empty() {
            return;
        }
        self.cancel.store(true, Ordering::Relaxed);
        self.covers.clear();
        self.no_art.clear();
        self.heroes.clear();
        self.hero_loading.clear();
        self.loading = false;
        self.inbox.lock().unwrap().clear();
    }

    fn library(&mut self) -> &[Game] {
        if self.lib.is_none() {
            let mut lib = steam::installed();
            if self.demo_running && !lib.iter().any(|g| g.appid == CS2) {
                lib.insert(0, Game { appid: CS2, name: "Counter-Strike 2".into() });
            }
            self.lib = Some(lib);
        }
        self.lib.as_deref().unwrap_or(&[])
    }

    pub fn library_len(&self) -> usize {
        self.lib.as_ref().map(|l| l.len()).unwrap_or(0)
    }

    pub fn detected_name(&self) -> Option<String> {
        self.detected.as_ref().map(|d| d.name.clone())
    }

    /// Outside demo mode the auto-live thread does the looking; this reads what it found.
    pub fn detect(&mut self, al: &SharedAutoLive) {
        if self.last_scan.map_or(false, |t| t.elapsed() < Duration::from_secs(1)) {
            return;
        }
        self.last_scan = Some(Instant::now());
        self.library();
        if self.demo_running {
            self.detected = Some(Detected { appid: CS2, name: "Counter-Strike 2".into(), window: None });
            return;
        }
        self.detected = al.lock().unwrap().detected.clone();
    }

    fn ensure_art(&mut self, ctx: &egui::Context) {
        if !self.loading {
            self.loading = true;
            self.cancel = Arc::new(AtomicBool::new(false));
            let (games, inbox, cancel, ctx) = (self.lib.clone().unwrap_or_default(), self.inbox.clone(), self.cancel.clone(), ctx.clone());
            std::thread::Builder::new()
                .name("cover-art".into())
                .spawn(move || {
                    for g in games {
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        let im = steam::art(g.appid, Art::Cover).and_then(|p| steam::load_rgba(&p, (300, 450))).map(|(w, h, px)| ColorImage::from_rgba_unmultiplied([w, h], &px));
                        inbox.lock().unwrap().push(Loaded::Cover(g.appid, im));
                        ctx.request_repaint();
                    }
                })
                .ok();
        }
        let items: Vec<Loaded> = std::mem::take(&mut *self.inbox.lock().unwrap());
        for l in items {
            match l {
                Loaded::Cover(id, Some(im)) => {
                    self.covers.insert(id, ctx.load_texture(format!("cover-{id}"), im, TextureOptions::LINEAR));
                }
                Loaded::Cover(id, None) => {
                    self.no_art.insert(id);
                }
                Loaded::Hero(id, im) => {
                    self.hero_loading.remove(&id);
                    self.heroes.insert(id, im.map(|im| ctx.load_texture(format!("hero-{id}"), im, TextureOptions::LINEAR)));
                }
            }
        }
    }

    fn hero(&mut self, ctx: &egui::Context, appid: u32) -> Option<TextureHandle> {
        if let Some(h) = self.heroes.get(&appid) {
            return h.clone();
        }
        if self.hero_loading.insert(appid) {
            let (inbox, ctx) = (self.inbox.clone(), ctx.clone());
            std::thread::spawn(move || {
                let im = steam::art(appid, Art::Hero).and_then(|p| steam::load_rgba(&p, (1280, 420))).map(|(w, h, px)| ColorImage::from_rgba_unmultiplied([w, h], &px));
                inbox.lock().unwrap().push(Loaded::Hero(appid, im));
                ctx.request_repaint();
            });
        }
        None
    }

    pub fn game_name(&self, appid: u32) -> Option<&str> {
        self.lib.as_ref()?.iter().find(|g| g.appid == appid).map(|g| g.name.as_str())
    }

    fn is_running(&self, appid: u32) -> bool {
        self.detected.as_ref().map(|d| d.appid == appid).unwrap_or(false)
    }
}

/// UV rect that crops a texture to fill `rect` (like CSS object-fit: cover), zoomed by `zoom`.
fn cover_uv(size: [usize; 2], rect: Rect, zoom: f32) -> Rect {
    let (tw, th) = (size[0] as f32, size[1] as f32);
    let (ta, ra) = (tw / th, rect.width() / rect.height());
    let (mut w, mut h) = if ta > ra { (ra / ta, 1.0) } else { (1.0, ta / ra) };
    w /= zoom;
    h /= zoom;
    Rect::from_center_size(pos2(0.5, 0.5), vec2(w, h))
}

fn vfade(p: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut m = egui::Mesh::default();
    m.colored_vertex(rect.left_top(), top);
    m.colored_vertex(rect.right_top(), top);
    m.colored_vertex(rect.left_bottom(), bottom);
    m.colored_vertex(rect.right_bottom(), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 3, 2);
    p.add(egui::Shape::mesh(m));
}

fn mode_color(m: &modes::Mode) -> Color32 {
    match m.id {
        "competitive" => hex("#4FD1FF"),
        "shooter" => hex("#FF5A1F"),
        "cinematic" => hex("#7C6CFF"),
        "rpg" => hex("#E0A43A"),
        "horror" => hex("#D7263D"),
        "racing" => hex("#22D3A6"),
        "cozy" => hex("#FF8FB1"),
        _ => c32(Rgb::from_u8(m.art[0])),
    }
}

/// Cover for games without art: a diagonal blend seeded by the appid.
fn fallback_art(p: &egui::Painter, r: Rect, appid: u32, key: Color32) {
    let h = (appid.wrapping_mul(2654435761) % 1000) as f32 / 1000.0;
    let a = Rgb::from_hsv(h, 0.65, 0.55);
    let b = Rgb::from_hsv((h + 0.08) % 1.0, 0.7, 0.22);
    let mut m = egui::Mesh::default();
    let (ca, cb, cc) = (mix(c32(a), key, 0.25), c32(b), hex("#0D0D0F"));
    m.colored_vertex(r.left_top(), ca);
    m.colored_vertex(r.right_top(), cb);
    m.colored_vertex(r.left_bottom(), cb);
    m.colored_vertex(r.right_bottom(), cc);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 3, 2);
    p.add(egui::Shape::mesh(m));
}

const GROUPS: &[(&str, &str, &[Fx])] = &[
    ("Round", "who takes the round", &[Fx::RoundWinCt, Fx::RoundWinT]),
    ("Bomb", "the beat speeds up as the fuse runs", &[Fx::BombPlanted, Fx::BombDefused, Fx::BombExploded]),
    ("You", "what happens to you", &[Fx::Flash, Fx::LowHp, Fx::Burning, Fx::Death]),
];

fn fx_name(fx: Fx) -> &'static str {
    match fx {
        Fx::Flash => "Flashed",
        Fx::LowHp => "Low health",
        Fx::Burning => "Burning",
        Fx::BombPlanted => "Bomb planted",
        Fx::BombExploded => "Exploded",
        Fx::BombDefused => "Defused",
        Fx::RoundWinCt => "Round won · CT",
        Fx::RoundWinT => "Round won · T",
        Fx::Death => "Death",
    }
}

fn fx_desc(fx: Fx) -> &'static str {
    match fx {
        Fx::Flash => "Room goes white while you're blind",
        Fx::LowHp => "Red heartbeat under 40 HP, faster near zero",
        Fx::Burning => "Orange flicker while you stand in fire",
        Fx::BombPlanted => "Red beat that follows the 40 s fuse",
        Fx::BombExploded => "White burst, then an orange fall-off",
        Fx::BombDefused => "Cool blue sweep",
        Fx::RoundWinCt => "Three soft pulses in CT blue",
        Fx::RoundWinT => "Three soft pulses in T gold",
        Fx::Death => "Colour drains while you spectate",
    }
}

impl LumenApp {
    /// (effects on, total); none count as on while CS2 effects are disabled.
    pub fn fx_counts(&self) -> (usize, usize) {
        let on = if self.cfg.cs2.enabled { Fx::ALL.iter().filter(|f| self.cfg.cs2.is_on(**f)).count() } else { 0 };
        (on, Fx::ALL.len())
    }

    fn fx_changed(&mut self) {
        self.cfg.cs2.push_to(&mut self.cs2.lock().unwrap());
        self.save_cfg();
    }

    pub fn rules_for(&self, appid: u32) -> GameRules {
        self.cfg.game_rules.get(&appid).cloned().unwrap_or_default()
    }

    /// Notes newly detected games and brings the UI in line with what launch rules did on the
    /// auto-live thread (the Hue bridge restores the lights when a stream ends).
    pub fn game_rules_tick(&mut self) {
        let now = self.games.detected.as_ref().map(|d| d.appid);
        if now != self.games.last_detected {
            self.games.last_detected = now;
            if let Some(d) = &self.games.detected {
                self.bg.lock().unwrap().note(&d.name, "Detected", INFO);
            }
        }
        let events = std::mem::take(&mut self.autolive.lock().unwrap().events);
        for e in events {
            match e {
                Event::Started { appid, mode, area, mix } => {
                    self.apply_mode(&mode);
                    self.set_mix(mix);
                    if let Some((hwnd, title)) = self.games.detected.as_ref().and_then(|d| d.window.clone()) {
                        self.sy.follow_window(hwnd, title);
                    }
                    self.sy.area = Some(area.clone());
                    self.cfg.last_area = Some(area);
                    self.save_cfg();
                    let game = self.games.game_name(appid).unwrap_or("game").to_string();
                    self.bg.lock().unwrap().note("Live", &format!("{game}: {}", crate::mix::describe(mix)), ACCENT);
                }
                Event::Stopped => self.bg.lock().unwrap().note("Screen sync", "Stopped", ACCENT),
            }
        }
    }

    /// Keeps the auto-live thread's launch plan current, at most once a second.
    pub fn refresh_autolive(&mut self) {
        if self.demo || self.autolive_at.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
            return;
        }
        self.autolive_at = Some(Instant::now());
        let plan = autolive::plan(&self.cfg, self.bridge.clone(), self.sy.area.clone(), self.sy.program(), self.fx_sources());
        self.autolive.lock().unwrap().plan = plan;
    }

    pub fn games_page(&mut self, ui: &mut egui::Ui, _inv: &Inventory) {
        let ctx = ui.ctx().clone();
        self.games.library();
        self.games.ensure_art(&ctx);
        let lib = self.games.lib.clone().unwrap_or_default();
        if self.games.sel.is_none() {
            self.games.sel = self.games.detected.as_ref().map(|d| d.appid).or_else(|| lib.iter().find(|g| g.appid == CS2).map(|g| g.appid)).or_else(|| lib.first().map(|g| g.appid));
        }
        let full = ui.available_rect_before_wrap();
        let aside_w = 400.0;
        let left = Rect::from_min_max(full.min, pos2(full.right() - aside_w - 36.0, full.bottom()));
        let right = Rect::from_min_max(pos2(left.right() + 36.0, full.top()), full.max);
        ui.allocate_new_ui(UiBuilder::new().max_rect(left), |ui| self.library_grid(ui, &lib));
        ui.allocate_new_ui(UiBuilder::new().max_rect(right), |ui| {
            egui::ScrollArea::vertical().id_salt("game-aside").auto_shrink([false, false]).show(ui, |ui| self.game_aside(ui, &lib));
        });
        ui.allocate_rect(full, Sense::hover());
    }

    fn library_grid(&mut self, ui: &mut egui::Ui, lib: &[Game]) {
        let head_w = (ui.available_width() - 380.0).max(240.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width(head_w);
                ui.label(headline("Games", 44.0));
                ui.label(RichText::new("Your Steam library. Each game gets a mode, and RokSync switches to it when you launch.").font(body(14.0)).color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                let mut f = self.games.filter;
                ui.allocate_ui(vec2(360.0, 40.0), |ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        segmented(ui, &mut f, &[(Filter::All, "All"), (Filter::Native, "Native FX"), (Filter::Yours, "Set by you")]);
                    });
                });
                self.games.filter = f;
            });
        });
        ui.add_space(16.0);
        let shown: Vec<&Game> = lib
            .iter()
            .filter(|g| match self.games.filter {
                Filter::All => true,
                Filter::Native => native_for(g.appid, &g.name).is_some(),
                Filter::Yours => self.cfg.game_modes.contains_key(&g.appid),
            })
            .collect();
        if lib.is_empty() {
            ui.label(RichText::new("No Steam games found. Screen sync on Home works with anything on screen.").color(MUTED));
            return;
        }
        let mut shown = shown;
        shown.sort_by_key(|g| (!self.games.is_running(g.appid), native_for(g.appid, &g.name).is_none(), g.name.to_lowercase()));
        egui::ScrollArea::vertical().id_salt("library").auto_shrink([false, false]).show(ui, |ui| {
            let gap = 14.0;
            let w = ui.available_width() - 4.0;
            let per = (((w + gap) / (150.0 + gap)).floor() as usize).max(1);
            let tw = (w - gap * (per as f32 - 1.0)) / per as f32;
            let art_h = tw * 1.5;
            let th = art_h + 34.0;
            let mut pick = None;
            for (ri, row) in shown.chunks(per).enumerate() {
                let (rr, _) = ui.allocate_exact_size(vec2(w, th + 4.0), Sense::hover());
                if !ui.is_rect_visible(rr) {
                    ui.add_space(gap - ui.spacing().item_spacing.y);
                    continue;
                }
                for (ci, g) in row.iter().enumerate() {
                    let r = Rect::from_min_size(pos2(rr.left() + 2.0 + ci as f32 * (tw + gap), rr.top() + 2.0), vec2(tw, th));
                    let resp = ui.interact(r, ui.id().with(("game", g.appid)), Sense::click());
                    let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.3);
                    let sel = self.games.sel == Some(g.appid);
                    let mode = modes::find(&self.mode_for(g.appid, &g.name)).unwrap_or(&MODES[2]);
                    let key = mode_color(mode);
                    let p = ui.painter();
                    p.rect_filled(r, 0.0, SURFACE);
                    let art = Rect::from_min_size(r.min, vec2(tw, art_h));
                    let ap = p.with_clip_rect(art);
                    let k = if motion() { ease(((ri * per + ci) as f32 * 0.035).min(0.0) + 1.0) } else { 1.0 };
                    let _ = k;
                    if let Some(tex) = self.games.covers.get(&g.appid) {
                        ap.image(tex.id(), art, cover_uv(tex.size(), art, 1.0 + 0.04 * hov), Color32::WHITE);
                    } else {
                        fallback_art(&ap, art, g.appid, key);
                        vfade(&ap, Rect::from_min_max(pos2(art.left(), art.bottom() - 90.0), art.max), Color32::TRANSPARENT, Color32::from_black_alpha(130));
                        let gal = ap.layout(g.name.to_uppercase(), display_bold(21.0), TEXT, art.width() - 20.0);
                        ap.galley(pos2(art.left() + 10.0, art.bottom() - 10.0 - gal.size().y), gal, TEXT);
                    }
                    let mut ty = art.top();
                    for (show, label, fill) in [(self.games.is_running(g.appid), "RUNNING", INFO), (native_for(g.appid, &g.name).is_some(), if native_for(g.appid, &g.name) == Some(Native::Cod) { "ESTIMATED FX" } else { "NATIVE FX" }, ACCENT)] {
                        if show {
                            let gl = ap.layout_job(spaced(label, display_bold(12.0), Color32::PLACEHOLDER, 1.0));
                            let tr = Rect::from_min_size(pos2(art.left(), ty), vec2(gl.size().x + 14.0, 20.0));
                            ap.rect_filled(tr, 0.0, fill);
                            ap.galley(tr.center() - gl.size() / 2.0, gl, INK);
                            ty = tr.bottom();
                        }
                    }
                    let my = art.bottom() + 17.0;
                    chip(p, pos2(r.left() + 14.0, my), key);
                    text_at(p, pos2(r.left() + 25.0, my), Align2::LEFT_CENTER, &mode.name.to_uppercase(), display(14.0), TEXT, 0.6);
                    let own = self.cfg.game_modes.contains_key(&g.appid);
                    text_at(p, pos2(r.right() - 10.0, my), Align2::RIGHT_CENTER, if own { "YOURS" } else { "AUTO" }, display(12.0), MUTED, 0.6);
                    if sel {
                        p.rect_stroke(r.expand(3.0), 0.0, Stroke::new(2.0_f32, ACCENT));
                    }
                    if resp.on_hover_text(&g.name).clicked() {
                        pick = Some(g.appid);
                    }
                }
                ui.add_space(gap - ui.spacing().item_spacing.y);
            }
            if let Some(id) = pick {
                self.games.sel = Some(id);
            }
        });
    }

    fn game_aside(&mut self, ui: &mut egui::Ui, lib: &[Game]) {
        let Some(g) = self.games.sel.and_then(|id| lib.iter().find(|g| g.appid == id)).cloned() else {
            ui.label(RichText::new("Pick a game.").color(MUTED));
            return;
        };
        let ctx = ui.ctx().clone();
        let w = ui.available_width();
        let (hr, _) = ui.allocate_exact_size(vec2(w, 150.0), Sense::hover());
        let mode_id = self.mode_for(g.appid, &g.name);
        let mode = modes::find(&mode_id).unwrap_or(&MODES[2]);
        {
            let p = ui.painter().with_clip_rect(hr);
            match self.games.hero(&ctx, g.appid) {
                Some(tex) => {
                    p.image(tex.id(), hr, cover_uv(tex.size(), hr, 1.0), Color32::WHITE);
                }
                None => fallback_art(&p, hr, g.appid, mode_color(mode)),
            }
            vfade(&p, Rect::from_min_max(pos2(hr.left(), hr.top() + hr.height() * 0.3), hr.max), Color32::TRANSPARENT, Color32::from_rgba_premultiplied(11, 11, 13, 217));
            let gal = p.layout(g.name.to_uppercase(), display_bold(34.0), TEXT, hr.width() - 32.0);
            p.galley(pos2(hr.left() + 16.0, hr.bottom() - 12.0 - gal.size().y), gal, TEXT);
        }
        ui.add_space(18.0);
        let own = self.cfg.game_modes.contains_key(&g.appid);
        section_note(ui, "Mode", if own { "set by you" } else { "picked automatically" });
        ui.add_space(2.0);
        let guess = modes::guess(g.appid, &g.name);
        let mut pick = None;
        ui.spacing_mut().item_spacing.y = 0.0;
        for m in MODES {
            let (r, resp) = ui.allocate_exact_size(vec2(w, 44.0), Sense::click());
            let on = m.id == mode.id;
            let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
            let p = ui.painter();
            p.rect_filled(r, 0.0, if on { SURFACE2 } else { mix(BG, HOVER, hov) });
            row_rule(p, r.top(), r.left(), r.right());
            chip(p, pos2(r.left() + 16.0, r.center().y), mode_color(m));
            text_at(p, pos2(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, &m.name.to_uppercase(), display(17.0), TEXT, 0.3);
            let (tag, col) = if on {
                (if own { "YOURS" } else { "AUTO" }, ACCENT)
            } else if m.id == guess {
                ("suggested", MUTED)
            } else {
                ("", MUTED)
            };
            text_at(p, pos2(r.right() - 12.0, r.center().y), Align2::RIGHT_CENTER, tag, body(12.5), col, 0.0);
            if resp.on_hover_text(m.blurb).clicked() {
                pick = Some(m.id);
            }
        }
        if let Some(id) = pick {
            self.cfg.game_modes.insert(g.appid, id.to_string());
            self.save_cfg();
            if self.games.is_running(g.appid) {
                self.apply_mode(id);
            }
        }
        ui.add_space(18.0);
        section(ui, "When it launches");
        ui.add_space(2.0);
        let mut rules = self.rules_for(g.appid);
        let before = rules.clone();
        let native_kind = native_for(g.appid, &g.name);
        let native = native_kind.is_some();
        let mut rows: Vec<(&str, &str, &mut bool)> = Vec::new();
        let area = "your entertainment area";
        rows.push(("Start screen sync", "Go live on your area as the game opens", &mut rules.sync));
        if native {
            rows.push(("Game effects", "Native effects layered on top", &mut rules.fx));
        }
        let mut mix = crate::mix::for_game(rules.mix);
        rows.push(("React to sound", "Bass and loud hits push the lights brighter", &mut mix.sound));
        rows.push(("Restore my lights after", "When you quit, lights go back to how they were", &mut rules.restore));
        let _ = area;
        for (name, desc, val) in rows {
            let (r, _) = ui.allocate_exact_size(vec2(w, 56.0), Sense::hover());
            let p = ui.painter();
            row_rule(p, r.top(), r.left(), r.right());
            p.text(pos2(r.left(), r.center().y - 9.0), Align2::LEFT_CENTER, name, semibold(14.5), TEXT);
            p.text(pos2(r.left(), r.center().y + 10.0), Align2::LEFT_CENTER, desc, body(13.0), MUTED);
            let sw = Rect::from_min_max(pos2(r.right() - 64.0, r.center().y - 20.0), pos2(r.right(), r.center().y + 20.0));
            if onoff_at(ui, sw, ui.id().with(("rule", g.appid, name)), *val).clicked() {
                *val = !*val;
            }
        }
        if mix != crate::mix::for_game(rules.mix) {
            rules.mix = Some(mix);
        }
        if rules.fx != before.fx && native {
            self.set_game_fx(native_kind, rules.fx);
        }
        if rules.sync != before.sync || rules.fx != before.fx || rules.restore != before.restore || rules.mix != before.mix {
            self.cfg.game_rules.insert(g.appid, rules);
            self.save_cfg();
        }
        let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
        row_rule(ui.painter(), r.top(), r.left(), r.right());
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.add_space(18.0);
        if let Some(k) = native_kind {
            let (on, page) = match k {
                Native::Cs2 => (self.fx_counts().0, Page::Game),
                Native::Dota => (self.dota_counts().0, Page::Dota),
                Native::Cod => (self.cod_counts().0, Page::Cod),
            };
            if ui.add(primary(&format!("Game effects · {on} on  →")).height(48.0).min_size(vec2(w, 48.0))).clicked() {
                self.page = page;
            }
        } else {
            ui.label(RichText::new("No native effects for this game. Screen sync and its mode do the work.").font(body(13.0)).color(MUTED));
        }
    }

    /// Turning effects on also lifts a stop-all pause.
    pub fn set_game_fx(&mut self, kind: Option<Native>, on: bool) {
        let Some(kind) = kind else { return };
        if on {
            self.cfg.fx_paused = false;
        }
        match kind {
            Native::Cs2 => {
                self.cfg.cs2.enabled = on;
                if on {
                    self.enable_cs2();
                } else {
                    self.fx_changed();
                }
            }
            Native::Dota => {
                self.cfg.dota.enabled = on;
                if on {
                    self.start_dota_listener();
                }
                self.push_game_fx();
                self.save_cfg();
            }
            Native::Cod => {
                self.cfg.cod.enabled = on;
                self.push_game_fx();
                self.save_cfg();
            }
        }
    }

    fn enable_cs2(&mut self) {
        if self.cfg.cs2.token.is_empty() {
            self.cfg.cs2.token = lumen_cs2::new_token();
        }
        self.start_cs2_listener();
        self.fx_changed();
    }

    pub fn game_page(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        let now = Instant::now();
        let connected = self.cs2.lock().unwrap().connected(now);
        let enabled = self.cfg.cs2.enabled;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                if ui.add(egui::Label::new(label_caps("← Games", 15.0, MUTED)).sense(Sense::click())).clicked() {
                    self.page = Page::Games;
                }
                ui.label(headline("Counter-Strike 2", 76.0));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if enabled && connected {
                        tag(ui, "Linked", GOOD, INK);
                    } else if enabled {
                        tag(ui, "Waiting", SURFACE3, SOFT);
                    } else {
                        tag(ui, "Off", SURFACE3, SOFT);
                    }
                    tag(ui, &format!("Game state link · port {}", self.cfg.cs2.port), SURFACE2, SOFT);
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if labelled_switch(ui, "Game effects", enabled, 48.0).clicked() {
                    self.cfg.cs2.enabled = !enabled;
                    if !enabled {
                        self.cfg.fx_paused = false;
                        self.enable_cs2();
                    } else {
                        self.fx_changed();
                    }
                }
                let mode = self.cfg.game_modes.get(&CS2).cloned().unwrap_or_else(|| "competitive".into());
                let m = modes::find(&mode).unwrap_or(&MODES[0]);
                let mresp = ui.add(ghost(&format!("Mode: {} ▾", m.name)).height(48.0));
                let pid = ui.make_persistent_id("cs2-mode");
                if mresp.clicked() {
                    ui.memory_mut(|mm| mm.toggle_popup(pid));
                }
                let mut pick = None;
                egui::popup_below_widget(ui, pid, &mresp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                    ui.set_min_width(240.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for mm in MODES {
                        if ui.add(ghost(mm.name).selected(mm.id == m.id).min_size(vec2(240.0, 40.0))).on_hover_text(mm.blurb).clicked() {
                            pick = Some(mm.id);
                        }
                    }
                });
                if let Some(id) = pick {
                    self.cfg.game_modes.insert(CS2, id.to_string());
                    self.apply_mode(id);
                }
                if enabled {
                    let installed = lumen_cs2::find_cfg_dir().map(|d| lumen_cs2::cfg_installed(&d)).unwrap_or(false);
                    let b = if installed { quiet("Reinstall config") } else { ghost("Install config").icon(Icon::Save) };
                    if ui.add(b.height(48.0)).on_hover_text("Adds RokSync's config file to your CS2 folder").clicked() {
                        let r = lumen_cs2::find_cfg_dir()
                            .ok_or_else(|| anyhow::anyhow!("CS2 not found in your Steam libraries"))
                            .and_then(|d| lumen_cs2::install_cfg(&d, self.cfg.cs2.port, &self.cfg.cs2.token));
                        match r {
                            Ok(_) => self.toast("CS2: config installed. Restart CS2 if it's running.", false),
                            Err(e) => self.toast(format!("CS2 setup: {e:#}"), true),
                        }
                    }
                }
            });
        });
        ui.add_space(22.0);
        let full = ui.available_rect_before_wrap();
        let aside_w = 430.0;
        let left = Rect::from_min_max(full.min, pos2(full.right() - aside_w - 36.0, full.bottom()));
        let right = Rect::from_min_max(pos2(left.right() + 36.0, full.top()), full.max);
        ui.allocate_new_ui(UiBuilder::new().max_rect(left), |ui| {
            if !enabled {
                ui.set_opacity(0.6);
            }
            egui::ScrollArea::vertical().id_salt("fx-list").auto_shrink([false, false]).show(ui, |ui| self.fx_list(ui));
        });
        ui.allocate_new_ui(UiBuilder::new().max_rect(right), |ui| {
            egui::ScrollArea::vertical().id_salt("fx-aside").auto_shrink([false, false]).show(ui, |ui| {
                self.fx_preview(ui, inv);
                ui.add_space(24.0);
                self.round_timeline(ui);
                ui.add_space(24.0);
                self.how_effects_play(ui);
            });
        });
        ui.allocate_rect(full, Sense::hover());
        if motion() {
            ui.ctx().request_repaint_after(Duration::from_millis(40));
        }
    }

    fn fx_list(&mut self, ui: &mut egui::Ui) {
        let testing = {
            let s = self.cs2.lock().unwrap();
            s.test.filter(|(f, t0)| t0.elapsed().as_secs_f32() < f.duration()).map(|(f, _)| f)
        };
        let mut fire = None;
        let mut changed = false;
        for (gname, note, list) in GROUPS {
            section_note(ui, gname, note);
            ui.add_space(-2.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            for fx in list.iter().copied() {
                let w = ui.available_width();
                let (r, _) = ui.allocate_exact_size(vec2(w, 56.0), Sense::hover());
                let id = ui.id().with(("fx", fx.id()));
                let hovered = ui.rect_contains_pointer(r);
                let hov = ease_bool(ui, id.with("h"), hovered, 0.15);
                let p = ui.painter().clone();
                p.rect_filled(r, 0.0, mix(BG, HOVER, hov));
                row_rule(&p, r.top(), r.left(), r.right());
                let on = self.cfg.cs2.is_on(fx);
                let op = if on { 1.0 } else { 0.45 };
                let sw = Rect::from_min_size(pos2(r.left(), r.center().y - 20.0), vec2(64.0, 40.0));
                if onoff_at(ui, sw, id.with("on"), on).clicked() {
                    if on {
                        self.cfg.cs2.off.push(fx.id().to_string());
                    } else {
                        self.cfg.cs2.off.retain(|o| o != fx.id());
                    }
                    changed = true;
                }
                let bar = Rect::from_min_max(pos2(sw.right() + 14.0, r.top() + 8.0), pos2(sw.right() + 22.0, r.bottom() - 8.0));
                p.rect_filled(bar, 0.0, c32(fx.color()).gamma_multiply(op));
                let tx = bar.right() + 14.0;
                let test_r = Rect::from_min_max(pos2(r.right() - 76.0, r.center().y - 20.0), pos2(r.right() - 4.0, r.center().y + 20.0));
                let str_r = Rect::from_min_max(pos2(test_r.left() - 14.0 - 132.0, r.center().y - 20.0), pos2(test_r.left() - 14.0, r.center().y + 20.0));
                text_at(&p, pos2(tx, r.center().y - 9.0), Align2::LEFT_CENTER, &fx_name(fx).to_uppercase(), display(19.0), TEXT.gamma_multiply(op), 0.3);
                text_at(&p, pos2(tx, r.center().y + 11.0), Align2::LEFT_CENTER, fx_desc(fx), body(13.5), MUTED.gamma_multiply(op), 0.0);
                let lvl = self.cfg.cs2.level(fx);
                let sresp = ui.interact(str_r, id.with("str"), Sense::click()).on_hover_text(format!("Strength {lvl} of 5"));
                let segw = (str_r.width() - 3.0 * 4.0) / 5.0;
                for n in 1..=5u8 {
                    let s = Rect::from_min_size(pos2(str_r.left() + (n - 1) as f32 * (segw + 3.0), str_r.center().y - 6.0), vec2(segw, 12.0));
                    let fill = if n <= lvl { if on { TEXT } else { MUTED } } else { SURFACE3 };
                    p.rect_filled(s, 0.0, fill.gamma_multiply(if on { 1.0 } else { 0.8 }));
                }
                if sresp.clicked() {
                    if let Some(pos) = sresp.interact_pointer_pos() {
                        let n = (((pos.x - str_r.left()) / (segw + 3.0)).floor() as u8 + 1).clamp(1, 5);
                        self.cfg.cs2.levels.insert(fx.id().to_string(), n);
                        changed = true;
                    }
                }
                let playing = testing == Some(fx);
                let b = if playing { primary("Playing").small().height(40.0) } else { ghost("Test").small().height(40.0) };
                if ui.put(test_r, b.min_size(test_r.size())).on_hover_text("Play it on your lights now").clicked() {
                    fire = Some(fx);
                }
            }
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            row_rule(ui.painter(), r.top(), r.left(), r.right());
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.add_space(22.0);
        }
        if changed {
            self.fx_changed();
        }
        if let Some(fx) = fire {
            self.cs2.lock().unwrap().fire_test(fx, Instant::now());
            if !self.is_streaming() {
                self.toast("Go live on Home to see it on your lights. It plays in the preview here.", false);
            }
        }
    }

    fn fx_preview(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        let area = self.sync_area(inv).or_else(|| inv.ent_configs.first());
        let name = area.map(|a| a.metadata.name.trim().to_uppercase()).unwrap_or_else(|| "PREVIEW".into());
        let mut chans: Vec<ChannelPos> = area
            .map(|a| a.channels.iter().map(|c| ChannelPos { id: c.channel_id, x: c.position.x as f32, y: c.position.y as f32, z: c.position.z as f32 }).collect())
            .unwrap_or_default();
        if chans.is_empty() {
            chans = (0..7).map(|i| ChannelPos { id: i, x: -0.9 + 0.3 * i as f32, y: 0.0, z: 0.0 }).collect();
        }
        chans.truncate(10);
        let now = Instant::now();
        let t = anim_time(ui);
        let (test, ovs) = {
            let s = self.cs2.lock().unwrap();
            let test = s.test.filter(|(f, t0)| t0.elapsed().as_secs_f32() < f.duration()).map(|(f, _)| f);
            (test, s.overlays(now))
        };
        let streamed: Vec<(u8, Rgb)> = self.sync_shared.lock().unwrap().last_frame.clone();
        let mut frame = if streamed.len() >= chans.len() && self.sync_shared.lock().unwrap().running {
            streamed
        } else {
            chans.iter().enumerate().map(|(i, c)| (c.id, [hex("#C9853A"), hex("#7FA9D9"), hex("#9DBBDD"), hex("#B7C7D6"), hex("#8FB4DE"), hex("#8C6538"), hex("#B48A55")][i % 7])).map(|(id, c)| (id, Rgb::from_u8([c.r(), c.g(), c.b()]))).collect()
        };
        let mut ovs = ovs;
        lumen_engine::scale_overlays(&mut ovs, self.cfg.fx_gain);
        lumen_engine::apply_overlays(&mut frame, &ovs);
        let w = ui.available_width();
        let start = ui.cursor().min;
        let bg = ui.painter().add(egui::Shape::Noop);
        let (head, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
        let p = ui.painter();
        text_at(p, pos2(head.left() + 16.0, head.center().y), Align2::LEFT_CENTER, &format!("{name} · PREVIEW"), display(16.0), TEXT, 1.2);
        let (note, nc) = match test {
            Some(f) => (format!("TESTING: {}", fx_name(f).to_uppercase()), ACCENT),
            None if self.is_streaming() => ("FOLLOWING THE SCREEN".to_string(), MUTED),
            None => ("SAMPLE SCENE".to_string(), MUTED),
        };
        text_at(p, pos2(head.right() - 16.0, head.center().y), Align2::RIGHT_CENTER, &note, display(16.0), nc, 1.2);
        let n = frame.len().max(1);
        let (lr, _) = ui.allocate_exact_size(vec2(w, 100.0), Sense::hover());
        let inner = Rect::from_min_max(pos2(lr.left() + 16.0, lr.top()), pos2(lr.right() - 16.0, lr.bottom() - 16.0));
        let lw = (inner.width() - 3.0 * (n as f32 - 1.0)) / n as f32;
        for (i, (id, c)) in frame.iter().enumerate() {
            let r = Rect::from_min_size(pos2(inner.left() + i as f32 * (lw + 3.0), inner.top()), vec2(lw, 64.0));
            ui.painter().rect_filled(r, 0.0, c32(*c));
            text_at(ui.painter(), pos2(r.center().x, r.bottom() + 10.0), Align2::CENTER_CENTER, &format!("{id:02}"), display(12.0), MUTED, 0.0);
        }
        ui.painter().set(bg, egui::Shape::rect_filled(Rect::from_min_max(start, lr.max), 0.0, SURFACE));
        let _ = t;
    }

    fn round_timeline(&mut self, ui: &mut egui::Ui) {
        section_note(ui, "One round, start to win", "what fires when");
        let w = ui.available_width();
        let marks: [(&str, Fx, f32); 6] = [
            ("0:33", Fx::Flash, 48.0),
            ("0:48", Fx::LowHp, 63.0),
            ("0:55", Fx::Burning, 70.0),
            ("1:10", Fx::BombPlanted, 85.0),
            ("1:46", Fx::BombDefused, 121.0),
            ("1:47", Fx::RoundWinCt, 124.0),
        ];
        let (mr, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
        for (i, (_, fx, s)) in marks.iter().enumerate() {
            let x = mr.left() + mr.width() * s / 130.0;
            let (top, h) = if i % 2 == 1 { (12.0, 18.0) } else { (0.0, 30.0) };
            let op = if self.cfg.cs2.is_on(*fx) && self.cfg.cs2.enabled { 1.0 } else { 0.3 };
            ui.painter().rect_filled(Rect::from_min_size(pos2(x - 1.0, mr.top() + top), vec2(3.0, h)), 0.0, c32(fx.color()).gamma_multiply(op));
        }
        let (br, _) = ui.allocate_exact_size(vec2(w, 26.0), Sense::hover());
        let p = ui.painter();
        let seg = |a: f32, b: f32| Rect::from_min_max(pos2(br.left() + br.width() * a, br.top()), pos2(br.left() + br.width() * b, br.bottom()));
        let freeze = seg(0.0, 0.115);
        p.rect_filled(freeze, 0.0, hex("#5D9CEC"));
        text_at(p, pos2(freeze.left() + 6.0, br.center().y), Align2::LEFT_CENTER, "FREEZE", display(12.5), INK, 1.0);
        let live = seg(0.115, 0.654);
        p.rect_filled(live, 0.0, SOFT);
        text_at(p, pos2(live.left() + 6.0, br.center().y), Align2::LEFT_CENTER, "LIVE", display(12.5), INK, 1.0);
        let bomb = seg(0.654, 0.962);
        paint_gradient(p, bomb, &[Rgb::from_u8([255, 176, 0]), Rgb::from_u8([255, 59, 31])], 0.0);
        text_at(p, pos2(bomb.left() + 6.0, br.center().y), Align2::LEFT_CENTER, "BOMB 0:40", display(12.5), INK, 1.0);
        p.rect_filled(seg(0.962, 1.0), 0.0, GOOD);
        ui.add_space(6.0);
        let colw = (w - 18.0) / 2.0;
        ui.spacing_mut().item_spacing.y = 0.0;
        for pair in marks.chunks(2) {
            let (row, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
            for (i, (t, fx, _)) in pair.iter().enumerate() {
                let x = row.left() + i as f32 * (colw + 18.0);
                let p = ui.painter();
                row_rule(p, row.top(), x, x + colw);
                let op = if self.cfg.cs2.is_on(*fx) && self.cfg.cs2.enabled { 1.0 } else { 0.3 };
                text_at(p, pos2(x, row.center().y), Align2::LEFT_CENTER, t, display(14.0), MUTED.gamma_multiply(op), 0.0);
                chip(p, pos2(x + 42.0, row.center().y), c32(fx.color()).gamma_multiply(op));
                text_at(p, pos2(x + 54.0, row.center().y), Align2::LEFT_CENTER, &fx_name(*fx).to_uppercase(), display(15.0), TEXT.gamma_multiply(op), 0.0);
            }
        }
        ui.spacing_mut().item_spacing.y = 8.0;
    }

    fn how_effects_play(&mut self, ui: &mut egui::Ui) {
        section(ui, "How effects play");
        ui.spacing_mut().item_spacing.y = 0.0;
        let w = ui.available_width();
        let mode = self.cfg.game_mode.as_deref().and_then(modes::find).map(|m| m.name).unwrap_or("your settings");
        let mut calm = self.cfg.cs2.calm;
        let rows: [(&str, String); 2] = [
            ("Overall strength", format!("{:.0}% · set by {mode} mode", self.cfg.fx_gain * 100.0)),
            ("Reduce flashing", "No full-room white or strobe. Flashes become soft fades.".to_string()),
        ];
        for (i, (name, desc)) in rows.iter().enumerate() {
            let (r, _) = ui.allocate_exact_size(vec2(w, 60.0), Sense::hover());
            let p = ui.painter();
            row_rule(p, r.top(), r.left(), r.right());
            p.text(pos2(r.left(), r.center().y - 9.0), Align2::LEFT_CENTER, *name, semibold(14.5), TEXT);
            p.text(pos2(r.left(), r.center().y + 10.0), Align2::LEFT_CENTER, desc, body(13.0), MUTED);
            if i == 0 {
                let mr = Rect::from_min_max(pos2(r.right() - 132.0, r.center().y - 6.0), pos2(r.right(), r.center().y + 6.0));
                meter(p, mr, self.cfg.fx_gain, 5, TEXT);
            } else {
                let sw = Rect::from_min_max(pos2(r.right() - 64.0, r.center().y - 20.0), pos2(r.right(), r.center().y + 20.0));
                if onoff_at(ui, sw, ui.id().with("calm"), calm).clicked() {
                    calm = !calm;
                }
            }
        }
        let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
        row_rule(ui.painter(), r.top(), r.left(), r.right());
        ui.spacing_mut().item_spacing.y = 8.0;
        if calm != self.cfg.cs2.calm {
            self.cfg.cs2.calm = calm;
            self.fx_changed();
        }
        let _ = Vec2::ZERO;
    }
}
