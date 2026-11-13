//! Dota 2 and Call of Duty pages. Dota reads Valve's game state integration; CoD has no game
//! data, so its effects are estimated from the screen and the PC's sound.

use super::theme::*;
use super::{LumenApp, Page};
use crate::games_fx::{DOTA_APPID, Native};
use crate::modes::{self, MODES};
use eframe::egui::{self, pos2, vec2, Align2, Color32, Rect, RichText, Sense, Shape, Stroke, UiBuilder};
use lumen_engine::{ChannelPos, Overlay};
use lumen_hue::{Inventory, Rgb};
use std::time::{Duration, Instant};

struct Row<'a> {
    key: &'a str,
    on: bool,
    can_toggle: bool,
    color: Rgb,
    name: String,
    desc: String,
    tag: Option<(&'a str, Color32, Color32)>,
    /// Confidence: (bars out of 3, label, colour).
    conf: Option<(u8, &'a str, Color32)>,
    level: u8,
    level_word: &'a str,
    playing: bool,
    /// The detector is firing right now.
    live: bool,
    height: f32,
}

#[derive(Default)]
struct RowOut {
    toggle: bool,
    level: Option<u8>,
    test: bool,
    hovered: bool,
}

fn fx_row(ui: &mut egui::Ui, s: &Row) -> RowOut {
    let mut out = RowOut::default();
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, s.height), Sense::hover());
    let id = ui.id().with(("fx", s.key));
    out.hovered = ui.rect_contains_pointer(r);
    let hov = ease_bool(ui, id.with("h"), out.hovered, 0.15);
    let p = ui.painter().clone();
    p.rect_filled(r, 0.0, mix(BG, HOVER, hov));
    row_cursor(ui, id.with("cur"), r, out.hovered);
    row_rule(&p, r.top(), r.left(), r.right());
    let op = if s.on { 1.0 } else { 0.45 };
    let sw = Rect::from_min_size(pos2(r.left(), r.center().y - 20.0), vec2(64.0, 40.0));
    let resp = ui.add_enabled_ui(s.can_toggle, |ui| onoff_at(ui, sw, id.with("on"), s.on)).inner;
    if resp.clicked() && s.can_toggle {
        out.toggle = true;
    }
    let bar = Rect::from_min_max(pos2(sw.right() + 14.0, r.top() + 8.0), pos2(sw.right() + 22.0, r.bottom() - 8.0));
    p.rect_filled(bar, 0.0, c32(s.color).gamma_multiply(op));
    if s.live {
        p.rect_stroke(bar.expand(2.0), 0.0, Stroke::new(2.0_f32, TEXT));
    }
    let tx = bar.right() + 14.0;
    let test_r = Rect::from_min_max(pos2(r.right() - 76.0, r.center().y - 20.0), pos2(r.right() - 4.0, r.center().y + 20.0));
    let str_r = Rect::from_min_max(pos2(test_r.left() - 14.0 - 132.0, r.center().y - 20.0), pos2(test_r.left() - 14.0, r.center().y + 20.0));
    let name = text_at(&p, pos2(tx, r.center().y - 10.0), Align2::LEFT_CENTER, &s.name.to_uppercase(), display(19.0), TEXT.gamma_multiply(op), 0.3);
    let mut x = name.right() + 8.0;
    if let Some((t, fill, ink)) = s.tag {
        let g = p.layout_job(spaced(t, display(11.5), Color32::PLACEHOLDER, 1.0));
        let tr = Rect::from_min_size(pos2(x, name.center().y - 9.0), vec2(g.size().x + 10.0, 18.0));
        p.rect_filled(tr, 0.0, fill.gamma_multiply(op));
        p.galley(tr.center() - g.size() / 2.0, g, ink);
        x = tr.right() + 8.0;
    }
    if let Some((n, label, col)) = s.conf {
        for i in 0..3u8 {
            let b = Rect::from_min_size(pos2(x + i as f32 * 6.0, name.center().y - 5.0), vec2(4.0, 10.0));
            p.rect_filled(b, 0.0, if i < n { col } else { SURFACE3 }.gamma_multiply(op));
        }
        text_at(&p, pos2(x + 22.0, name.center().y), Align2::LEFT_CENTER, label, display(11.5), col.gamma_multiply(op), 1.0);
    }
    let desc_w = (str_r.left() - 14.0 - tx).max(60.0);
    let dg = p.layout(s.desc.clone(), body(13.5), MUTED.gamma_multiply(op), desc_w);
    let dg = if dg.rows.len() > 1 { p.layout_no_wrap(truncate_to(&p, &s.desc, desc_w), body(13.5), MUTED.gamma_multiply(op)) } else { dg };
    p.galley(pos2(tx, r.center().y + 3.0), dg, MUTED);
    // five steps: strength (Dota) or sensitivity (CoD)
    let sresp = ui.interact(str_r, id.with("str"), Sense::click()).on_hover_text(format!("{} {} of 5", s.level_word, s.level));
    let segw = (str_r.width() - 3.0 * 4.0) / 5.0;
    for n in 1..=5u8 {
        let sr = Rect::from_min_size(pos2(str_r.left() + (n - 1) as f32 * (segw + 3.0), str_r.center().y - 6.0), vec2(segw, 12.0));
        let fill = if n <= s.level { if s.on { TEXT } else { MUTED } } else { SURFACE3 };
        p.rect_filled(sr, 0.0, fill);
    }
    if sresp.clicked() {
        if let Some(pos) = sresp.interact_pointer_pos() {
            out.level = Some((((pos.x - str_r.left()) / (segw + 3.0)).floor() as u8 + 1).clamp(1, 5));
        }
    }
    if sresp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let b = if s.playing { primary("Playing").small().height(40.0) } else { ghost("Test").small().height(40.0) };
    if ui.put(test_r, b.min_size(test_r.size())).on_hover_text("Play it on your lights now").clicked() {
        out.test = true;
    }
    out
}

