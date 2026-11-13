//! Home page: rooms as rows (colour bar, name, brightness, power) that open to their lights and
//! colours, with moods and screen sync on the right.

use super::icons::Icon;
use super::lights::Sel;
use super::moods::MoodItem;
use super::theme::*;
use super::{LumenApp, Page, Pending};
use eframe::egui::{self, pos2, vec2, Align2, Color32, Rect, RichText, Sense, Stroke, UiBuilder, Vec2};
use lumen_hue::{Group, Inventory, Light, LightState, Rgb};
use std::collections::HashMap;
use std::time::Instant;

pub const SWATCHES: [(&str, &str); 11] = [
    ("Warm white", "#FFD29A"),
    ("Neutral white", "#FFF1DE"),
    ("Cool white", "#DCEBFF"),
    ("Red", "#FF2E2E"),
    ("Orange", "#FF6A00"),
    ("Gold", "#FFB000"),
    ("Green", "#00E676"),
    ("Cyan", "#00E5FF"),
    ("Blue", "#2D5BFF"),
    ("Purple", "#7B2FF7"),
    ("Pink", "#FF2BD6"),
];

#[derive(Default)]
pub struct HomeUi {
    pub open: Option<String>,
    opened_at: Option<Instant>,
    pub sel: Option<String>,
    /// Room id → name of the mood last applied there.
    pub room_mood: HashMap<String, String>,
    pub prompt: String,
    /// (room id, 0..1) while a brightness slider is being dragged.
    drag: Option<(String, f32)>,
}

/// Gradient strips return all their points.
pub fn light_colors(l: &Light) -> Vec<Rgb> {
    let g = l.current_gradient();
    if g.len() > 1 {
        g
    } else {
        vec![l.current_rgb()]
    }
}

