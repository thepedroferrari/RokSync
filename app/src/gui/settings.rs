//! Settings page: one scrolling page of numbered sections with a sticky index.

use super::theme::*;
use super::{Link, LumenApp, Page};
use crate::config::{AppConfig, BridgeCfg};
use crate::perf::Perf;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Rect, RichText, Sense, Stroke, UiBuilder};
use std::time::{Duration, Instant};

const INDEX: [(&str, &str); 8] = [
    ("01", "BRIDGE"),
    ("02", "SETUP"),
    ("03", "PERFORMANCE"),
    ("04", "STARTUP"),
    ("05", "GAMES"),
    ("06", "LOCAL MODEL"),
    ("07", "KEYS"),
    ("08", "PRIVACY"),
];

#[derive(Default)]
pub struct SettingsUi {
    pub ask_forget: bool,
    pub ask_remove_model: bool,
    /// Section to scroll to the top on the next frame.
    pub jump: Option<usize>,
    current: usize,
    tested_at: Option<Instant>,
    /// Cached registry read, refreshed every few seconds while the page is open.
    autostart: Option<bool>,
    checked_at: Option<Instant>,
    cs2_installed: bool,
    pub(super) dota_installed: bool,
    /// `-gamestateintegration` in Dota's Steam launch options (None: no Steam user found).
    pub(super) dota_launch: Option<bool>,
    pub battery: Option<bool>,
    battery_at: Option<Instant>,
    /// Setup row opened in place (screen, room, sync area, sound).
    setup_open: Option<usize>,
}

impl LumenApp {
    pub fn set_perf(&mut self, p: Perf) {
        if self.cfg.perf != p {
            self.cfg.perf = p;
            self.save_cfg();
        }
        self.apply_perf();
    }

    /// Effective preset: the chosen one, or ECO while on battery when `eco_on_battery` is set.
    pub fn apply_perf(&mut self) {
        let due = self.settings.battery_at.map(|t| t.elapsed() > Duration::from_secs(10)).unwrap_or(true);
        if due {
            self.settings.battery = crate::system::on_battery();
            self.settings.battery_at = Some(Instant::now());
        }
        let eco = self.cfg.eco_on_battery && self.settings.battery == Some(true);
        crate::perf::set(if eco { Perf::Low } else { self.cfg.perf });
    }

    /// Re-reads state outside config.json (registry, game folders, Steam launch options) at most
    /// every 5 s, never every frame.
    pub(super) fn settings_probe(&mut self) {
        let due = self.settings.checked_at.map(|t| t.elapsed() > Duration::from_secs(5)).unwrap_or(true);
        if !due {
            return;
        }
        self.settings.checked_at = Some(Instant::now());
        self.settings.autostart = Some(!self.demo && crate::system::autostart_enabled());
        self.settings.cs2_installed = self.demo || lumen_cs2::find_cfg_dir().map(|d| lumen_cs2::cfg_installed(&d)).unwrap_or(false);
        self.settings.dota_installed = self.demo || lumen_dota::is_installed();
        // parses every Steam user's localconfig.vdf (often over 1 MB): only the Dota page shows it
        if self.page == Page::Dota {
            self.settings.dota_launch = if self.demo { Some(true) } else { lumen_dota::launch_option_set() };
        }
    }

    pub fn settings_recheck(&mut self) {
        self.settings.checked_at = None;
    }