fn truncate_to(p: &egui::Painter, s: &str, w: f32) -> String {
    let fits = |t: &str| p.layout_no_wrap(t.to_string(), body(13.5), MUTED).size().x <= w;
    if fits(s) {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut n = chars.len();
    while n > 0 && !fits(&format!("{}…", chars[..n].iter().collect::<String>().trim_end())) {
        n -= 1;
    }
    format!("{}…", chars[..n].iter().collect::<String>().trim_end())
}

fn group_rule(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    row_rule(ui.painter(), r.top(), r.left(), r.right());
}

fn switch_line(ui: &mut egui::Ui, key: &str, name: &str, desc: &str, on: bool) -> bool {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 60.0), Sense::hover());
    let p = ui.painter();
    row_rule(p, r.top(), r.left(), r.right());
    p.text(pos2(r.left(), r.center().y - 9.0), Align2::LEFT_CENTER, name, semibold(14.5), TEXT);
    let g = p.layout(desc.to_string(), body(13.0), MUTED, w - 90.0);
    p.galley(pos2(r.left(), r.center().y + 2.0), g, MUTED);
    let sw = Rect::from_min_max(pos2(r.right() - 64.0, r.center().y - 20.0), pos2(r.right(), r.center().y + 20.0));
    onoff_at(ui, sw, ui.id().with(("switch", key)), on).clicked()
}

/// Sample base colours for the Dota preview when no match is running.
fn dota_sample(base: lumen_dota::Base) -> [&'static str; 7] {
    match base {
        lumen_dota::Base::Cycle => ["#2D5BFF", "#1E3A8A", "#3A00FF", "#2D5BFF", "#1E3A8A", "#3A00FF", "#2D5BFF"],
        lumen_dota::Base::Faction => ["#3FA34D", "#2E7D32", "#3FA34D", "#66BB6A", "#3FA34D", "#2E7D32", "#3FA34D"],
        lumen_dota::Base::Screen => ["#6B8F3A", "#3A5A2E", "#8FA35A", "#C9B27A", "#5A7A3A", "#2E3A2A", "#7A6B4A"],
    }
}

impl LumenApp {
    pub fn dota_counts(&self) -> (usize, usize) {
        let on = if self.cfg.dota.enabled { lumen_dota::Fx::ALL.iter().filter(|f| self.cfg.dota.is_on(f.id())).count() } else { 0 };
        (on, lumen_dota::Fx::ALL.len())
    }

    pub fn cod_counts(&self) -> (usize, usize) {
        let on = if self.cfg.cod.enabled {
            let s = self.gfx.cod.lock().unwrap();
            lumen_cod::Fx::ALL.iter().filter(|f| self.cfg.cod.is_on(f.id()) && s.calibrated_for(**f)).count()
        } else {
            0
        };
        (on, lumen_cod::Fx::ALL.len())
    }

    fn game_fx_changed(&mut self) {
        self.push_game_fx();
        self.save_cfg();
    }