pub(super) fn room_colors(lights: &[&Light]) -> Vec<Rgb> {
    let mut v: Vec<Rgb> = lights.iter().filter(|l| l.on.on).flat_map(|l| light_colors(l)).filter(|c| c.max_channel() > 0.03).collect();
    v.dedup_by(|a, b| (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs() < 0.05);
    v.truncate(6);
    v
}

fn avg_bri(lights: &[&Light]) -> f32 {
    let on: Vec<f64> = lights.iter().filter(|l| l.on.on).map(|l| l.brightness()).collect();
    if on.is_empty() {
        0.0
    } else {
        (on.iter().sum::<f64>() / on.len() as f64 / 100.0) as f32
    }
}

fn set_xy(l: &mut Light, c: Rgb) {
    let (x, y) = c.to_xy();
    if let Some(col) = l.color.as_mut() {
        col.xy.x = x;
        col.xy.y = y;
    }
    if let Some(g) = l.gradient.as_mut() {
        for p in g.points.iter_mut() {
            p.color.xy.x = x;
            p.color.xy.y = y;
        }
    }
}

/// Mirrors a LightState onto the cached inventory for instant feedback; demo mode relies on it.
pub fn apply_local(inv: &mut Inventory, id: &str, st: &LightState) {
    let Some(l) = inv.lights.iter_mut().find(|l| l.id == id) else { return };
    if let Some(on) = st.on {
        l.on.on = on;
    }
    if let Some(b) = st.brightness {
        if let Some(d) = l.dimming.as_mut() {
            d.brightness = b;
        }
    }
    if let Some(m) = st.mirek {
        l.color_temperature = Some(lumen_hue::model::LightCt { mirek: Some(m as f64), mirek_valid: true });
    } else if st.color.is_some() || st.gradient.is_some() {
        if let Some(ct) = l.color_temperature.as_mut() {
            ct.mirek_valid = false;
        }
    }
    if let Some(c) = st.color {
        set_xy(l, c);
    }
    if let Some(pts) = &st.gradient {
        if let Some(g) = l.gradient.as_mut() {
            for (p, c) in g.points.iter_mut().zip(pts) {
                let (x, y) = c.to_xy();
                p.color.xy.x = x;
                p.color.xy.y = y;
            }
        }
        if let (Some(c), Some(col)) = (pts.first(), l.color.as_mut()) {
            let (x, y) = c.to_xy();
            col.xy.x = x;
            col.xy.y = y;
        }
    }
}

pub(super) fn chevron(p: &egui::Painter, c: egui::Pos2, t: f32, color: Color32) {
    let a = t * std::f32::consts::PI;
    let rot = |v: Vec2| c + vec2(v.x * a.cos() - v.y * a.sin(), v.x * a.sin() + v.y * a.cos());
    let pts = [rot(vec2(-4.5, -2.0)), rot(vec2(0.0, 2.5)), rot(vec2(4.5, -2.0))];
    p.line_segment([pts[0], pts[1]], Stroke::new(2.0_f32, color));
    p.line_segment([pts[1], pts[2]], Stroke::new(2.0_f32, color));
}

impl LumenApp {
    pub fn home_page(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        let full = ui.available_rect_before_wrap();
        let mut top = full.top();
        if let Some(d) = self.games.detected.clone() {
            let strip = Rect::from_min_size(pos2(full.left(), top), vec2(full.width(), 48.0));
            self.detected_strip(ui, inv, strip, &d);
            top = strip.bottom() + 22.0;
        }
        let right_w = 420.0;
        let gap = 36.0;
        let left = Rect::from_min_max(pos2(full.left(), top), pos2(full.right() - right_w - gap, full.bottom()));
        let right = Rect::from_min_max(pos2(left.right() + gap, top), pos2(full.right(), full.bottom()));
        ui.allocate_new_ui(UiBuilder::new().max_rect(left), |ui| self.rooms_column(ui, inv));
        ui.allocate_new_ui(UiBuilder::new().max_rect(right), |ui| {
            egui::TopBottomPanel::bottom("home-sync")
                .frame(egui::Frame::none())
                .show_separator_line(false)
                .resizable(false)
                .show_inside(ui, |ui| {
                    ui.add_space(16.0);
                    self.sync_block(ui, inv);
                });
            egui::CentralPanel::default().frame(egui::Frame::none()).show_inside(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("home-right").auto_shrink([false, false]).show(ui, |ui| self.moods_block(ui, inv));
            });
        });
        ui.allocate_rect(full, Sense::hover());
    }

    fn detected_strip(&mut self, ui: &mut egui::Ui, inv: &Inventory, r: Rect, d: &super::games::Detected) {
        let p = ui.painter().clone();
        p.rect_filled(r, 0.0, SURFACE);
        let mut x;
        let g = p.layout_job(spaced("DETECTED", display_bold(15.0), Color32::PLACEHOLDER, 1.2));
        let b = Rect::from_min_max(r.min, pos2(r.left() + g.size().x + 46.0, r.bottom()));
        p.rect_filled(b, 0.0, ACCENT);
        let dot = pos2(b.left() + 20.0, b.center().y);
        if motion() {
            let t = (ui.input(|i| i.time) as f32 / 1.6).fract();
            let k = ease(t);
            p.rect_stroke(Rect::from_center_size(dot, Vec2::splat(8.0 + 16.0 * k)), 0.0, Stroke::new(2.0_f32, INK.gamma_multiply(0.6 * (1.0 - k))));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
        chip(&p, dot, INK);
        p.galley(pos2(b.left() + 32.0, b.center().y - g.size().y / 2.0), g, INK);
        x = b.right() + 18.0;
        let name = text_at(&p, pos2(x, r.center().y), Align2::LEFT_CENTER, &d.name.to_uppercase(), display_bold(24.0), TEXT, 0.4);
        x = name.right() + 18.0;
        p.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], Stroke::new(1.0_f32, RULE));
        let mode = crate::modes::find(&self.mode_for(d.appid, &d.name)).unwrap_or(&crate::modes::MODES[2]);
        let mut info = mode.name.to_uppercase();
        if let Some(k) = crate::games_fx::native_for(d.appid, &d.name) {
            let on = match k {
                crate::games_fx::Native::Cs2 => self.fx_counts().0,
                crate::games_fx::Native::Dota => self.dota_counts().0,
                crate::games_fx::Native::Cod => self.cod_counts().0,
            };
            info = format!("{info} · {on} GAME EFFECTS ON");
        }
        chip(&p, pos2(x + 22.0, r.center().y), c32(Rgb::from_u8(mode.art[0])));
        text_at(&p, pos2(x + 34.0, r.center().y), Align2::LEFT_CENTER, &info, display(15.0), SOFT, 0.6);

        let live = self.is_streaming();
        let can = self.sync_area(inv).is_some();
        let go_label = if live {
            "STOP GAME LIGHTS".to_string()
        } else if can {
            format!("START GAME LIGHTS IN {}", self.target(inv).name.to_uppercase())
        } else {
            "NO GAME LIGHTS HERE".to_string()
        };
        let gg = p.layout_job(spaced(&go_label, display_bold(18.0), Color32::PLACEHOLDER, 1.0));
        let go = Rect::from_min_max(pos2(r.right() - gg.size().x - 44.0, r.top()), r.max);
        let btn = if live { light(&go_label) } else { primary(&go_label) };
        let resp = ui.add_enabled_ui(live || can, |ui| ui.put(go, btn.height(48.0))).inner;
        if resp.clicked() {
            if live {
                self.stop_live();
            } else {
                let mode_id = self.mode_for(d.appid, &d.name);
                self.apply_mode(&mode_id);
                if let Some((hwnd, title)) = d.window.clone() {
                    self.sy.follow_window(hwnd, title);
                }
                self.go_live(inv);
            }
        }
        let gs = p.layout_job(spaced("GAME SETTINGS", display(16.0), Color32::PLACEHOLDER, 1.0));
        let set = Rect::from_min_max(pos2(go.left() - gs.size().x - 36.0, r.top()), pos2(go.left(), r.bottom()));
        let resp = ui.interact(set, ui.id().with("game-settings"), Sense::click());
        let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
        p.rect_filled(set, 0.0, mix(SURFACE, TEXT, hov));
        p.line_segment([set.left_top(), set.left_bottom()], Stroke::new(1.0_f32, RULE));
        p.galley(set.center() - gs.size() / 2.0, gs, mix(TEXT, INK, hov));
        if resp.clicked() {
            self.games.sel = Some(d.appid);
            self.page = match crate::games_fx::native_for(d.appid, &d.name) {
                Some(crate::games_fx::Native::Cs2) => Page::Game,
                Some(crate::games_fx::Native::Dota) => Page::Dota,
                Some(crate::games_fx::Native::Cod) => Page::Cod,
                None => Page::Games,
            };
        }
    }

    /// The saved choice for this game, else a guess from its appid and name.
    pub fn mode_for(&self, appid: u32, name: &str) -> String {
        self.cfg.game_modes.get(&appid).cloned().unwrap_or_else(|| crate::modes::guess(appid, name).to_string())
    }

    fn rooms_column(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        let t = self.target(inv);
        // a room target shows its own row; the whole home lists every room
        let mut rooms: Vec<&Group> = match t.group {
            Some(g) => vec![g],
            None => inv.rooms.iter().collect(),
        };
        rooms.sort_by_key(|r| r.metadata.name.to_lowercase());
        if let Some(g) = t.group {
            if self.home.open.as_deref() != Some(&g.id) && self.home.opened_at.is_none() {
                self.home.open = Some(g.id.clone());
                self.home.opened_at = Some(Instant::now() - std::time::Duration::from_secs(5));
            }
        }
        let on = t.lights.iter().filter(|l| l.on.on).count();
        let off_label = if t.is_home() { "All off".to_string() } else { format!("{} off", t.name) };
        ui.horizontal(|ui| {
            ui.label(headline("Lights", 44.0));
            ui.add_space(4.0);
            ui.label(RichText::new(format!("{on} of {} on", t.lights.len())).font(body(14.0)).color(MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(ghost(&off_label).height(40.0)).clicked() {
                    match t.group {
                        Some(g) => {
                            let gl = inv.grouped_light_of(g).map(str::to_string);
                            self.room_power(g, &t.lights, gl, false);
                        }
                        None => self.all_off(inv),
                    }
                }
            });
        });
        ui.add_space(10.0);
        let others = t.other_rooms(inv);
        egui::ScrollArea::vertical().id_salt("rooms").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, room) in rooms.iter().enumerate() {
                self.room_row(ui, inv, room, i);
            }
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            rule(ui.painter(), r.top(), r.left(), r.right());
            if !others.is_empty() {
                ui.add_space(24.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("NOT CONTROLLED").font(display(16.0)).color(MUTED).extra_letter_spacing(1.5));
                    ui.add_space(6.0);
                    ui.label(RichText::new("RokSync leaves these alone. Click one to control it.").font(body(13.0)).color(MUTED));
                });
                ui.add_space(8.0);
                let mut pick = None;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    for r in &others {
                        let ls = inv.group_lights(r);
                        let lit = ls.iter().filter(|l| l.on.on).count();
                        if other_chip(ui, &r.metadata.name.trim().to_uppercase(), &room_colors(&ls), lit).clicked() {
                            pick = Some(r.id.clone());
                        }
                    }
                });
                if let Some(id) = pick {
                    self.set_target(inv, &id);
                }
            }
            if t.is_home() {
                let orphans = inv.lights.iter().filter(|l| inv.room_of_light(l).is_none()).count();
                if orphans > 0 {
                    ui.add_space(14.0);
                    ui.label(RichText::new(format!("{} not in a room yet. Add them to one in the Hue app.", plural(orphans, "light"))).color(MUTED));
                }
            }
        });
    }

    fn room_row(&mut self, ui: &mut egui::Ui, inv: &Inventory, room: &Group, idx: usize) {
        let lights = inv.group_lights(room);
        let gl = inv.grouped_light_of(room).map(str::to_string);
        let any_on = lights.iter().any(|l| l.on.on);
        let open = self.home.open.as_deref() == Some(&room.id);
        let w = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(w, 76.0), Sense::hover());
        let id = ui.id().with(("room", &room.id));
        let hovered = ui.rect_contains_pointer(row);
        let p = ui.painter().clone();
        if open {
            // the open room's panel extends below; paint its ground now
            p.rect_filled(row, 0.0, OPENED);
        }
        let hov = ease_bool(ui, id.with("hov"), hovered, 0.15);
        if hov > 0.0 {
            p.rect_filled(row, 0.0, mix(if open { OPENED } else { BG }, HOVER, hov));
        }
        rule(&p, row.top(), row.left(), row.right());
        row_cursor(ui, id, row, hovered);

        let bar = Rect::from_min_size(row.min + vec2(0.0, 1.0), vec2(10.0, row.height() - 1.0));
        let cols = room_colors(&lights);
        if any_on && !cols.is_empty() {
            paint_gradient_v(&p, bar, &cols);
        } else {
            hatch(&p, bar, BG);
        }

        let right = row.right() - 12.0;
        let sw = Rect::from_min_max(pos2(right - 84.0, row.center().y - 20.0), pos2(right, row.center().y + 20.0));
        let val = Rect::from_min_max(pos2(sw.left() - 18.0 - 56.0, row.top()), pos2(sw.left() - 18.0, row.bottom()));
        let sl = Rect::from_min_max(pos2(val.left() - 18.0 - 200.0, row.top()), pos2(val.left() - 18.0, row.bottom()));
        let name_r = Rect::from_min_max(pos2(bar.right() + 18.0, row.top()), pos2(sl.left() - 18.0, row.bottom()));

        let nresp = ui.interact(name_r, id.with("name"), Sense::click());
        let name_col = if any_on { TEXT } else { MUTED };
        let lit = lights.iter().filter(|l| l.on.on).count();
        let nm = text_at(&p, pos2(name_r.left(), name_r.top() + 15.0), Align2::LEFT_TOP, &room.metadata.name.trim().to_uppercase(), display_bold(30.0), name_col, -0.2);
        let rot = ease_bool(ui, id.with("chev"), open, 0.25);
        chevron(&p, pos2(nm.right() + 17.0, nm.center().y + 1.0), rot, MUTED);
        let mood = self.home.room_mood.get(&room.id).map(|m| {
            let mut c = m.to_lowercase();
            if let Some(f) = c.get_mut(0..1) {
                f.make_ascii_uppercase();
            }
            c
        });
        let sub = match (any_on, mood) {
            (true, Some(m)) => format!("{} · {lit} on · {m}", plural(lights.len(), "light")),
            (true, None) => format!("{} · {lit} on", plural(lights.len(), "light")),
            (false, _) => format!("{} · off", plural(lights.len(), "light")),
        };
        p.text(pos2(name_r.left(), nm.bottom() + 3.0), Align2::LEFT_TOP, sub, body(13.5), MUTED);
        if nresp.clicked() {
            if open {
                self.home.open = None;
                self.home.opened_at = Some(Instant::now());
            } else {
                self.home.open = Some(room.id.clone());
                self.home.opened_at = Some(Instant::now());
                self.home.sel = lights.iter().find(|l| l.gradient_points() >= 2).or(lights.first()).map(|l| l.id.clone());
            }
        }

        let shown = match &self.home.drag {
            Some((rid, v)) if rid == &room.id => *v,
            _ => avg_bri(&lights),
        };
        let (sresp, newv) = bri_slider_at(ui, sl, id.with("bri"), shown, any_on && gl.is_some());
        if let (Some(v), Some(gl)) = (newv, &gl) {
            let b = (v * 100.0).clamp(1.0, 100.0) as f64;
            self.home.drag = Some((room.id.clone(), v));
            self.pending = Some(Pending::Group(gl.clone(), LightState { brightness: Some(b), ..Default::default() }));
            let ids: Vec<String> = lights.iter().filter(|l| l.on.on).map(|l| l.id.clone()).collect();
            self.local_edit(|inv| {
                for id in &ids {
                    apply_local(inv, id, &LightState { brightness: Some(b), ..Default::default() });
                }
            });
        }
        if sresp.drag_stopped() || (sresp.clicked() && !sresp.dragged()) {
            if let Some((_, v)) = self.home.drag.take() {
                self.bg.lock().unwrap().note(room.metadata.name.trim(), &format!("Brightness {:.0}%", v * 100.0), TEXT);
            }
        }
        if any_on {
            num_unit(&p, pos2(val.right(), val.center().y), Align2::RIGHT_CENTER, &format!("{:.0}", shown * 100.0), "%", 22.0, name_col);
        } else {
            text_at(&p, pos2(val.right(), val.center().y), Align2::RIGHT_CENTER, "—", display(22.0), name_col, 0.0);
        }

        if onoff_at(ui, sw, id.with("pow"), any_on).clicked() {
            self.room_power(room, &lights, gl.clone(), !any_on);
        }

        if open {
            self.room_panel(ui, inv, room, &lights);
        }
        let _ = idx;
    }

    fn room_power(&mut self, room: &Group, lights: &[&Light], gl: Option<String>, on: bool) {
        let ids: Vec<String> = lights.iter().map(|l| l.id.clone()).collect();
        self.local_edit(|inv| {
            for id in &ids {
                apply_local(inv, id, &LightState::power(on));
            }
        });
        let name = room.metadata.name.trim().to_string();
        self.bg.lock().unwrap().note(&name, if on { "Turned on" } else { "Turned off" }, if on { TEXT } else { EDGE });
        if let Some(gl) = gl {
            self.task("Power", true, move |b| b.set_grouped_light(&gl, &LightState::power(on)).map(|_| String::new()));
        }
    }

    fn all_off(&mut self, inv: &Inventory) {
        let gls: Vec<String> = inv.rooms.iter().filter_map(|r| inv.grouped_light_of(r).map(str::to_string)).collect();
        self.local_edit(|inv| inv.lights.iter_mut().for_each(|l| l.on.on = false));
        self.bg.lock().unwrap().note("Home", "All lights off", EDGE);
        self.task("All off", true, move |b| {
            for gl in &gls {
                b.set_grouped_light(gl, &LightState::power(false))?;
                std::thread::sleep(std::time::Duration::from_millis(110));
            }
            Ok(String::new())
        });
    }

    fn room_panel(&mut self, ui: &mut egui::Ui, inv: &Inventory, _room: &Group, lights: &[&Light]) {
        let since = self.home.opened_at.map(|t| t.elapsed().as_secs_f32()).unwrap_or(10.0);
        let w = ui.available_width();
        let top = ui.cursor().top();
        let bg_idx = ui.painter().add(egui::Shape::Noop);
        let inner = Rect::from_min_max(pos2(ui.cursor().left() + 28.0, top + 4.0), pos2(ui.cursor().left() + w - 12.0, top + 4000.0));
        let mut sel = self.home.sel.clone().filter(|s| lights.iter().any(|l| &l.id == s)).or_else(|| lights.first().map(|l| l.id.clone()));
        let resp = ui.allocate_new_ui(UiBuilder::new().max_rect(inner), |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            let gap = 8.0;
            let iw = ui.available_width();
            let per = (((iw + gap) / (140.0 + gap)).floor() as usize).max(1);
            let tw = (iw - gap * (per as f32 - 1.0)) / per as f32;
            for (ri, chunk) in lights.chunks(per).enumerate() {
                ui.horizontal(|ui| {
                    for (ci, l) in chunk.iter().enumerate() {
                        let i = ri * per + ci;
                        let k = if motion() { ease(((since - 0.06 - i as f32 * 0.035) / 0.4).clamp(0.0, 1.0)) } else { 1.0 };
                        if k < 1.0 {
                            ui.ctx().request_repaint();
                        }
                        ui.scope(|ui| {
                            ui.set_opacity(k);
                            let selected = sel.as_deref() == Some(&l.id);
                            if light_tile_sized(ui, &l.metadata.name, &light_colors(l), l.on.on, l.brightness() as f32 / 100.0, selected, tw).clicked() {
                                sel = Some(l.id.clone());
                            }
                        });
                    }
                });
            }
            ui.add_space(6.0);
            let sl = sel.as_ref().and_then(|id| lights.iter().find(|l| &l.id == id)).copied();
            let label = match sl {
                Some(l) => format!("{} · SET COLOUR", l.metadata.name.trim().to_uppercase()),
                None => "SET COLOUR".into(),
            };
            ui.label(RichText::new(label).font(display(14.0)).color(MUTED).extra_letter_spacing(1.2));
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                let cur = sl.map(|l| l.current_rgb());
                for (n, h) in SWATCHES {
                    let c = Rgb::from_hex(h).unwrap_or(Rgb::WHITE);
                    let r = swatch(ui, c, 40.0).on_hover_text(n);
                    let is_cur = cur.map(|x| (x.r - c.r).abs() + (x.g - c.g).abs() + (x.b - c.b).abs() < 0.06).unwrap_or(false) && sl.map(|l| l.on.on).unwrap_or(false);
                    if is_cur {
                        ui.painter().rect_stroke(r.rect.expand(3.0), 0.0, Stroke::new(2.0_f32, TEXT));
                    }
                    if r.clicked() {
                        if let Some(l) = sl {
                            if l.supports_color() || h.starts_with("#FF") {
                                self.set_light_color(l, c, n);
                            }
                        }
                    }
                }
                if ui.add(ghost("Custom").height(40.0)).clicked() {
                    if let Some(l) = sl {
                        self.lights.sel = Sel::Light(l.id.clone());
                    }
                    self.page = Page::Lights;
                }
            });
        });
        self.home.sel = sel;
        let panel = Rect::from_min_max(pos2(ui.cursor().left(), top), pos2(ui.cursor().left() + w, resp.response.rect.bottom() + 16.0));
        ui.painter().set(bg_idx, egui::Shape::rect_filled(panel, 0.0, OPENED));
        ui.allocate_rect(Rect::from_min_max(pos2(panel.left(), resp.response.rect.bottom()), panel.max), Sense::hover());
        let _ = inv;
    }

    fn set_light_color(&mut self, l: &Light, c: Rgb, name: &str) {
        let id = l.id.clone();
        let st = LightState::color(c);
        self.local_edit(|inv| apply_local(inv, &id, &st));
        self.pending = Some(Pending::Light(id, st));
        self.bg.lock().unwrap().note(l.metadata.name.trim(), &format!("Set to {}", name.to_lowercase()), c32(c));
    }

    fn moods_block(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        let t = self.target(inv);
        let room = t.group.cloned();
        let live_here = self.is_streaming();
        let note = if live_here { "picking one stops sync".to_string() } else { format!("goes to {}", t.name) };
        section_note(ui, "Moods", &note);
        ui.add_space(2.0);
        let items: Vec<MoodItem> = self.mood_items(inv, room.as_ref().map(|r| r.id.as_str())).into_iter().take(6).collect();
        let gap = 8.0;
        let w = ui.available_width();
        let tw = (w - gap) / 2.0;
        let cur = room.as_ref().and_then(|r| self.home.room_mood.get(&r.id)).cloned();
        let mut pick: Option<MoodItem> = None;
        for pair in items.chunks(2) {
            let (row, _) = ui.allocate_exact_size(vec2(w, 72.0), Sense::hover());
            for (i, m) in pair.iter().enumerate() {
                let r = Rect::from_min_size(pos2(row.left() + i as f32 * (tw + gap), row.top()), vec2(tw, 72.0));
                let resp = ui.interact(r, ui.id().with(("mood", &m.name)), Sense::click());
                let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
                let r = if resp.is_pointer_button_down_on() { r.translate(vec2(0.0, 1.0)) } else { r };
                let p = ui.painter();
                p.rect_filled(r, 0.0, mix(SURFACE, SURFACE2, hov));
                let band = Rect::from_min_size(r.min, vec2(r.width(), 34.0));
                paint_gradient(p, band, &m.colors, 0.0);
                lit_edge(p, band);
                text_at(p, pos2(r.left() + 10.0, band.bottom() + 19.0), Align2::LEFT_CENTER, &m.name.to_uppercase(), display(17.0), TEXT, 0.0);
                let on = cur.as_deref().map(|c| c.eq_ignore_ascii_case(&m.name)).unwrap_or(false);
                if on {
                    text_at(p, pos2(r.right() - 10.0, band.bottom() + 19.0), Align2::RIGHT_CENTER, "ON", display(12.0), ACCENT, 1.0);
                    brackets(p, r, ACCENT, 0.0, 2.0);
                }
                if resp.clicked() {
                    pick = Some(m.clone());
                }
            }
            ui.add_space(gap);
        }
        if let Some(m) = pick {
            self.apply_palette(inv, &m.colors, 80.0, &m.name);
            let key = room.as_ref().map(|r| r.id.clone());
            match key {
                Some(k) => {
                    self.home.room_mood.insert(k, m.name.clone());
                }
                None => {
                    for r in &inv.rooms {
                        self.home.room_mood.insert(r.id.clone(), m.name.clone());
                    }
                }
            }
        }
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let te = ui.add(text_input(&mut self.home.prompt, "Describe a mood: late-night rain…").desired_width(ui.available_width() - 96.0 - 24.0));
            let enter = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.add(primary("Create").height(44.0).min_size(vec2(96.0, 44.0))).clicked() || enter {
                self.create_mood_from_home(inv);
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(RichText::new("Or").font(body(12.5)).color(MUTED));
            if ui.add(egui::Label::new(RichText::new("build one by hand").font(body(12.5)).color(TEXT).underline()).sense(Sense::click())).clicked() {
                self.page = Page::Palettes;
            }
            ui.label(RichText::new(".").font(body(12.5)).color(MUTED));
        });
    }

    fn create_mood_from_home(&mut self, inv: &Inventory) {
        let prompt = self.home.prompt.trim().to_string();
        if prompt.is_empty() {
            self.page = Page::Palettes;
            return;
        }
        self.moods.ask = prompt.clone();
        let _ = inv;
        self.home.prompt.clear();
        self.page = Page::Palettes;
        self.moods.pending_create = true;
    }

    fn sync_block(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        let live = self.is_streaming();
        let t = self.target(inv);
        let fit = super::target::fit(inv, &t, self.sy.area.as_deref());
        let area = fit.area().map(|a| a.metadata.name.trim().to_string());
        let detected = self.games.detected.clone();
        let w = ui.available_width();
        let start = ui.cursor().min;
        let bg = ui.painter().add(egui::Shape::Noop);
        egui::Frame::none().inner_margin(egui::Margin { left: 16.0, right: 16.0, top: 14.0, bottom: 12.0 }).show(ui, |ui| {
            ui.set_width(w - 32.0);
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(8.0, 18.0), Sense::hover());
                chip(ui.painter(), r.center(), if live { ACCENT } else { EDGE });
                section(ui, "Screen sync");
            });
            let desc = match &fit {
                super::target::Fit::Fits(a) => format!("{} follows your screen and games. {} lights, all in {}.", a.metadata.name.trim(), super::target::area_lights(inv, a).len(), t.name),
                super::target::Fit::Spills { area, rooms, .. } => {
                    format!("Off here. {} also covers {}, outside {}. Pick a zone that holds all of it, or trim the area in the Hue app.", area.metadata.name.trim(), super::target::join(rooms), t.name)
                }
                super::target::Fit::Home => "Off for the whole home. Pick a room or zone to sync.".into(),
                super::target::Fit::None => format!("{} has no entertainment area. Make one in the Hue app to sync here.", t.name),
            };
            ui.label(RichText::new(desc).font(body(13.5)).color(if matches!(fit, super::target::Fit::Spills { .. }) { WARN } else { SOFT }));
            if let super::target::Fit::Spills { fix: Some(g), .. } = &fit {
                ui.add_space(4.0);
                if ui.add(ghost(&format!("Control {} instead", g.metadata.name.trim())).height(40.0)).clicked() {
                    let id = g.id.clone();
                    self.set_target(inv, &id);
                }
            }
            let note = match &detected {
                Some(d) => {
                    let m = crate::modes::find(&self.mode_for(d.appid, &d.name)).map(|m| m.name).unwrap_or("Cinematic");
                    format!("{} is running. Sync uses {m} mode.", d.name)
                }
                None => "No game running. RokSync picks a mode when you launch one.".into(),
            };
            ui.label(RichText::new(note).font(body(12.5)).color(MUTED));
        });
        let (row, _) = ui.allocate_exact_size(vec2(w, 48.0), Sense::hover());
        let block = Rect::from_min_max(start, row.max);
        ui.painter().set(bg, egui::Shape::rect_filled(block, 0.0, SURFACE));
        rule(ui.painter(), row.top(), row.left(), row.right());
        let half = row.width() / 2.0;
        let mode_r = Rect::from_min_size(row.min, vec2(half, 48.0));
        let go_r = Rect::from_min_max(pos2(row.left() + half, row.top()), row.max);
        let mode_name = self.cfg.game_mode.as_deref().and_then(crate::modes::find).map(|m| m.name).unwrap_or("Custom");
        let mresp = ui.interact(mode_r, ui.id().with("sync-mode"), Sense::click());
        let hov = ease_bool(ui, mresp.id.with("h"), mresp.hovered(), 0.15);
        ui.painter().rect_filled(mode_r, 0.0, mix(SURFACE, TEXT, hov));
        ui.painter().line_segment([mode_r.right_top(), mode_r.right_bottom()], Stroke::new(1.0_f32, RULE));
        text_at(ui.painter(), mode_r.center(), Align2::CENTER_CENTER, &format!("MODE: {}", mode_name.to_uppercase()), display(16.0), mix(TEXT, INK, hov), 1.0);
        let pid = ui.make_persistent_id("sync-mode-pop");
        if mresp.clicked() {
            ui.memory_mut(|m| m.toggle_popup(pid));
        }
        let mut pick = None;
        egui::popup_below_widget(ui, pid, &mresp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(mode_r.width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for m in crate::modes::MODES {
                let sel = self.cfg.game_mode.as_deref() == Some(m.id);
                if ui.add(ghost(m.name).selected(sel).min_size(vec2(mode_r.width(), 40.0))).on_hover_text(m.blurb).clicked() {
                    pick = Some(m.id);
                }
            }
        });
        if let Some(id) = pick {
            self.apply_mode(id);
        }
        let can = area.is_some();
        let go_label = if can { "Go live".to_string() } else { "No sync here".to_string() };
        let btn = if live { light("Stop").icon(Icon::Stop) } else { primary(&go_label).icon(Icon::Play) };
        let go = ui.add_enabled_ui(live || can, |ui| ui.put(go_r, btn.height(48.0).min_size(go_r.size()))).inner;
        if go.clicked() {
            if live {
                self.stop_live();
            } else {
                self.go_live(inv);
                if self.is_streaming() {
                    self.page = Page::Sync;
                }
            }
        }
        if live {
            ui.add_space(8.0);
            if ui.add(quiet("Open live view").icon(Icon::Arrow)).clicked() {
                self.page = Page::Sync;
            }
        }
    }
}