    pub fn settings_page(&mut self, ui: &mut egui::Ui) {
        self.settings_probe();
        let full = ui.available_rect_before_wrap();
        let index_w = 210.0;
        let left = Rect::from_min_size(full.min, vec2(index_w, full.height()));
        let right = Rect::from_min_max(pos2(full.left() + index_w + 40.0, full.top()), full.max);
        ui.allocate_new_ui(UiBuilder::new().max_rect(left), |ui| self.settings_index(ui));
        ui.allocate_new_ui(UiBuilder::new().max_rect(right), |ui| {
            egui::ScrollArea::vertical().id_salt("settings").auto_shrink([false, false]).show(ui, |ui| {
                ui.set_max_width(1000.0);
                let view_top = ui.clip_rect().top();
                let jump = self.settings.jump.take();
                let mut tops = [0.0f32; 8];
                let sections: [fn(&mut LumenApp, &mut egui::Ui); 8] = [
                    LumenApp::bridge_section,
                    LumenApp::setup_section,
                    LumenApp::performance_section,
                    LumenApp::startup_section,
                    LumenApp::games_section,
                    LumenApp::model_section,
                    LumenApp::keys_section,
                    LumenApp::privacy_section,
                ];
                for (i, body) in sections.into_iter().enumerate() {
                    let r = egui::Frame::none().fill(SURFACE).inner_margin(egui::Margin::same(28.0)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        body(self, ui);
                    });
                    let rect = r.response.rect;
                    tops[i] = rect.top();
                    if jump == Some(i) {
                        ui.scroll_to_rect(rect, Some(egui::Align::TOP));
                    }
                    ui.add_space(14.0);
                }
                ui.add_space(6.0);
                ui.label(label_caps(&format!("ROKSYNC {} · MIT LICENCE", env!("CARGO_PKG_VERSION")), 13.0, FAINT));
                ui.add_space(24.0);
                let view_h = ui.clip_rect().height();
                // the last section can never reach the top: highlight it once it fills the lower half
                self.settings.current = if tops[7] < view_top + view_h * 0.55 { 7 } else { tops.iter().rposition(|t| *t <= view_top + 120.0).unwrap_or(0) };
            });
        });
    }

    fn settings_index(&mut self, ui: &mut egui::Ui) {
        ui.label(headline("Settings", 44.0));
        ui.add_space(18.0);
        for (i, (n, label)) in INDEX.iter().enumerate() {
            let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
            let cur = self.settings.current == i;
            let hov = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered() || cur, 0.2);
            let p = ui.painter();
            if hov > 0.0 {
                let h = (row.height() - 24.0) * ease(hov);
                p.rect_filled(Rect::from_center_size(pos2(row.left() + 1.5, row.center().y), vec2(3.0, h)), 0.0, ACCENT);
            }
            let c = mix(MUTED, TEXT, hov);
            text_at(p, pos2(row.left() + 16.0, row.center().y), Align2::LEFT_CENTER, n, display(14.0), if cur { ACCENT } else { FAINT }, 1.0);
            text_at(p, pos2(row.left() + 46.0, row.center().y), Align2::LEFT_CENTER, label, display(19.0), c, 1.2);
            if resp.has_focus() {
                focus_ring(p, row);
            }
            if resp.clicked() {
                self.settings.jump = Some(i);
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }
    }

    fn bridge_section(&mut self, ui: &mut egui::Ui) {
        let (link, ping, note, moved_from) = {
            let s = self.bg.lock().unwrap();
            (s.link, s.ping_ms, s.link_note.clone(), s.moved_from.clone())
        };
        let (tag, chip_c, msg) = match (self.demo, link, &moved_from) {
            (true, _, _) => ("DEMO", GOOD, "Demo home. Nothing goes to a bridge and nothing is saved."),
            (_, Link::Ok, Some(_)) => (
                "RECONNECTED",
                ACCENT2,
                "The bridge got a new address from your router. RokSync found it and reconnected.",
            ),
            (_, Link::Ok, None) => (
                "CONNECTED",
                GOOD,
                "Paired with this PC.",
            ),
            (_, Link::Searching, _) => ("SEARCHING", WARN, "Looking for your bridge on the network."),
            (_, Link::Unreachable, _) => (
                "CAN'T REACH IT",
                WARN,
                "Bridge not answering. Check it's powered and on the same network. RokSync retries every 30 seconds.",
            ),
            (_, Link::Unknown, _) => ("CONNECTING", WARN, "Connecting to your bridge…"),
        };
        sec_head(ui, 0, "", Some((tag, chip_c)));
        let Some(b) = self.cfg.bridge.clone().or_else(|| self.demo.then(demo_bridge)) else {
            ui.label(RichText::new("No bridge paired.").color(MUTED));
            return;
        };
        ui.add_space(4.0);
        ui.label(label_caps(&b.name, 20.0, TEXT));
        ui.add_space(4.0);
        ui.label(RichText::new(msg).font(body(14.5)).color(SOFT));
        if matches!(link, Link::Unreachable | Link::Searching) && !self.demo {
            ui.add_space(10.0);
            let searching = link == Link::Searching;
            if ui.add_enabled(!searching, primary(if searching { "Searching…" } else { "Try now" }).height(HIT)).clicked() {
                self.bg.lock().unwrap().link = Link::Unknown;
                self.refresh();
                self.relink();
            }
            if !note.is_empty() {
                ui.label(RichText::new(note).font(body(13.0)).color(MUTED));
            }
        }
        ui.add_space(14.0);
        let status = match (link, ping) {
            (Link::Ok, Some(ms)) => format!("{tag} · {ms} MS"),
            _ => tag.to_string(),
        };
        let addr = match &moved_from {
            Some(old) if link == Link::Ok => format!("{} (was {old})", b.ip),
            _ => b.ip.clone(),
        };
        let since = if self.demo { None } else { std::fs::metadata(BridgeCfg::path()).and_then(|m| m.modified()).ok().map(date_label) };
        fact(ui, "STATUS", &status, Some(chip_c), true);
        fact(ui, "ADDRESS", &addr, None, true);
        fact(ui, "BRIDGE ID", &group4(&b.bridge_id), None, true);
        fact(ui, "PAIRING", &since.map(|d| format!("Since {d}")).unwrap_or_else(|| "Saved".into()), Some(GOOD), false);
        ui.add_space(16.0);
        if self.settings.ask_forget {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Forget this bridge? Pairing again needs the button on the bridge.").font(body(14.0)).color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(danger("Forget").height(HIT)).clicked() {
                        self.settings.ask_forget = false;
                        self.forget_bridge();
                    }
                    if ui.add(ghost("Keep it").height(HIT)).clicked() {
                        self.settings.ask_forget = false;
                    }
                });
            });
        } else {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let fresh = self.settings.tested_at.map(|t| t.elapsed() < Duration::from_secs(6)).unwrap_or(false);
                let label = match (fresh, link, ping) {
                    (true, Link::Ok, Some(ms)) => format!("OK · {ms} ms"),
                    (true, Link::Unreachable, _) => "No answer".into(),
                    (true, _, _) => "Testing…".into(),
                    _ => "Test connection".into(),
                };
                if ui.add(ghost(&label).height(HIT)).clicked() && !self.demo {
                    self.settings.tested_at = Some(Instant::now());
                    self.refresh();
                }
                if ui.add_enabled(!self.demo, ghost("Find bridge again").height(HIT)).clicked() {
                    self.relink();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(quiet("Forget bridge…").height(HIT)).clicked() {
                        self.settings.ask_forget = true;
                    }
                });
            });
        }
    }

    fn setup_section(&mut self, ui: &mut egui::Ui) {
        sec_head(ui, 1, "what RokSync uses unless you pick something else", None);
        let Some(inv) = self.bg.lock().unwrap().inv.clone() else {
            ui.label(RichText::new("Waiting for your bridge.").color(MUTED));
            return;
        };
        let (res, screen_note) = self.screen_summary();
        let t = self.target(&inv);
        let room = (t.name.to_uppercase(), format!("{} · {} lights", if t.kind == super::target::Kind::Zone { "Zone" } else { "Room" }, t.lights.len()));
        drop(t);
        let (area, area_note) = self.area_summary(&inv);
        let sound = self.sound_summary();
        let rows = [
            ("SCREEN", res, screen_note),
            ("ROOM", room.0, room.1),
            ("SYNC AREA", area.to_uppercase(), area_note),
            ("SOUND", sound.to_uppercase(), "Sound sync and sound hits in games listen here".to_string()),
        ];
        let w = ui.available_width();
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, (k, v, sub)) in rows.into_iter().enumerate() {
            let (r, _) = ui.allocate_exact_size(vec2(w, 72.0), Sense::hover());
            let p = ui.painter();
            if i > 0 {
                row_rule(p, r.top(), r.left(), r.right());
            }
            text_at(p, pos2(r.left(), r.center().y), Align2::LEFT_CENTER, k, display(14.0), MUTED, 1.2);
            text_at(p, pos2(r.left() + 170.0, r.center().y - 11.0), Align2::LEFT_CENTER, &truncate(&v, 52), display_bold(21.0), TEXT, 0.3);
            text_at(p, pos2(r.left() + 170.0, r.center().y + 13.0), Align2::LEFT_CENTER, &truncate(&sub, 80), body(13.0), MUTED, 0.0);
            let open = self.settings.setup_open == Some(i);
            let br = Rect::from_min_size(pos2(r.right() - 120.0, r.center().y - HIT / 2.0), vec2(120.0, HIT));
            if ui.put(br, ghost(if open { "Done" } else { "Change" }).selected(open).height(HIT)).clicked() {
                self.settings.setup_open = if open { None } else { Some(i) };
            }
            if open {
                ui.horizontal_top(|ui| {
                    ui.add_space(170.0);
                    ui.allocate_ui_with_layout(vec2(w - 170.0, 10.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        match i {
                            0 => self.screen_list(ui),
                            1 => self.room_list(ui, &inv),
                            2 => self.area_list(ui, &inv),
                            _ => self.sound_list(ui),
                        }
                    });
                });
                ui.add_space(12.0);
            }
        }
        let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
        row_rule(ui.painter(), r.top(), r.left(), r.right());
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            if ui.add(ghost("Run setup again").height(HIT)).clicked() {
                self.setup.open = true;
                self.setup.step = 0;
            }
            ui.label(RichText::new("Picks save as you make them. A game with its own mix keeps it.").font(body(13.0)).color(MUTED));
        });
    }

    fn performance_section(&mut self, ui: &mut egui::Ui) {
        sec_head(ui, 2, "smoother lights cost more CPU", None);
        ui.add_space(12.0);
        let gap = 12.0;
        let w = (ui.available_width() - gap * 3.0) / 4.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for p in Perf::ALL {
                let sel = self.cfg.perf == p;
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 168.0), Sense::click());
                let hov = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered(), 0.15);
                let painter = ui.painter();
                painter.rect_filled(rect, 0.0, mix(SURFACE2, SURFACE3, hov * 0.6));
                let top = if sel {
                    ACCENT
                } else if p == Perf::Extreme {
                    hex("#7A2E12")
                } else {
                    SURFACE3
                };
                painter.rect_filled(Rect::from_min_size(rect.min, vec2(rect.width(), 4.0)), 0.0, top);
                if sel {
                    painter.rect_stroke(rect.shrink(1.0), 0.0, Stroke::new(2.0_f32, ACCENT));
                }
                let x = rect.left() + 18.0;
                text_at(painter, pos2(x, rect.top() + 34.0), Align2::LEFT_CENTER, p.name(), display_bold(26.0), TEXT, 1.0);
                text_at(painter, pos2(x, rect.top() + 62.0), Align2::LEFT_CENTER, p.tagline(), body(13.0), MUTED, 0.0);
                let prof = p.profile();
                let stats = [
                    ("CAPTURE", if prof.capture_fps == 0 { "MAX".to_string() } else { prof.capture_fps.to_string() }, if prof.capture_fps == 0 { "" } else { " FPS" }),
                    ("LIGHTS", prof.stream_hz.to_string(), " HZ"),
                    ("SAMPLE", prof.capture_width.to_string(), " PX"),
                ];
                let sw = (rect.width() - 36.0) / 3.0;
                for (k, (key, v, u)) in stats.iter().enumerate() {
                    let sx = x + sw * k as f32;
                    text_at(painter, pos2(sx, rect.bottom() - 54.0), Align2::LEFT_CENTER, key, display(12.0), FAINT, 1.0);
                    num_unit(painter, pos2(sx, rect.bottom() - 28.0), Align2::LEFT_CENTER, v, u, 24.0, TEXT);
                }
                if resp.has_focus() {
                    focus_ring(painter, rect);
                }
                if resp.clicked() {
                    self.set_perf(p);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
        });
        ui.add_space(10.0);
        ui.label(
            RichText::new("In game, the load is screen capture and light updates. The window itself costs almost nothing.")
                .font(body(13.0))
                .color(MUTED),
        );
        ui.add_space(8.0);
        let battery = match self.settings.battery {
            Some(true) => "on battery now",
            Some(false) => "plugged in now",
            None => "no battery on this PC",
        };
        let mut eco = self.cfg.eco_on_battery;
        if switch_row(ui, "Drop to Eco on battery", &format!("Switches back when you plug in · {battery}"), &mut eco) {
            self.cfg.eco_on_battery = eco;
            self.save_cfg();
            self.settings.battery_at = None;
            self.apply_perf();
        }
    }

    fn startup_section(&mut self, ui: &mut egui::Ui) {
        sec_head(ui, 3, "", None);
        ui.add_space(6.0);
        let mut on = self.settings.autostart.unwrap_or(false);
        if switch_row(ui, "Start with Windows", "Opens minimised at sign-in, so game effects are ready", &mut on) {
            if self.demo {
                self.toast("Demo: nothing changed on this PC", false);
            } else {
                match crate::system::set_autostart(on) {
                    Ok(()) => self.settings.autostart = Some(on),
                    Err(e) => self.toast(format!("Start with Windows: {e:#}"), true),
                }
            }
        }
        let mut tray = self.cfg.to_tray;
        if switch_row(ui, "Close to the tray", "Closing or minimising hides RokSync by the clock. Sync and game effects keep running. Quit from the tray icon.", &mut tray) {
            self.cfg.to_tray = tray;
            self.save_cfg();
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new("Lights keep their last colours when RokSync closes.")
                .font(body(13.0))
                .color(MUTED),
        );
    }

    fn games_section(&mut self, ui: &mut egui::Ui) {
        sec_head(ui, 4, "", None);
        ui.add_space(6.0);
        let cs2 = if self.settings.cs2_installed { ("LINKED", GOOD, "Reinstall") } else { ("NOT SET UP", WARN, "Set up") };
        let dota = if self.settings.dota_installed { ("LINKED", GOOD, "Reinstall") } else { ("NOT SET UP", WARN, "Set up") };
        let libs = crate::steam::libraries().len();
        let n = self.games.library_len();
        let rows: [(&str, String, (&str, Color32, &str)); 4] = [
            ("COUNTER-STRIKE 2", format!("Game state file in your CS2 folder · port {}", self.cfg.cs2.port), cs2),
            ("DOTA 2", format!("Game state file in your Dota 2 folder · port {}", crate::games_fx::dota_port(&self.cfg)), dota),
            ("CALL OF DUTY", "Reads screen and sound. Nothing to install.".into(), ("READY", GOOD, "")),
            ("STEAM LIBRARY", format!("{libs} {} found · {n} games", if libs == 1 { "library" } else { "libraries" }), ("FOUND", GOOD, "Rescan")),
        ];
        for (i, (name, desc, (st, c, btn))) in rows.iter().enumerate() {
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
            row_rule(ui.painter(), row.bottom(), row.left(), row.right());
            let p = ui.painter();
            text_at(p, pos2(row.left(), row.center().y - 10.0), Align2::LEFT_CENTER, name, display(19.0), TEXT, 1.0);
            text_at(p, pos2(row.left(), row.center().y + 12.0), Align2::LEFT_CENTER, desc, body(13.0), MUTED, 0.0);
            let bw = 120.0;
            let st_x = row.right() - bw - 24.0;
            let g = text_at(p, pos2(st_x, row.center().y), Align2::RIGHT_CENTER, st, display(15.0), *c, 1.0);
            chip(p, pos2(g.left() - 10.0, row.center().y), *c);
            if !btn.is_empty() {
                let br = Rect::from_min_size(pos2(row.right() - bw, row.center().y - HIT / 2.0), vec2(bw, HIT));
                if ui.put(br, ghost(btn).height(HIT)).clicked() {
                    match i {
                        0 => self.setup_cs2(),
                        1 => self.setup_dota(),
                        3 => {
                            self.games.release();
                            self.settings_recheck();
                            self.toast("Steam library rescanned", false);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn model_section(&mut self, ui: &mut egui::Ui) {
        sec_head(ui, 5, "", None);
        ui.add_space(4.0);
        ui.label(
            RichText::new("Turns a mood you describe into colours. Without it, moods use Quick mode.")
                .font(body(14.5))
                .color(SOFT),
        );
        ui.add_space(14.0);
        let size = |b: u64| if b >= 1_000_000_000 { format!("{:.1} GB", b as f64 / 1e9) } else { format!("{} MB", b / 1_000_000) };
        let model = self.active_model();
        match model {
            Some(m) => {
                ui.label(RichText::new(format!("{} · {} · {}", m.name, size(m.bytes), m.license)).font(body(13.0)).color(MUTED));
                ui.add_space(8.0);
                if self.settings.ask_remove_model {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Remove {}? Moods switch to Quick mode.", m.name)).color(TEXT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(danger("Remove").height(HIT)).clicked() {
                                self.settings.ask_remove_model = false;
                                if self.demo {
                                    self.toast("Demo: nothing removed", false);
                                } else {
                                    match lumen_mood::download::remove_model(m) {
                                        Ok(()) => {
                                            self.cfg.mood_model = None;
                                            self.save_cfg();
                                            self.toast(format!("{} removed", m.name), false);
                                        }
                                        Err(e) => self.toast(format!("Remove model: {e:#}"), true),
                                    }
                                }
                            }
                            if ui.add(ghost("Keep it").height(HIT)).clicked() {
                                self.settings.ask_remove_model = false;
                            }
                        });
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        if ui.add(ghost("Test it").height(HIT)).clicked() {
                            self.page = Page::Mood;
                        }
                        if ui.add(quiet("Remove model").height(HIT)).clicked() {
                            self.settings.ask_remove_model = true;
                        }
                    });
                }
            }
            None => {
                ui.label(RichText::new("No model installed. Moods use Quick mode.").color(MUTED));
                ui.add_space(8.0);
                if ui.add(ghost("Get a model").height(HIT)).clicked() {
                    self.page = Page::Mood;
                }
            }
        }
    }

    fn keys_section(&mut self, ui: &mut egui::Ui) {
        sec_head(ui, 6, "", None);
        ui.add_space(6.0);
        let rows: [(&str, &str, &[&str]); 5] = [
            ("Go live / stop", "in RokSync", &["F"]),
            ("Previous / next section", "in RokSync", &["Q", "E"]),
            ("Jump to a section", "in RokSync", &["1–5"]),
            ("Close a panel", "in RokSync", &["ESC"]),
            ("Stop all effects", "anywhere, even in a game", &["CTRL", "ALT", "L"]),
        ];
        for (what, where_, keys) in rows {
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::hover());
            row_rule(ui.painter(), row.bottom(), row.left(), row.right());
            let p = ui.painter();
            let g = text_at(p, pos2(row.left(), row.center().y), Align2::LEFT_CENTER, what, semibold(15.0), TEXT, 0.0);
            text_at(p, pos2(g.right() + 12.0, row.center().y), Align2::LEFT_CENTER, where_, body(13.0), MUTED, 0.0);
            let mut x = row.right();
            for k in keys.iter().rev() {
                let kr = key_cap(p, pos2(x, row.center().y), k);
                x = kr.left() - 6.0;
            }
        }
        ui.add_space(10.0);
        let mut on = self.cfg.panic_key;
        if switch_row(ui, "Stop-all key", "Ctrl+Alt+L stops sync and pauses game effects, even in a game", &mut on) {
            self.cfg.panic_key = on;
            self.save_cfg();
            if on {
                self.ensure_panic_key();
            }
        }
    }

    fn privacy_section(&mut self, ui: &mut egui::Ui) {
        sec_head(ui, 7, "", None);
        ui.add_space(10.0);
        let promises = [
            ("NO ACCOUNT", "Nothing to sign up for. Find your bridge and play."),
            ("NO TELEMETRY", "No analytics, crash reports or usage stats. Not even anonymous."),
            ("STAYS HOME", "RokSync talks to your bridge and to games on this PC. Nothing else."),
        ];
        let gap = 12.0;
        let w = (ui.available_width() - gap * 2.0) / 3.0;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (t, d) in promises {
                ui.allocate_ui_with_layout(vec2(w, 130.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    egui::Frame::none().fill(SURFACE2).inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
                        ui.set_width(w - 36.0);
                        ui.set_min_height(90.0);
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(10.0, 18.0), Sense::hover());
                            chip(ui.painter(), r.center(), GOOD);
                            ui.label(label_caps(t, 18.0, TEXT));
                        });
                        ui.add_space(4.0);
                        ui.label(RichText::new(d).font(body(13.5)).color(SOFT));
                    });
                });
            }
        });
        ui.add_space(12.0);
        ui.label(
            RichText::new("The only download is the mood model, and only when you ask for it. Cover art comes from Steam on this PC.")
                .font(body(13.0))
                .color(MUTED),
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            if ui.add(ghost("Open data folder").height(HIT)).clicked() {
                if let Some(dir) = AppConfig::path().parent() {
                    let _ = std::fs::create_dir_all(dir);
                    crate::system::reveal(dir);
                }
            }
            if ui.add(ghost("Export settings").height(HIT)).clicked() {
                self.export_settings();
            }
        });
    }

    /// Exports settings with the bridge keys and game tokens stripped.
    fn export_settings(&mut self) {
        let mut c = self.cfg.clone();
        c.bridge = None;
        c.cs2.token.clear();
        c.dota.token.clear();
        let dir = dirs::document_dir().or_else(dirs::desktop_dir).unwrap_or_else(|| std::path::PathBuf::from("."));
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let path = dir.join(format!("RokSync settings {}.json", ymd(now)));
        let r = serde_json::to_string_pretty(&c).map_err(anyhow::Error::from).and_then(|s| crate::config::write_atomic(&path, &s));
        match r {
            Ok(()) => {
                self.toast(format!("Settings exported to {} (no bridge keys)", path.display()), false);
                crate::system::reveal(&path);
            }
            Err(e) => self.toast(format!("Export settings: {e:#}"), true),
        }
    }

    pub fn setup_cs2(&mut self) {
        if self.demo {
            self.toast("Demo: nothing installed", false);
            return;
        }
        if self.cfg.cs2.token.is_empty() {
            self.cfg.cs2.token = lumen_cs2::new_token();
        }
        match lumen_cs2::find_cfg_dir() {
            Some(dir) => match lumen_cs2::install_cfg(&dir, self.cfg.cs2.port, &self.cfg.cs2.token) {
                Ok(_) => {
                    self.cfg.cs2.enabled = true;
                    self.save_cfg();
                    self.start_cs2_listener();
                    self.settings_recheck();
                    self.toast("CS2: linked. Restart CS2 if it's running.", false);
                }
                Err(e) => self.toast(format!("CS2 setup: {e:#}"), true),
            },
            None => self.toast("CS2 setup: not found in your Steam libraries", true),
        }
    }
}