    fn game_title(&mut self, ui: &mut egui::Ui, kicker: Option<&str>, title: &str, tags: &[(String, Color32, Color32)]) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            if ui.add(egui::Label::new(label_caps("← Games", 15.0, MUTED)).sense(Sense::click())).clicked() {
                self.page = Page::Games;
            }
            if let Some(k) = kicker {
                ui.label(label_caps(k, 18.0, MUTED));
            }
            ui.label(headline(title, 76.0));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (t, fill, ink) in tags {
                    tag(ui, t, *fill, *ink);
                }
            });
        });
    }

    fn mode_menu(&mut self, ui: &mut egui::Ui, appid: u32, name: &str) {
        let mode = self.mode_for(appid, name);
        let m = modes::find(&mode).unwrap_or(&MODES[0]);
        let mresp = ui.add(ghost(&format!("Mode: {} ▾", m.name)).height(48.0));
        let pid = ui.make_persistent_id(("game-mode", appid));
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
            self.cfg.game_modes.insert(appid, id.to_string());
            self.apply_mode(id);
        }
    }

    /// Turning it on also lifts a panic-key pause.
    fn master_switch(&mut self, ui: &mut egui::Ui, kind: Native) -> bool {
        let on = match kind {
            Native::Cs2 => self.cfg.cs2.enabled,
            Native::Dota => self.cfg.dota.enabled,
            Native::Cod => self.cfg.cod.enabled,
        } && !self.cfg.fx_paused;
        if labelled_switch(ui, "Game effects", on, 48.0).clicked() {
            self.set_game_fx(Some(kind), !on);
        }
        on
    }

    fn area_preview(&mut self, ui: &mut egui::Ui, inv: &Inventory, mut ovs: Vec<Overlay>, sample: [&str; 7], base: Option<Rgb>, note: (String, Color32)) {
        let area = self.sync_area(inv).or_else(|| inv.ent_configs.first());
        let name = area.map(|a| a.metadata.name.trim().to_uppercase()).unwrap_or_else(|| "PREVIEW".into());
        let mut chans: Vec<ChannelPos> = area
            .map(|a| a.channels.iter().map(|c| ChannelPos { id: c.channel_id, x: c.position.x as f32, y: c.position.y as f32, z: c.position.z as f32 }).collect())
            .unwrap_or_default();
        if chans.is_empty() {
            chans = (0..7).map(|i| ChannelPos { id: i, x: -0.9 + 0.3 * i as f32, y: 0.0, z: 0.0 }).collect();
        }
        chans.truncate(10);
        let streamed: Vec<(u8, Rgb)> = self.sync_shared.lock().unwrap().last_frame.clone();
        let running = self.sync_shared.lock().unwrap().running;
        let mut frame: Vec<(u8, Rgb)> = if running && streamed.len() >= chans.len() {
            streamed
        } else {
            chans
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let h = hex(sample[i % 7]);
                    (c.id, base.unwrap_or(Rgb::from_u8([h.r(), h.g(), h.b()])))
                })
                .collect()
        };
        lumen_engine::scale_overlays(&mut ovs, self.cfg.fx_gain.max(0.6));
        lumen_engine::apply_overlays(&mut frame, &ovs);
        let w = ui.available_width();
        let start = ui.cursor().min;
        let bg = ui.painter().add(egui::Shape::Noop);
        let (head, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
        let p = ui.painter();
        text_at(p, pos2(head.left() + 16.0, head.center().y), Align2::LEFT_CENTER, &format!("{name} · PREVIEW"), display(16.0), TEXT, 1.2);
        text_at(p, pos2(head.right() - 16.0, head.center().y), Align2::RIGHT_CENTER, &note.0, display(16.0), note.1, 1.2);
        let n = frame.len().max(1);
        let (lr, _) = ui.allocate_exact_size(vec2(w, 100.0), Sense::hover());
        let inner = Rect::from_min_max(pos2(lr.left() + 16.0, lr.top()), pos2(lr.right() - 16.0, lr.bottom() - 16.0));
        let lw = (inner.width() - 3.0 * (n as f32 - 1.0)) / n as f32;
        for (i, (id, c)) in frame.iter().enumerate() {
            let r = Rect::from_min_size(pos2(inner.left() + i as f32 * (lw + 3.0), inner.top()), vec2(lw, 64.0));
            ui.painter().rect_filled(r, 0.0, c32(*c));
            lit_edge(ui.painter(), r);
            text_at(ui.painter(), pos2(r.center().x, r.bottom() + 10.0), Align2::CENTER_CENTER, &format!("{id:02}"), display(12.0), MUTED, 0.0);
        }
        ui.painter().set(bg, egui::Shape::rect_filled(Rect::from_min_max(start, lr.max), 0.0, SURFACE));
    }

    fn two_columns(&mut self, ui: &mut egui::Ui, dim: bool, left: impl FnOnce(&mut Self, &mut egui::Ui), right: impl FnOnce(&mut Self, &mut egui::Ui)) {
        let full = ui.available_rect_before_wrap();
        let aside_w = 430.0;
        let l = Rect::from_min_max(full.min, pos2(full.right() - aside_w - 36.0, full.bottom()));
        let r = Rect::from_min_max(pos2(l.right() + 36.0, full.top()), full.max);
        ui.allocate_new_ui(UiBuilder::new().max_rect(l), |ui| {
            if dim {
                ui.set_opacity(0.6);
            }
            egui::ScrollArea::vertical().id_salt("fx-list").auto_shrink([false, false]).show(ui, |ui| left(self, ui));
        });
        ui.allocate_new_ui(UiBuilder::new().max_rect(r), |ui| {
            egui::ScrollArea::vertical().id_salt("fx-aside").auto_shrink([false, false]).show(ui, |ui| right(self, ui));
        });
        ui.allocate_rect(full, Sense::hover());
        if motion() {
            ui.ctx().request_repaint_after(Duration::from_millis(40));
        }
    }

    pub fn dota_page(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        let now = Instant::now();
        let connected = self.gfx.dota.lock().unwrap().connected(now);
        let enabled = self.cfg.dota.enabled && !self.cfg.fx_paused;
        self.settings_probe();
        let installed = self.settings.dota_installed;
        let port = crate::games_fx::dota_port(&self.cfg);
        ui.horizontal(|ui| {
            let status = if self.demo {
                ("Demo".to_string(), SURFACE3, SOFT)
            } else if enabled && connected {
                ("Linked".to_string(), GOOD, INK)
            } else if enabled && installed {
                ("Waiting".to_string(), SURFACE3, SOFT)
            } else if !installed {
                ("Not set up".to_string(), WARN, INK)
            } else {
                ("Off".to_string(), SURFACE3, SOFT)
            };
            let tags = [status, (format!("Game state link · port {port}"), SURFACE2, SOFT)];
            self.game_title(ui, None, "Dota 2", &tags);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                self.master_switch(ui, Native::Dota);
                self.mode_menu(ui, DOTA_APPID, "Dota 2");
                if !self.demo {
                    let b = if installed { quiet("Reinstall config") } else { ghost("Set up Dota 2").icon(super::icons::Icon::Save) };
                    if ui.add(b.height(48.0)).on_hover_text("Adds RokSync's config file to your Dota 2 folder").clicked() {
                        self.setup_dota();
                    }
                }
            });
        });
        // Dota sends no game state until the launch option is set and the game restarted
        let launch = self.settings.dota_launch;
        if !installed || launch == Some(false) {
            ui.add_space(18.0);
            let text = if !installed {
                "Press Set up Dota 2, add -gamestateintegration to Dota's launch options in Steam, then restart Dota."
            } else {
                "Add -gamestateintegration to Dota's launch options (Steam › Dota 2 › Properties › Launch options), then restart Dota."
            };
            self.note_bar(ui, "Set up", text, WARN);
        }
        ui.add_space(22.0);
        self.two_columns(ui, !enabled, |s, ui| s.dota_list(ui), |s, ui| {
            s.dota_preview(ui, inv);
            ui.add_space(24.0);
            s.dota_timeline(ui);
            ui.add_space(24.0);
            s.dota_how(ui);
        });
    }

    fn note_bar(&mut self, ui: &mut egui::Ui, label: &str, text: &str, fill: Color32) {
        let w = ui.available_width();
        let p = ui.painter().clone();
        let lg = p.layout_job(spaced(&label.to_uppercase(), display_bold(15.0), Color32::PLACEHOLDER, 1.2));
        let lw = lg.size().x + 32.0;
        let tg = p.layout(text.to_string(), body(14.0), SOFT, w - lw - 32.0);
        let h = (tg.size().y + 24.0).max(46.0);
        let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
        p.rect_filled(r, 0.0, SURFACE);
        let lr = Rect::from_min_size(r.min, vec2(lw, h));
        p.rect_filled(lr, 0.0, fill);
        p.galley(lr.center() - lg.size() / 2.0, lg, if fill == SURFACE3 { TEXT } else { INK });
        p.galley(pos2(lr.right() + 16.0, r.center().y - tg.size().y / 2.0), tg, SOFT);
    }

    fn dota_list(&mut self, ui: &mut egui::Ui) {
        use lumen_dota::{Fx, Group};
        let testing = {
            let s = self.gfx.dota.lock().unwrap();
            s.test.filter(|(f, t0)| t0.elapsed().as_secs_f32() < f.duration().max(1.8)).map(|(f, _)| f)
        };
        let mut changed = false;
        let mut fire = None;
        for g in Group::ALL {
            section_note(ui, g.label(), g.note());
            ui.add_space(-2.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            for fx in g.fx() {
                let on = self.cfg.dota.is_on(fx.id());
                let row = Row {
                    key: fx.id(),
                    on,
                    can_toggle: true,
                    color: fx.color(),
                    name: fx.label().to_string(),
                    desc: fx.blurb().to_string(),
                    tag: Some(if fx.verified() { ("NEW", ACCENT, INK) } else { ("TO VERIFY", SOFT, INK) }),
                    conf: None,
                    level: self.cfg.dota.level(fx.id(), fx.default_level()),
                    level_word: "Strength",
                    playing: testing == Some(fx),
                    live: false,
                    height: 56.0,
                };
                let o = fx_row(ui, &row);
                if o.toggle {
                    self.cfg.dota.set_on(fx.id(), !on);
                    changed = true;
                }
                if let Some(n) = o.level {
                    self.cfg.dota.levels.insert(fx.id().to_string(), n);
                    changed = true;
                }
                if o.test {
                    fire = Some(fx);
                }
            }
            group_rule(ui);
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.add_space(22.0);
        }
        if changed {
            self.game_fx_changed();
        }
        if let Some(fx) = fire {
            self.gfx.dota.lock().unwrap().fire_test(fx, Instant::now());
            self.test_hint();
        }
        let _ = Fx::ALL;
    }

    fn test_hint(&mut self) {
        if !self.is_streaming() {
            self.toast("Go live on Home to see it on your lights. It plays in the preview here.", false);
        }
    }

    fn dota_preview(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        let now = Instant::now();
        let (ovs, base_now, test, base, clock, live) = {
            let s = self.gfx.dota.lock().unwrap();
            let test = s.test.filter(|(f, t0)| t0.elapsed().as_secs_f32() < f.duration().max(1.8)).map(|(f, _)| f);
            (s.overlays(now), s.base_color(now), test, s.base, s.clock(now), s.connected(now) && s.in_game())
        };
        let note = match test {
            Some(f) => (format!("TESTING: {}", f.label().to_uppercase()), ACCENT),
            None if live => {
                let c = clock.max(0.0) as u32;
                (format!("{} · {}:{:02}", if base == lumen_dota::Base::Faction { "FACTION" } else { "MATCH" }, c / 60, c % 60), MUTED)
            }
            None => (
                match base {
                    lumen_dota::Base::Cycle => "NIGHT · SAMPLE",
                    lumen_dota::Base::Faction => "RADIANT GREEN · SAMPLE",
                    lumen_dota::Base::Screen => "FOLLOWING THE SCREEN",
                }
                .to_string(),
                MUTED,
            ),
        };
        self.area_preview(ui, inv, ovs, dota_sample(base), base_now, note);
    }

    fn dota_timeline(&mut self, ui: &mut egui::Ui) {
        use lumen_dota::Fx;
        section_note(ui, "One match, horn to Ancient", "day and night every 5 min");
        let w = ui.available_width();
        let total = 41.0 * 60.0 + 5.0;
        let marks: [(&str, Fx, &str, f32); 8] = [
            ("0:00", Fx::Horn, "HORN", 0.0),
            ("3:12", Fx::FirstBlood, "FIRST BLOOD", 192.0),
            ("7:40", Fx::Spike, "LEVEL 6", 460.0),
            ("14:05", Fx::Buyback, "BUYBACK", 845.0),
            ("21:30", Fx::Roshan, "ROSHAN KILLED", 1290.0),
            ("29:50", Fx::LowHp, "LOW HEALTH", 1790.0),
            ("33:10", Fx::MultiKill, "RAMPAGE", 1990.0),
            ("41:05", Fx::Victory, "VICTORY", total),
        ];
        let enabled = self.cfg.dota.enabled && !self.cfg.fx_paused;
        let (mr, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
        for (i, (_, fx, _, s)) in marks.iter().enumerate() {
            let x = (mr.left() + mr.width() * s / total).min(mr.right() - 2.0);
            let (top, h) = if i % 2 == 1 { (12.0, 18.0) } else { (0.0, 30.0) };
            let op = if enabled && self.cfg.dota.is_on(fx.id()) { 1.0 } else { 0.3 };
            ui.painter().rect_filled(Rect::from_min_size(pos2(x - 1.0, mr.top() + top), vec2(3.0, h)), 0.0, c32(fx.color()).gamma_multiply(op));
        }
        let (br, _) = ui.allocate_exact_size(vec2(w, 26.0), Sense::hover());
        let p = ui.painter();
        let mut a = 0.0;
        let mut night = false;
        while a < total {
            let b = (a + 300.0f32).min(total);
            let r = Rect::from_min_max(pos2(br.left() + br.width() * a / total, br.top()), pos2(br.left() + br.width() * b / total, br.bottom()));
            let (fill, ink) = if night { (hex("#1E3A8A"), hex("#C9D8FF")) } else { (hex("#E0A43A"), INK) };
            p.rect_filled(r, 0.0, fill);
            if b - a >= 300.0 {
                text_at(&p.with_clip_rect(r), pos2(r.left() + 5.0, r.center().y), Align2::LEFT_CENTER, if night { "NIGHT" } else { "DAY" }, display(12.0), ink, 1.0);
            }
            a = b;
            night = !night;
        }
        let (lr, _) = ui.allocate_exact_size(vec2(w, 18.0), Sense::hover());
        let p = ui.painter();
        text_at(p, lr.left_center(), Align2::LEFT_CENTER, "0:00 HORN", display(12.0), MUTED, 0.0);
        text_at(p, lr.center(), Align2::CENTER_CENTER, "20:00", display(12.0), MUTED, 0.0);
        text_at(p, lr.right_center(), Align2::RIGHT_CENTER, "41:05", display(12.0), MUTED, 0.0);
        ui.add_space(4.0);
        let colw = (w - 18.0) / 2.0;
        ui.spacing_mut().item_spacing.y = 0.0;
        for pair in marks.chunks(2) {
            let (row, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
            for (i, (t, fx, name, _)) in pair.iter().enumerate() {
                let x = row.left() + i as f32 * (colw + 18.0);
                let p = ui.painter();
                row_rule(p, row.top(), x, x + colw);
                let op = if enabled && self.cfg.dota.is_on(fx.id()) { 1.0 } else { 0.3 };
                text_at(p, pos2(x, row.center().y), Align2::LEFT_CENTER, t, display(14.0), MUTED.gamma_multiply(op), 0.0);
                chip(p, pos2(x + 46.0, row.center().y), c32(fx.color()).gamma_multiply(op));
                text_at(p, pos2(x + 58.0, row.center().y), Align2::LEFT_CENTER, name, display(15.0), TEXT.gamma_multiply(op), 0.0);
            }
        }
        ui.spacing_mut().item_spacing.y = 8.0;
    }

    fn dota_how(&mut self, ui: &mut egui::Ui) {
        use lumen_dota::Base;
        section(ui, "How effects play");
        ui.spacing_mut().item_spacing.y = 0.0;
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 52.0), Sense::hover());
        let p = ui.painter();
        row_rule(p, r.top(), r.left(), r.right());
        p.text(pos2(r.left(), r.center().y - 7.0), Align2::LEFT_CENTER, "Between events", semibold(14.5), TEXT);
        p.text(pos2(r.left(), r.center().y + 12.0), Align2::LEFT_CENTER, "What the room shows when nothing is happening", body(13.0), MUTED);
        let mut base = Base::from_id(&self.cfg.dota.base).unwrap_or_default();
        let opts: Vec<(Base, &str)> = Base::ALL.iter().map(|b| (*b, b.label())).collect();
        if segmented_h(ui, &mut base, &opts, 40.0, true) {
            self.cfg.dota.base = base.id().into();
            self.game_fx_changed();
        }
        ui.add_space(14.0);
        if switch_line(ui, "dota-calm", "Reduce flashing", "Kills and stuns become soft fades, never strobes", self.cfg.dota.calm) {
            self.cfg.dota.calm = !self.cfg.dota.calm;
            self.game_fx_changed();
        }
        group_rule(ui);
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.add_space(10.0);
        ui.label(
            RichText::new("TO VERIFY: not yet confirmed in a live match. Some game data only arrives when spectating.")
                .font(body(12.5))
                .color(MUTED),
        );
    }

    pub fn cod_page(&mut self, ui: &mut egui::Ui, _inv: &Inventory) {
        let enabled = self.cfg.cod.enabled && !self.cfg.fx_paused;
        ui.horizontal(|ui| {
            let tags = [
                ("Estimated".to_string(), hex("#FFB000"), INK),
                ("Reads screen and sound only · never touches the game".to_string(), SURFACE2, SOFT),
            ];
            self.game_title(ui, Some("Modern Warfare 4"), "Call of Duty", &tags);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                self.master_switch(ui, Native::Cod);
                let appid = self.games.detected.as_ref().filter(|d| crate::games_fx::native_for(d.appid, &d.name) == Some(Native::Cod)).map(|d| d.appid).unwrap_or(0);
                self.mode_menu(ui, appid, "Call of Duty");
            });
        });
        ui.add_space(20.0);
        self.note_bar(ui, "Why estimated", lumen_cod::WHY_ESTIMATED, SURFACE3);
        if let Some(e) = self.gfx.cod_rt.as_ref().and_then(|r| r.error.clone()) {
            ui.add_space(8.0);
            self.note_bar(ui, "No PC sound", &format!("Sound effects off ({e}). Screen effects still work."), WARN);
        }
        ui.add_space(22.0);
        let focus = self.gfx.cod.lock().unwrap().test.filter(|(f, t0)| t0.elapsed().as_secs_f32() < f.duration().max(1.8)).map(|(f, _)| f);
        let hover_id = ui.id().with("cod-hover");
        let hovered: Option<lumen_cod::Fx> = ui.ctx().data(|d| d.get_temp::<Option<&'static str>>(hover_id)).flatten().and_then(lumen_cod::Fx::from_id);
        let focus = focus.or(hovered);
        self.two_columns(
            ui,
            !enabled,
            |s, ui| {
                let h = s.cod_list(ui);
                ui.ctx().data_mut(|d| d.insert_temp(hover_id, h.map(|f| f.id())));
            },
            |s, ui| {
                s.cod_watch(ui, focus);
                ui.add_space(24.0);
                s.cod_calibrate(ui);
                ui.add_space(24.0);
                s.cod_safety(ui);
            },
        );
    }

    /// Returns the row under the pointer.
    fn cod_list(&mut self, ui: &mut egui::Ui) -> Option<lumen_cod::Fx> {
        use lumen_cod::{Fx, Group};
        let (testing, cal, conf): (Option<Fx>, Vec<bool>, Vec<f32>) = {
            let s = self.gfx.cod.lock().unwrap();
            (
                s.test.filter(|(f, t0)| t0.elapsed().as_secs_f32() < f.duration().max(1.8)).map(|(f, _)| f),
                Fx::ALL.iter().map(|f| s.calibrated_for(*f)).collect(),
                Fx::ALL.iter().map(|f| s.confidence(*f)).collect(),
            )
        };
        let mut changed = false;
        let mut fire = None;
        let mut hovered = None;
        for g in Group::ALL {
            section_note(ui, g.name(), g.note());
            ui.add_space(-2.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            for fx in g.effects().iter().copied() {
                let i = Fx::ALL.iter().position(|f| *f == fx).unwrap_or(0);
                let calibrated = cal[i];
                let on = self.cfg.cod.is_on(fx.id()) && calibrated;
                let (label, col) = fx.reliability_label();
                let mut desc = fx.blurb().to_string();
                if fx.needs_calibration() && !calibrated {
                    desc = format!("{}. Calibrate first.", desc.trim_end_matches('.'));
                }
                let row = Row {
                    key: fx.id(),
                    on,
                    can_toggle: calibrated,
                    color: fx.color(),
                    name: fx.label().to_string(),
                    desc,
                    tag: Some((fx.source().tag(), SURFACE3, SOFT)),
                    conf: Some((fx.reliability(), label, c32(col))),
                    level: self.cfg.cod.level(fx.id(), fx.default_sensitivity()),
                    level_word: "Sensitivity",
                    playing: testing == Some(fx),
                    live: conf[i] >= 0.5,
                    height: 60.0,
                };
                let o = fx_row(ui, &row);
                if o.hovered {
                    hovered = Some(fx);
                }
                if o.toggle {
                    self.cfg.cod.set_on(fx.id(), !on);
                    changed = true;
                }
                if let Some(n) = o.level {
                    self.cfg.cod.levels.insert(fx.id().to_string(), n);
                    changed = true;
                }
                if o.test {
                    fire = Some(fx);
                }
            }
            group_rule(ui);
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.add_space(22.0);
        }
        ui.label(
            RichText::new("Bars set sensitivity. Higher catches more and misfires more. Hover a row to see where RokSync looks.")
                .font(body(12.5))
                .color(MUTED),
        );
        if changed {
            self.game_fx_changed();
        }
        if let Some(fx) = fire {
            self.gfx.cod.lock().unwrap().fire_test(fx, Instant::now());
            self.test_hint();
        }
        hovered
    }

    fn cod_watch(&mut self, ui: &mut egui::Ui, focus: Option<lumen_cod::Fx>) {
        use lumen_cod::Fx;
        section_note(ui, "What RokSync watches", &focus.map(|f| f.label().to_lowercase()).unwrap_or_else(|| "hover an effect".into()));
        ui.add_space(4.0);
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, w * 9.0 / 16.0), Sense::hover());
        let p = ui.painter().with_clip_rect(r);
        let mid = r.top() + r.height() * 0.48;
        let mut m = egui::Mesh::default();
        let quad = |m: &mut egui::Mesh, top: f32, bot: f32, a: Color32, b: Color32| {
            let i = m.vertices.len() as u32;
            m.colored_vertex(pos2(r.left(), top), a);
            m.colored_vertex(pos2(r.right(), top), a);
            m.colored_vertex(pos2(r.left(), bot), b);
            m.colored_vertex(pos2(r.right(), bot), b);
            m.add_triangle(i, i + 1, i + 2);
            m.add_triangle(i + 1, i + 3, i + 2);
        };
        quad(&mut m, r.top(), mid, hex("#2A3138"), hex("#4A4F45"));
        quad(&mut m, mid, r.bottom(), hex("#2E2A22"), hex("#1A1712"));
        p.add(Shape::mesh(m));
        let is = |f: &[Fx]| focus.map(|x| f.contains(&x)).unwrap_or(false);
        if is(&[Fx::Flashed]) {
            p.rect_filled(r, 0.0, Color32::from_white_alpha(215));
        }
        if is(&[Fx::Gas]) {
            p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(170, 170, 170, 150));
        }
        if is(&[Fx::Nvg]) {
            p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(0, 230, 118, 90));
        }
        let edge_op = if is(&[Fx::Damage, Fx::LowHp]) { 1.0 } else if focus.is_some() { 0.08 } else { 0.35 };
        p.rect_stroke(r.shrink(9.0), 0.0, Stroke::new(18.0_f32, Color32::from_rgba_unmultiplied(215, 38, 61, (140.0 * edge_op) as u8)));
        for (fx, col) in [(Fx::Hit, TEXT), (Fx::Kill, ACCENT), (Fx::Streak, hex("#FFB000"))] {
            let op = if focus == Some(fx) { 1.0 } else if focus.is_some() { 0.08 } else { 0.35 };
            for z in lumen_cod::zones(fx) {
                let zr = Rect::from_min_size(pos2(r.left() + z.x * r.width(), r.top() + z.y * r.height()), vec2(z.w * r.width(), z.h * r.height()));
                let pts = [zr.left_top(), zr.right_top(), zr.right_bottom(), zr.left_bottom(), zr.left_top()];
                p.extend(Shape::dashed_line(&pts, Stroke::new(2.0_f32, col.gamma_multiply(op)), 6.0, 4.0));
            }
        }
        let c = r.center();
        let cs = Stroke::new(1.5_f32, TEXT);
        for (a, b) in [((0.0, -7.0), (0.0, -2.0)), ((0.0, 2.0), (0.0, 7.0)), ((-7.0, 0.0), (-2.0, 0.0)), ((2.0, 0.0), (7.0, 0.0))] {
            p.line_segment([c + vec2(a.0, a.1), c + vec2(b.0, b.1)], cs);
        }
        let lg = p.layout_job(spaced("YOUR SCREEN · STAND-IN", display(12.0), Color32::PLACEHOLDER, 1.0));
        let lr = Rect::from_min_size(r.min, lg.size() + vec2(16.0, 8.0));
        p.rect_filled(lr, 0.0, BG);
        p.galley(lr.min + vec2(8.0, 4.0), lg, TEXT);
        ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::from_white_alpha(20)));
        ui.add_space(10.0);
        let levels = self.gfx.cod_rt.as_ref().map(|rt| rt.audio.lock().unwrap().clone());
        let fresh = levels.as_ref().map(|l| l.fresh(Instant::now())).unwrap_or(false);
        let t = anim_time(ui);
        const H: [f32; 16] = [70.0, 92.0, 85.0, 60.0, 44.0, 38.0, 30.0, 34.0, 48.0, 62.0, 74.0, 66.0, 52.0, 40.0, 30.0, 22.0];
        egui::Frame::none().fill(SURFACE).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (eq, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
            let p = ui.painter();
            let n = 16;
            let bw = (eq.width() - 3.0 * (n as f32 - 1.0)) / n as f32;
            let sound_focus = is(&[Fx::Explosion, Fx::Firefight]) || focus.is_none();
            for i in 0..n {
                let base = H[i] / 100.0;
                let k = match &levels {
                    Some(l) if fresh => {
                        let band = if i < 4 { l.low } else if i > 7 { l.high } else { l.mid };
                        (band.clamp(0.0, 1.0) * (0.6 + 0.4 * base)).max(0.04)
                    }
                    _ => base * (0.85 + 0.15 * (t * 2.0 + i as f32 * 0.7).sin()),
                };
                let col = if i < 4 { hex("#FF6A00") } else if i > 7 { hex("#FFB000") } else { EDGE };
                let br = Rect::from_min_max(pos2(eq.left() + i as f32 * (bw + 3.0), eq.bottom() - eq.height() * k), pos2(eq.left() + i as f32 * (bw + 3.0) + bw, eq.bottom()));
                p.rect_filled(br, 0.0, col.gamma_multiply(if sound_focus { 1.0 } else { 0.3 }));
            }
            ui.horizontal(|ui| {
                ui.label(label_caps("Lows · explosions", 12.0, hex("#FF6A00")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(label_caps("Highs · gunfire", 12.0, hex("#FFB000"))));
            });
            let line = match &levels {
                Some(l) if fresh && !l.device.is_empty() => format!("Listening to {}.", l.device),
                _ => "Reads the sound your PC plays. No microphone.".to_string(),
            };
            ui.label(RichText::new(line).font(body(12.5)).color(MUTED));
        });
    }

    fn cod_calibrate(&mut self, ui: &mut egui::Ui) {
        let (step, seen, label) = {
            let s = self.gfx.cod.lock().unwrap();
            (s.calibration.step(), s.calibration.seen(), s.calibration.button_label())
        };
        section_note(ui, "Calibrate", "once, in a private match");
        ui.add_space(-2.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        let w = ui.available_width();
        for (i, (name, desc)) in lumen_cod::calibrate::STEPS.iter().enumerate() {
            let n = i as u8 + 1;
            let (done, cur) = (step > n, step == n);
            let (r, _) = ui.allocate_exact_size(vec2(w, 52.0), Sense::hover());
            let p = ui.painter();
            row_rule(p, r.top(), r.left(), r.right());
            let mr = Rect::from_min_size(pos2(r.left(), r.center().y - 14.0), vec2(28.0, 28.0));
            let (fill, ink) = if done { (GOOD, INK) } else if cur { (ACCENT, INK) } else { (SURFACE3, SOFT) };
            p.rect_filled(mr, 0.0, fill);
            text_at(p, mr.center(), Align2::CENTER_CENTER, &if done { "✓".to_string() } else { n.to_string() }, display_bold(15.0), ink, 0.0);
            p.text(pos2(mr.right() + 12.0, r.center().y - 8.0), Align2::LEFT_CENTER, *name, semibold(14.5), if done || cur { TEXT } else { SOFT });
            p.text(pos2(mr.right() + 12.0, r.center().y + 10.0), Align2::LEFT_CENTER, *desc, body(13.0), MUTED);
            let (state, sc) = if done {
                ("LEARNED", GOOD)
            } else if cur && seen {
                ("CAUGHT IT", GOOD)
            } else if cur {
                ("WAITING…", ACCENT)
            } else {
                ("", MUTED)
            };
            text_at(p, pos2(r.right(), r.center().y), Align2::RIGHT_CENTER, state, display(13.0), sc, 1.0);
        }
        ui.spacing_mut().item_spacing.y = 8.0;
        let b = if step >= 4 { light(label) } else { primary(label) };
        if ui.add(b.height(48.0).min_size(vec2(w, 48.0))).clicked() {
            let next = self.gfx.cod.lock().unwrap().calibration.advance();
            if next == 1 && self.cfg.cod.enabled && !self.is_streaming() {
                self.toast("Calibration: open a private match with CoD in front", false);
            }
            if next >= 4 {
                self.toast("Calibration: done. HUD effects can be switched on.", false);
            }
        }
        if (1..=3).contains(&step) && ui.add(quiet("Cancel").height(36.0)).clicked() {
            self.gfx.cod.lock().unwrap().calibration.cancel();
        }
    }

    fn cod_safety(&mut self, ui: &mut egui::Ui) {
        section(ui, "Safe by design");
        ui.spacing_mut().item_spacing.y = 0.0;
        let (fname, fdesc) = lumen_cod::GUARD_FOCUS;
        if switch_line(ui, "cod-focus", fname, fdesc, self.cfg.cod.only_in_front) {
            self.cfg.cod.only_in_front = !self.cfg.cod.only_in_front;
            self.game_fx_changed();
        }
        let (cname, cdesc) = lumen_cod::GUARD_CALM;
        if switch_line(ui, "cod-calm", cname, cdesc, self.cfg.cod.calm) {
            self.cfg.cod.calm = !self.cfg.cod.calm;
            self.game_fx_changed();
        }
        group_rule(ui);
        ui.add_space(12.0);
        for n in lumen_cod::NEVERS {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
            let p = ui.painter();
            let c = pos2(r.left() + 6.0, r.center().y);
            let s = Stroke::new(2.0_f32, ACCENT);
            p.line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], s);
            p.line_segment([c + vec2(4.0, -4.0), c + vec2(-4.0, 4.0)], s);
            p.text(pos2(r.left() + 22.0, r.center().y), Align2::LEFT_CENTER, n, body(13.5), SOFT);
        }
        ui.spacing_mut().item_spacing.y = 8.0;
    }
}