fn other_chip(ui: &mut egui::Ui, name: &str, cols: &[Rgb], lit: usize) -> egui::Response {
    let state = if lit == 0 { "off".to_string() } else { format!("{lit} on") };
    let ng = ui.painter().layout_job(spaced(name, display(16.0), Color32::PLACEHOLDER, 0.4));
    let sg = ui.painter().layout_no_wrap(state, body(12.5), MUTED);
    let w = 6.0 + 12.0 + ng.size().x + 10.0 + sg.size().x + 14.0;
    let (r, resp) = ui.allocate_exact_size(vec2(w, 44.0), Sense::click());
    let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
    let p = ui.painter();
    p.rect_filled(r, 0.0, mix(OPENED, SURFACE2, hov));
    let bar = Rect::from_min_size(r.min, vec2(6.0, r.height()));
    if lit > 0 && !cols.is_empty() {
        paint_gradient_v(p, bar, cols);
    } else {
        hatch(p, bar, OPENED);
    }
    let x = bar.right() + 12.0;
    let nw = ng.size().x;
    p.galley(pos2(x, r.center().y - ng.size().y / 2.0), ng, mix(SOFT, TEXT, hov));
    p.galley(pos2(x + nw + 10.0, r.center().y - sg.size().y / 2.0 + 1.0), sg, MUTED);
    if resp.has_focus() {
        focus_ring(p, r);
    }
    resp.on_hover_text("Control this instead")
}

fn plural(n: usize, w: &str) -> String {
    if n == 1 {
        format!("1 {w}")
    } else {
        format!("{n} {w}s")
    }
}