fn sec_head(ui: &mut egui::Ui, i: usize, note: &str, tag: Option<(&str, Color32)>) {
    let (n, title) = INDEX[i];
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    let p = ui.painter();
    let g = text_at(p, pos2(row.left(), row.center().y), Align2::LEFT_CENTER, &format!("{n} · {title}"), display_bold(24.0), TEXT, 1.0);
    if !note.is_empty() {
        text_at(p, pos2(g.right() + 16.0, row.center().y + 2.0), Align2::LEFT_CENTER, note, body(13.5), MUTED, 0.0);
    }
    if let Some((t, c)) = tag {
        let gt = p.layout_job(spaced(t, display(15.0), Color32::PLACEHOLDER, 1.0));
        let r = Rect::from_min_max(pos2(row.right() - gt.size().x - 40.0, row.top()), row.right_bottom());
        p.rect_filled(r, 0.0, SURFACE2);
        chip(p, pos2(r.left() + 14.0, r.center().y), c);
        p.galley(pos2(r.left() + 26.0, r.center().y - gt.size().y / 2.0), gt, TEXT);
    }
    ui.add_space(6.0);
    let (rr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    rule(ui.painter(), rr.top(), rr.left(), rr.right());
    ui.add_space(4.0);
}

fn fact(ui: &mut egui::Ui, k: &str, v: &str, chip_c: Option<Color32>, num: bool) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::hover());
    row_rule(ui.painter(), row.bottom(), row.left(), row.right());
    let p = ui.painter();
    text_at(p, pos2(row.left(), row.center().y), Align2::LEFT_CENTER, k, display(14.0), MUTED, 1.2);
    let x = row.left() + 170.0;
    let mut tx = x;
    if let Some(c) = chip_c {
        chip(p, pos2(x + 4.0, row.center().y), c);
        tx += 16.0;
    }
    let font = if num { display(17.0) } else { body(14.0) };
    text_at(p, pos2(tx, row.center().y), Align2::LEFT_CENTER, v, font, if num { TEXT } else { SOFT }, if num { 0.6 } else { 0.0 });
}

/// Returns true when flipped.
pub fn switch_row(ui: &mut egui::Ui, name: &str, desc: &str, on: &mut bool) -> bool {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
    row_rule(ui.painter(), row.bottom(), row.left(), row.right());
    let p = ui.painter();
    text_at(p, pos2(row.left(), row.center().y - 10.0), Align2::LEFT_CENTER, name, semibold(15.5), TEXT, 0.0);
    text_at(p, pos2(row.left(), row.center().y + 12.0), Align2::LEFT_CENTER, desc, body(13.0), MUTED, 0.0);
    let sw = Rect::from_min_size(pos2(row.right() - 72.0, row.center().y - HIT / 2.0), vec2(72.0, HIT));
    let r = onoff_at(ui, sw, ui.id().with(name), *on);
    if r.clicked() {
        *on = !*on;
        return true;
    }
    false
}

fn key_cap(p: &egui::Painter, right: egui::Pos2, k: &str) -> Rect {
    let g = p.layout_job(spaced(k, display_bold(15.0), Color32::PLACEHOLDER, 0.5));
    let r = Rect::from_min_max(pos2(right.x - g.size().x - 18.0, right.y - 14.0), pos2(right.x, right.y + 14.0));
    p.rect_filled(r, 0.0, SURFACE2);
    p.rect_filled(Rect::from_min_max(pos2(r.left(), r.bottom() - 2.0), r.max), 0.0, SURFACE3);
    p.galley(r.center() - g.size() / 2.0 - vec2(0.0, 1.0), g, TEXT);
    r
}

fn demo_bridge() -> BridgeCfg {
    BridgeCfg { ip: "192.168.1.20".into(), bridge_id: "001788FFFE000000".into(), name: "Hue Bridge".into(), app_key: String::new(), client_key: String::new() }
}

/// "001788FFFE000000" → "0017 88FF FE00 0000"
fn group4(s: &str) -> String {
    let up = s.to_uppercase();
    up.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).to_string()).collect::<Vec<_>>().join(" ")
}

fn date_label(t: std::time::SystemTime) -> String {
    let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (y, m, d) = civil(secs);
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{d} {} {y}", M[(m - 1) as usize])
}

fn ymd(secs: u64) -> String {
    let (y, m, d) = civil(secs);
    format!("{y}-{m:02}-{d:02}")
}

/// Days-from-civil inverse (Howard Hinnant's algorithm), UTC.
fn civil(secs: u64) -> (i64, u32, u32) {
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    #[test]
    fn dates_and_ids() {
        assert_eq!(super::ymd(0), "1970-01-01");
        assert_eq!(super::ymd(1_794_528_000), "2026-11-13");
        assert_eq!(super::group4("001788fffe00abcd"), "0017 88FF FE00 ABCD");
    }
}
