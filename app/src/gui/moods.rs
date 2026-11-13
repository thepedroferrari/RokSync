//! The Moods page: the mood list, the colour editor, and a preview on the target's lights.

use super::palettes::Col;
use super::sync_page::{LookKind, MotionKind};
use super::theme::*;
use super::{LumenApp, Page};
use crate::config::SavedPalette;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Rect, RichText, Sense, Stroke, UiBuilder};
use lumen_hue::{EntConfig, Inventory, Light, Rgb};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Src {
    Yours,
    Model,
    Hue,
    RokSync,
}

impl Src {
    fn tag(self) -> &'static str {
        match self {
            Src::Yours => "YOURS",
            Src::Model => "MODEL",
            Src::Hue => "HUE",
            Src::RokSync => "ROKSYNC",
        }
    }
    fn long(self) -> &'static str {
        match self {
            Src::Yours => "Made by you",
            Src::Model => "Made by the local model",
            Src::Hue => "From your Hue scenes",
            Src::RokSync => "RokSync starter",
        }
    }
}

#[derive(Clone, Debug)]
pub struct MoodItem {
    pub name: String,
    pub colors: Vec<Rgb>,
    pub src: Src,
}

const STARTERS: &[(&str, &[&str])] = &[
    ("Night raid", &["#FF2BD6", "#7B2FF7", "#2D7DFF", "#00E5FF"]),
    ("Arctic ops", &["#E8F6FF", "#7FD3FF", "#0050FF"]),
    ("Molten core", &["#FF2A00", "#FF8A00", "#FFD000"]),
    ("Savanna sunset", &["#FF6A00", "#FF2E63", "#7B2FF7"]),
    ("Deep forest", &["#00FF8A", "#00A36C", "#0A3D2A"]),
    ("Focus", &["#FFF1DE", "#FFE2B8"]),
    ("Nightlight", &["#FF7A3D", "#FF3D00"]),
    ("After hours", &["#3A00FF", "#C2185B", "#FF6A00"]),
];

const POOL: [&str; 14] = [
    "#FF2BD6", "#7B2FF7", "#2D5BFF", "#00E5FF", "#00FF8A", "#A6FF3D", "#FFD000", "#FF8A00", "#FF2A00", "#FF2E63", "#E8F6FF", "#FFD29A", "#3A00FF", "#00A36C",
];

const NAMES: &[(&str, &str)] = &[
    ("#FF2BD6", "Pink"),
    ("#7B2FF7", "Violet"),
    ("#2D7DFF", "Azure"),
    ("#2D5BFF", "Blue"),
    ("#00E5FF", "Cyan"),
    ("#00FF8A", "Mint"),
    ("#A6FF3D", "Lime"),
    ("#FFD000", "Gold"),
    ("#FF8A00", "Amber"),
    ("#FF2A00", "Red"),
    ("#FF2E63", "Rose"),
    ("#E8F6FF", "Ice"),
    ("#FFD29A", "Warm white"),
    ("#3A00FF", "Indigo"),
    ("#00A36C", "Jade"),
    ("#7FD3FF", "Sky"),
    ("#0050FF", "Cobalt"),
    ("#FF6A00", "Orange"),
    ("#0A3D2A", "Pine"),
    ("#FFF1DE", "Soft white"),
    ("#FFE2B8", "Cream"),
    ("#FF7A3D", "Ember"),
    ("#FF3D00", "Flame"),
    ("#C2185B", "Berry"),
    ("#0B1A3A", "Midnight"),
    ("#00C853", "Green"),
    ("#8E24AA", "Purple"),
    ("#FFFFFF", "White"),
];

pub fn colour_name(c: Rgb) -> &'static str {
    let d = |h: &str| {
        let o = Rgb::from_hex(h).unwrap_or(Rgb::WHITE);
        (o.r - c.r).powi(2) * 0.9 + (o.g - c.g).powi(2) * 1.2 + (o.b - c.b).powi(2) * 0.7
    };
    NAMES.iter().min_by(|a, b| d(a.0).partial_cmp(&d(b.0)).unwrap_or(std::cmp::Ordering::Equal)).map(|n| n.1).unwrap_or("Colour")
}

#[derive(Clone, Copy, PartialEq)]
pub enum Motion {
    Still,
    Drift,
    Breathe,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum LightView {
    #[default]
    List,
    Map,
}

#[derive(Default)]
pub struct MoodsUi {
    pub ask: String,
    /// Room or zone name the mood goes to (None = whole home).
    pub target: Option<String>,
    /// Set by Home: run the model once this page is shown.
    pub pending_create: bool,
    sel: Option<String>,
    loaded: Option<String>,
    src: Option<Src>,
    pick: usize,
    motion: Option<Motion>,
    made: Vec<MoodItem>,
    saved_at: Option<Instant>,
    applied: Option<(String, Option<String>)>,
    pub view: LightView,
    pub reveal: bool,
    flash: Option<(String, Instant)>,
}

impl LumenApp {
    /// Every mood, deduplicated by name: yours, the model's, Hue scenes (this room's first), starters.
    pub fn mood_items(&self, inv: &Inventory, room_id: Option<&str>) -> Vec<MoodItem> {
        let mut out: Vec<MoodItem> = Vec::new();
        let mut push = |m: MoodItem| {
            if m.colors.len() >= 2 && !out.iter().any(|o| o.name.eq_ignore_ascii_case(&m.name)) {
                out.push(m);
            }
        };
        for p in &self.cfg.palettes {
            push(MoodItem { name: p.name.clone(), colors: p.colors.iter().filter_map(|h| Rgb::from_hex(h)).collect(), src: Src::Yours });
        }
        for m in &self.moods.made {
            push(m.clone());
        }
        let mut scenes: Vec<_> = inv.scenes.iter().filter(|s| s.colors().len() >= 2).collect();
        scenes.sort_by_key(|s| (room_id.map(|r| s.group.rid != r).unwrap_or(false), s.metadata.name.to_lowercase()));
        for s in scenes {
            push(MoodItem { name: s.metadata.name.trim().to_string(), colors: s.colors(), src: Src::Hue });
        }
        for (n, cols) in STARTERS {
            push(MoodItem { name: n.to_string(), colors: cols.iter().filter_map(|h| Rgb::from_hex(h)).collect(), src: Src::RokSync });
        }
        out
    }

    pub fn moods_page(&mut self, ui: &mut egui::Ui, inv: &Arc<Inventory>) {
        self.keep_warm();
        self.take_model_result();
        self.moods.target = self.target(inv).group.map(|g| g.metadata.name.clone());
        if self.moods.pending_create {
            self.moods.pending_create = false;
            self.ask_model(inv);
        }
        let items = self.mood_items(inv, None);
        if self.moods.sel.is_none() {
            if let Some(first) = items.first() {
                self.moods.sel = Some(first.name.clone());
            }
        }
        if self.moods.loaded != self.moods.sel {
            if let Some(m) = items.iter().find(|m| Some(&m.name) == self.moods.sel.as_ref()) {
                self.studio.set_colors(&m.colors);
                self.studio.history.clear();
                self.studio.name = m.name.clone();
                self.moods.src = Some(m.src);
                self.moods.pick = 0;
            }
            self.moods.loaded = self.moods.sel.clone();
        }
        self.studio_keys(ui);

        let full = ui.available_rect_before_wrap();
        let lw = 300.0;
        let left = Rect::from_min_max(full.min, pos2(full.left() + lw, full.bottom()));
        let right = Rect::from_min_max(pos2(left.right() + 36.0, full.top()), full.max);
        ui.allocate_new_ui(UiBuilder::new().max_rect(left), |ui| self.mood_list(ui, inv, &items));
        const BAR: f32 = 52.0;
        let bar = Rect::from_min_max(pos2(right.left(), right.bottom() - BAR), right.max);
        let edit = Rect::from_min_max(right.min, pos2(right.right(), bar.top() - 12.0));
        ui.allocate_new_ui(UiBuilder::new().max_rect(edit), |ui| {
            egui::ScrollArea::vertical().id_salt("mood-editor").auto_shrink([false, false]).show(ui, |ui| self.mood_editor(ui, inv));
        });
        ui.allocate_new_ui(UiBuilder::new().max_rect(bar), |ui| self.mood_actions(ui, inv));
        ui.allocate_rect(full, Sense::hover());
    }

    fn studio_keys(&mut self, ui: &egui::Ui) {
        if ui.ctx().memory(|m| m.focused().is_some()) {
            return;
        }
        let (space, undo) = ui.input(|i| (i.key_pressed(egui::Key::Space), i.modifiers.command && i.key_pressed(egui::Key::Z)));
        if space {
            self.studio.generate();
            self.moods.applied = None;
        }
        if undo {
            self.studio.undo();
        }
    }

    fn ask_model(&mut self, inv: &Arc<Inventory>) {
        let ask = self.moods.ask.trim().to_string();
        if ask.is_empty() {
            return;
        }
        self.mood.prompt = ask;
        self.mood.scope = self.moods.target.clone();
        self.generate(inv.clone());
    }

    /// A finished plan (from the model or the offline keyword fallback) becomes a new mood at the top of the list.
    fn take_model_result(&mut self) {
        let plan = self.mood.state.lock().unwrap().plan.take();
        let Some(p) = plan else { return };
        let colors: Vec<Rgb> = p.palette.iter().filter_map(|h| Rgb::from_hex(h)).collect();
        if colors.len() < 2 {
            return;
        }
        let mut name = p.title.trim().to_string();
        if name.is_empty() {
            name = self.moods.ask.trim().to_string();
        }
        self.moods.made.retain(|m| !m.name.eq_ignore_ascii_case(&name));
        let src = if self.mood.used_ai { Src::Model } else { Src::RokSync };
        self.moods.made.insert(0, MoodItem { name: name.clone(), colors, src });
        self.moods.motion = Some(match p.motion.as_str() {
            "drift" => Motion::Drift,
            "breathe" => Motion::Breathe,
            _ => Motion::Still,
        });
        self.moods.sel = Some(name.clone());
        self.moods.loaded = None;
        self.moods.applied = None;
        self.bg.lock().unwrap().note(&name, if self.mood.used_ai { "Made by the local model" } else { "Made in Quick mode" }, ACCENT);
    }

    fn mood_list(&mut self, ui: &mut egui::Ui, inv: &Arc<Inventory>, items: &[MoodItem]) {
        ui.label(headline("Moods", 44.0));
        ui.add_space(4.0);
        let busy = self.mood.state.lock().unwrap().busy;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let te = ui.add(text_input(&mut self.moods.ask, "Describe one: rainy neon alley…").desired_width(ui.available_width() - 92.0 - 24.0));
            let enter = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let label = if busy { "…" } else { "Create" };
            if (ui.add_enabled(!busy, primary(label).small().height(44.0).min_size(vec2(92.0, 44.0))).clicked() || enter) && !busy {
                self.ask_model(inv);
            }
        });
        let status = {
            let s = self.mood.state.lock().unwrap();
            if let Some(e) = &s.err {
                (e.clone(), BAD)
            } else if s.busy {
                (if s.status.is_empty() { "Thinking…".to_string() } else { s.status.clone() }, ACCENT)
            } else {
                (String::new(), MUTED)
            }
        };
        ui.label(RichText::new(status.0).font(body(12.5)).color(status.1));
        ui.add_space(10.0);
        egui::ScrollArea::vertical().id_salt("mood-list").auto_shrink([false, false]).max_height(ui.available_height() - 40.0).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for m in items {
                let w = ui.available_width();
                let (r, resp) = ui.allocate_exact_size(vec2(w, 52.0), Sense::click());
                let sel = self.moods.sel.as_deref() == Some(m.name.as_str());
                let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
                let p = ui.painter();
                p.rect_filled(r, 0.0, if sel { SURFACE2 } else { mix(BG, HOVER, hov) });
                row_rule(p, r.top(), r.left(), r.right());
                if sel {
                    p.rect_filled(Rect::from_min_size(r.min + vec2(0.0, 1.0), vec2(4.0, r.height() - 1.0)), 0.0, ACCENT);
                }
                let strip = Rect::from_min_size(pos2(r.left() + 16.0, r.center().y - 12.0), vec2(56.0, 24.0));
                let n = m.colors.len().max(1) as f32;
                for (i, c) in m.colors.iter().enumerate() {
                    let cr = Rect::from_min_size(pos2(strip.left() + strip.width() * i as f32 / n, strip.top()), vec2(strip.width() / n + 0.5, 24.0));
                    p.rect_filled(cr, 0.0, c32(*c));
                }
                let tag = text_at(p, pos2(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, m.src.tag(), display(12.0), MUTED, 1.0);
                let max_chars = ((tag.left() - strip.right() - 24.0) / 8.0) as usize;
                text_at(p, pos2(strip.right() + 12.0, r.center().y), Align2::LEFT_CENTER, &truncate(&m.name.to_uppercase(), max_chars), display(17.0), TEXT, 0.0);
                if resp.clicked() {
                    self.moods.sel = Some(m.name.clone());
                    self.moods.applied = None;
                }
            }
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            row_rule(ui.painter(), r.top(), r.left(), r.right());
        });
        ui.add_space(8.0);
        if ui.add(quiet("Local model settings").small()).clicked() {
            self.page = Page::Mood;
        }
    }

    fn mood_editor(&mut self, ui: &mut egui::Ui, inv: &Arc<Inventory>) {
        let src = self.moods.src.unwrap_or(Src::Yours);
        let edited = !self.studio.history.is_empty();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                kicker(ui, &format!("Editing · {}", if edited && src != Src::Yours { "your copy" } else { src.long() }));
                ui.add(egui::TextEdit::singleline(&mut self.studio.name).font(display_bold(64.0)).text_color(TEXT).frame(false).desired_width(ui.available_width().min(560.0)).margin(vec2(0.0, 0.0)));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let can_add = self.studio.cols.len() < 6;
                if ui.add_enabled(can_add, ghost("+ Colour").height(44.0)).clicked() {
                    self.studio.snapshot();
                    let c = Rgb::from_hex(POOL[(self.studio.cols.len() * 5) % POOL.len()]).unwrap_or(Rgb::WHITE);
                    self.studio.cols.push(Col { c: c.to_u8(), locked: false });
                    self.moods.pick = self.studio.cols.len() - 1;
                    self.moods.applied = None;
                }
                if ui.add_enabled(!self.studio.history.is_empty(), ghost("Undo").height(44.0)).on_hover_text("Ctrl+Z").clicked() {
                    self.studio.undo();
                }
                if ui.add(ghost("Roll unlocked").height(44.0)).on_hover_text("Space").clicked() {
                    self.studio.generate();
                    self.moods.applied = None;
                }
            });
        });
        ui.add_space(14.0);
        self.columns(ui);
        ui.add_space(14.0);
        self.pick_row(ui);
        ui.add_space(18.0);
        let w = ui.available_width();
        let colw = (w - 32.0) / 2.0;
        let motion = self.moods.motion.unwrap_or(Motion::Still);
        let mut new_motion = motion;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 32.0;
            ui.allocate_ui(vec2(colw, 120.0), |ui| {
                ui.set_width(colw);
                ui.vertical(|ui| {
                    section(ui, "Motion");
                    segmented_h(ui, &mut new_motion, &[(Motion::Still, "Still"), (Motion::Drift, "Slow drift"), (Motion::Breathe, "Breathe")], 44.0, true);
                    ui.add_space(4.0);
                    let note = match new_motion {
                        Motion::Still => "Colours hold still. Stays on after RokSync closes.",
                        Motion::Drift => "Colours move slowly between lights. Needs RokSync open.",
                        Motion::Breathe => "Brightness rises and falls every 8 seconds or so. Needs RokSync open.",
                    };
                    ui.label(RichText::new(note).font(body(13.0)).color(MUTED));
                });
            });
            ui.allocate_ui(vec2(colw, 120.0), |ui| {
                ui.set_width(colw);
                ui.vertical(|ui| {
                    section(ui, "Goes to");
                    let t = self.target(inv);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                        let _ = block_choice(ui, &t.name.to_uppercase(), true);
                        if ui.add(ghost("Change").height(44.0)).on_hover_text("Pick the room or zone RokSync controls").clicked() {
                            self.target_ui.open = true;
                        }
                    });
                });
            });
        });
        if new_motion != motion {
            self.moods.motion = Some(new_motion);
            self.moods.applied = None;
        }
        ui.add_space(18.0);
        self.landing(ui, inv);
        ui.add_space(20.0);
    }

    fn columns(&mut self, ui: &mut egui::Ui) {
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(w, 300.0), Sense::hover());
        let n = self.studio.cols.len().max(1);
        let gap = 4.0;
        let cw = (w - gap * (n as f32 - 1.0)) / n as f32;
        let mut remove = None;
        for i in 0..n.min(self.studio.cols.len()) {
            let col = self.studio.cols[i];
            let r = Rect::from_min_size(pos2(rect.left() + i as f32 * (cw + gap), rect.top()), vec2(cw, 300.0));
            let id = ui.id().with(("col", i));
            let resp = ui.interact(Rect::from_min_max(r.min, pos2(r.right(), r.bottom() - 44.0)), id, Sense::click());
            let c = Rgb::from_u8(col.c);
            let shown = [0, 1, 2].map(|k| ui.ctx().animate_value_with_time(id.with(("c", k)), col.c[k] as f32, if motion() { 0.35 } else { 0.0 }));
            let p = ui.painter();
            p.rect_filled(r, 0.0, Color32::from_rgb(shown[0] as u8, shown[1] as u8, shown[2] as u8));
            lit_edge(p, r);
            if self.moods.pick == i {
                p.rect_stroke(r.shrink(1.5), 0.0, Stroke::new(3.0_f32, TEXT));
            }
            let band = Rect::from_min_max(pos2(r.left(), r.bottom() - 44.0), r.max);
            p.rect_filled(band, 0.0, Color32::from_rgba_premultiplied(12, 12, 14, 230));
            let mut bx = band.right();
            if n > 2 {
                let xr = Rect::from_min_max(pos2(bx - 44.0, band.top()), band.max);
                let xresp = ui.interact(xr, id.with("x"), Sense::click()).on_hover_text("Remove colour");
                let h = ease_bool(ui, xresp.id.with("h"), xresp.hovered(), 0.15);
                ui.painter().rect_filled(xr, 0.0, mix(Color32::TRANSPARENT, SURFACE3, h));
                text_at(ui.painter(), xr.center(), Align2::CENTER_CENTER, "×", body(20.0), SOFT, 0.0);
                if xresp.clicked() {
                    remove = Some(i);
                }
                bx = xr.left();
            }
            let lt = if col.locked { "LOCKED" } else { "LOCK" };
            let gl = ui.painter().layout_job(spaced(lt, display_bold(13.0), Color32::PLACEHOLDER, 1.0));
            let lr = Rect::from_min_max(pos2(bx - gl.size().x - 24.0, band.top()), pos2(bx, band.bottom()));
            let lresp = ui.interact(lr, id.with("lock"), Sense::click());
            let h = ease_bool(ui, lresp.id.with("h"), lresp.hovered(), 0.15);
            let p = ui.painter();
            p.rect_filled(lr, 0.0, if col.locked { TEXT } else { mix(Color32::TRANSPARENT, SURFACE3, h) });
            p.galley(lr.center() - gl.size() / 2.0, gl, if col.locked { INK } else { SOFT });
            let label_w = lr.left() - band.left() - 20.0;
            let label = colour_name(c).to_uppercase();
            let label = if label_w < 60.0 { String::new() } else { truncate(&label, (label_w / 8.5) as usize) };
            text_at(p, pos2(band.left() + 12.0, band.center().y), Align2::LEFT_CENTER, &label, display(17.0), TEXT, 0.8);
            if lresp.clicked() {
                self.studio.cols[i].locked = !col.locked;
            }
            if resp.clicked() {
                self.moods.pick = i;
            }
            resp.on_hover_text(c.to_hex());
        }
        if let Some(i) = remove {
            self.studio.snapshot();
            self.studio.cols.remove(i);
            self.moods.pick = self.moods.pick.min(self.studio.cols.len() - 1);
            self.moods.applied = None;
        }
    }

    fn pick_row(&mut self, ui: &mut egui::Ui) {
        let pick = self.moods.pick.min(self.studio.cols.len().saturating_sub(1));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
            ui.add_sized(vec2(0.0, 40.0), egui::Label::new(label_caps(&format!("Colour {} · pick", pick + 1), 14.0, MUTED)));
            ui.add_space(4.0);
            for h in POOL {
                let c = Rgb::from_hex(h).unwrap_or(Rgb::WHITE);
                if swatch(ui, c, 40.0).on_hover_text(colour_name(c)).clicked() && pick < self.studio.cols.len() {
                    self.studio.snapshot();
                    self.studio.cols[pick].c = c.to_u8();
                    self.moods.applied = None;
                }
            }
            if pick < self.studio.cols.len() {
                let mut c = self.studio.cols[pick].c;
                let before = c;
                ui.scope(|ui| {
                    ui.spacing_mut().interact_size = vec2(40.0, 40.0);
                    egui::color_picker::color_edit_button_srgb(ui, &mut c).on_hover_text("Any colour");
                });
                if c != before {
                    if self.studio.history.last().map(|h| h.get(pick).map(|x| x.c) != Some(before)).unwrap_or(true) {
                        self.studio.snapshot();
                    }
                    self.studio.cols[pick].c = c;
                    self.moods.applied = None;
                }
            }
        });
    }

    fn landing(&mut self, ui: &mut egui::Ui, inv: &Arc<Inventory>) {
        let colors = self.studio.colors();
        let t = self.target(inv);
        let lights = t.lights.clone();
        let palette: Vec<String> = colors.iter().map(|c| c.to_hex()).collect();
        let plan = lumen_mood::MoodPlan {
            title: self.studio.name.clone(),
            assignments: lumen_mood::plan::arrange(&lights, &palette, self.studio.brightness as f64, None),
            palette: palette.clone(),
            motion: "still".into(),
            speed: "slow".into(),
        };
        let states = plan.to_states(inv);
        let now = anim_time(ui);
        let motion = self.moods.motion.unwrap_or(Motion::Still);
        let flashing = self.moods.flash.as_ref().filter(|(_, t0)| t0.elapsed().as_secs_f32() < 2.0).map(|(id, t0)| (id.clone(), t0.elapsed().as_secs_f32()));
        let swatch = |p: &egui::Painter, r: Rect, l: &Light, i: usize| {
            let k = match motion {
                Motion::Still => 1.0,
                Motion::Breathe => 0.86 + 0.14 * (now * std::f32::consts::TAU / 8.0).cos(),
                Motion::Drift => 0.86 + 0.14 * (now * std::f32::consts::TAU / (5.0 + i as f32) + i as f32).cos(),
            };
            match states.iter().find(|(id, _, _)| id == &l.id).map(|(_, _, s)| s.clone()) {
                Some(s) if s.gradient.as_ref().is_some_and(|g| g.len() > 1) => {
                    let g: Vec<Rgb> = s.gradient.unwrap().iter().map(|c| c.scale(k)).collect();
                    paint_gradient(p, r, &g, 0.0);
                }
                Some(s) if s.color.is_some() => {
                    p.rect_filled(r, 0.0, c32(s.color.unwrap().scale(k)));
                }
                _ => hatch(p, r, SURFACE2),
            }
            if let Some((_, e)) = flashing.as_ref().filter(|(id, _)| id == &l.id) {
                if (e * 4.0) as i32 % 2 == 0 {
                    p.rect_filled(r, 0.0, Color32::from_white_alpha(170));
                }
            }
            lit_edge(p, r);
        };
        let mut flash: Option<&Light> = None;

        let w = ui.available_width();
        let framed = egui::Frame::none().fill(SURFACE).inner_margin(egui::Margin { left: 16.0, right: 16.0, top: 12.0, bottom: 16.0 }).show(ui, |ui| {
            ui.set_width(w - 32.0);
            ui.horizontal(|ui| {
                let n = lights.len();
                ui.label(RichText::new(format!("{} · {n} {}", t.name.to_uppercase(), if n == 1 { "LIGHT" } else { "LIGHTS" })).font(display(16.0)).color(TEXT).extra_letter_spacing(1.2));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    segmented_h(ui, &mut self.moods.view, &[(LightView::List, "List"), (LightView::Map, "Map")], 32.0, false);
                });
            });
            ui.add_space(8.0);
            if lights.is_empty() {
                ui.label(RichText::new("No lights here.").color(MUTED));
                return;
            }
            match self.moods.view {
                LightView::List => {
                    if let Some(l) = light_list(ui, inv, &lights, &swatch, flashing.as_ref().map(|(id, _)| id.as_str())) {
                        flash = Some(l);
                    }
                }
                LightView::Map => {
                    if let Some(l) = self.light_map(ui, inv, &lights, &swatch) {
                        flash = Some(l);
                    }
                }
            }
        });
        if std::mem::take(&mut self.moods.reveal) {
            framed.response.scroll_to_me(Some(egui::Align::BOTTOM));
        }
        if let Some(l) = flash {
            self.moods.flash = Some((l.id.clone(), Instant::now()));
            if !self.demo {
                let dev = l.owner.rid.clone();
                self.task("Identify", false, move |b| b.identify(&dev).map(|_| String::new()));
            }
        }
        if flashing.is_some() || flash.is_some() || (motion != Motion::Still && now > 0.0) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    /// The target's lights placed by their channel positions in the sync area; lights the area
    /// doesn't cover are listed under the map. Returns the light clicked.
    fn light_map<'a>(&self, ui: &mut egui::Ui, inv: &'a Inventory, lights: &[&'a Light], swatch: &dyn Fn(&egui::Painter, Rect, &Light, usize)) -> Option<&'a Light> {
        let ids: HashSet<&str> = lights.iter().map(|l| l.id.as_str()).collect();
        let covers = |e: &EntConfig| super::target::area_lights(inv, e).iter().filter(|l| ids.contains(l.id.as_str())).count();
        let area = self.sync_area(inv).or_else(|| inv.ent_configs.iter().filter(|e| covers(e) > 0).max_by_key(|e| covers(e)));
        // every channel position a light shows up in (gradient lights span several)
        let spots = |l: &Light| -> Vec<(f32, f32)> {
            area.map(|a| {
                a.channels
                    .iter()
                    .filter(|ch| inv.channel_lights(ch).iter().any(|(cl, _)| cl.id == l.id))
                    .map(|ch| (ch.position.x as f32, ch.position.y as f32))
                    .collect()
            })
            .unwrap_or_default()
        };
        let placed: Vec<(usize, &'a Light, Vec<(f32, f32)>)> = lights.iter().enumerate().map(|(i, l)| (i, *l, spots(l))).filter(|(_, _, s)| !s.is_empty()).collect();
        let loose: Vec<(usize, &'a Light)> = lights.iter().enumerate().filter(|(_, l)| !placed.iter().any(|(_, p, _)| p.id == l.id)).map(|(i, l)| (i, *l)).collect();
        let mut hit = None;

        if !placed.is_empty() {
            let iw = ui.available_width();
            let (map, _) = ui.allocate_exact_size(vec2(iw, 300.0), Sense::hover());
            let p = ui.painter().clone();
            p.rect_filled(map, 0.0, BG);
            p.rect_stroke(map.shrink(0.5), 0.0, Stroke::new(1.0_f32, RULE));
            let screen = Rect::from_center_size(pos2(map.center().x, map.top() + 14.0), vec2(map.width() * 0.2, 6.0));
            p.rect_filled(screen, 0.0, SURFACE3);
            text_at(&p, pos2(screen.right() + 10.0, screen.center().y), Align2::LEFT_CENTER, "SCREEN", display(12.0), FAINT, 1.0);
            text_at(&p, pos2(map.center().x, map.bottom() - 8.0), Align2::CENTER_BOTTOM, "YOU", display(12.0), FAINT, 1.0);
            let inner = map.shrink2(vec2(90.0, 50.0));
            let at = |x: f32, y: f32| pos2(inner.center().x + x.clamp(-1.0, 1.0) * inner.width() / 2.0, inner.center().y - y.clamp(-1.0, 1.0) * inner.height() / 2.0);
            let mut taken: Vec<Rect> = Vec::new();
            for (i, l, s) in &placed {
                let xs: Vec<f32> = s.iter().map(|(x, _)| at(*x, 0.0).x).collect();
                let cy = at(0.0, s.iter().map(|(_, y)| y).sum::<f32>() / s.len() as f32).y;
                let x0 = xs.iter().cloned().fold(f32::MAX, f32::min);
                let x1 = xs.iter().cloned().fold(f32::MIN, f32::max);
                let kind = format!("{} {}", l.metadata.archetype, model_of(inv, l)).to_lowercase();
                let arch = kind.as_str();
                let size = if l.gradient_points() >= 2 || arch.contains("strip") {
                    vec2((x1 - x0).max(96.0), 8.0)
                } else if arch.contains("play") {
                    vec2(12.0, 30.0)
                } else if arch.contains("signe") || arch.contains("floor") {
                    vec2(10.0, 40.0)
                } else {
                    vec2(18.0, 18.0)
                };
                let shape = Rect::from_center_size(pos2((x0 + x1) / 2.0, cy), size);
                taken.push(shape.expand(2.0));
                swatch(&p, shape, l, *i);
                // name under the light; if that spot is taken, try lower, then above
                let g = p.layout_no_wrap(l.metadata.name.trim().to_string(), body(12.5), TEXT);
                let ls = vec2(g.size().x + 8.0, g.size().y + 2.0);
                let lx = (shape.center().x - ls.x / 2.0).clamp(map.left() + 4.0, map.right() - 4.0 - ls.x);
                let tops = [
                    shape.bottom() + 4.0,
                    shape.bottom() + 4.0 + ls.y,
                    shape.top() - 4.0 - ls.y,
                    shape.bottom() + 4.0 + ls.y * 2.0,
                    shape.top() - 4.0 - ls.y * 2.0,
                ];
                let cands = tops.map(|y| Rect::from_min_size(pos2(lx, y), ls));
                let lr = cands.iter().copied().find(|c| map.contains_rect(*c) && !taken.iter().any(|t| t.intersects(*c))).unwrap_or(cands[0]);
                taken.push(lr);
                p.rect_filled(lr, 0.0, Color32::from_rgba_unmultiplied(13, 13, 15, 220));
                p.galley(lr.min + vec2(4.0, 1.0), g, TEXT);
                let resp = ui.interact(shape.union(lr).expand(4.0), ui.id().with(("map", &l.id)), Sense::click());
                if resp.hovered() {
                    p.rect_stroke(shape.expand(3.0), 0.0, Stroke::new(1.5_f32, ACCENT));
                }
                if resp.on_hover_text(format!("{}\nClick to blink it", model_of(inv, l))).clicked() {
                    hit = Some(*l);
                }
            }
        } else {
            ui.label(RichText::new("None of these lights are in a sync area yet. Set one up in the Hue app to see them here.").font(body(13.0)).color(MUTED));
        }
        if !loose.is_empty() {
            ui.add_space(12.0);
            ui.label(label_caps(if placed.is_empty() { "Lights" } else { "Not in the sync area" }, 13.0, MUTED));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                for (i, l) in loose {
                    let g = ui.painter().layout_no_wrap(l.metadata.name.trim().to_string(), body(13.0), TEXT);
                    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 44.0, 34.0), Sense::click());
                    let p = ui.painter();
                    p.rect_filled(r, 0.0, if resp.hovered() { SURFACE3 } else { SURFACE2 });
                    swatch(p, Rect::from_min_size(r.min + vec2(9.0, 9.0), vec2(16.0, 16.0)), l, i);
                    p.galley(pos2(r.left() + 34.0, r.center().y - g.size().y / 2.0), g, TEXT);
                    if resp.on_hover_text(format!("{}\nClick to blink it", model_of(inv, l))).clicked() {
                        hit = Some(l);
                    }
                }
            });
        }
        hit
    }

    fn mood_actions(&mut self, ui: &mut egui::Ui, inv: &Arc<Inventory>) {
        let colors = self.studio.colors();
        let t = self.target(inv);
        let target = Some(t.id.clone());
        let room_label = t.name.to_uppercase();
        let row = ui.max_rect();
        ui.allocate_rect(row, Sense::hover());
        ui.painter().rect_filled(row, 0.0, SURFACE);
        rule(ui.painter(), row.top(), row.left(), row.right());
        let save_r = Rect::from_min_size(row.min, vec2(row.width() / 3.0, 52.0));
        let apply_r = Rect::from_min_max(pos2(save_r.right(), row.top()), row.max);
        let saved = self.moods.saved_at.map(|t| t.elapsed().as_secs_f32() < 2.5).unwrap_or(false);
        let sresp = ui.interact(save_r, ui.id().with("save"), Sense::click());
        let h = ease_bool(ui, sresp.id.with("h"), sresp.hovered(), 0.15);
        ui.painter().rect_filled(save_r, 0.0, mix(SURFACE, TEXT, h));
        ui.painter().line_segment([save_r.right_top(), save_r.right_bottom()], Stroke::new(1.0_f32, RULE));
        text_at(ui.painter(), save_r.center(), Align2::CENTER_CENTER, if saved { "SAVED" } else { "SAVE" }, display(17.0), mix(TEXT, INK, h), 1.0);
        if saved {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        }
        if sresp.clicked() {
            self.save_mood();
        }
        let applied = self.moods.applied.as_ref().map(|(n, t)| n == &self.studio.name && t == &target).unwrap_or(false);
        let label = if applied { format!("On in {}", room_label.to_lowercase()) } else { format!("Apply to {}", room_label.to_lowercase()) };
        let btn = if applied { light(&label) } else { primary(&label) };
        if ui.put(apply_r, btn.height(52.0).min_size(apply_r.size())).clicked() && !applied {
            self.apply_mood(inv, &colors, target.clone());
        }
    }

    fn save_mood(&mut self) {
        let name = self.studio.name.trim().to_string();
        let name = if name.is_empty() { "My mood".to_string() } else { name };
        let colors: Vec<String> = self.studio.colors().iter().map(|c| c.to_hex()).collect();
        match self.cfg.palettes.iter_mut().find(|p| p.name.eq_ignore_ascii_case(&name)) {
            Some(p) => p.colors = colors,
            None => self.cfg.palettes.insert(0, SavedPalette { name: name.clone(), colors }),
        }
        self.save_cfg();
        self.moods.saved_at = Some(Instant::now());
        self.moods.sel = Some(name.clone());
        self.moods.loaded = Some(name.clone());
        self.moods.src = Some(Src::Yours);
        self.studio.history.clear();
        self.bg.lock().unwrap().note(&name, "Saved to your moods", TEXT);
    }

    fn apply_mood(&mut self, inv: &Arc<Inventory>, colors: &[Rgb], target: Option<String>) {
        let name = if self.studio.name.trim().is_empty() { "Mood".to_string() } else { self.studio.name.trim().to_string() };
        match self.moods.motion.unwrap_or(Motion::Still) {
            Motion::Still => {
                self.apply_palette(inv, colors, self.studio.brightness as f64, &name);
            }
            m => {
                // moving moods are streamed: hand the colours to Live as a palette look
                self.sy.palette = colors.iter().map(|c| c.to_u8()).collect();
                self.sy.look = LookKind::Palette;
                self.sy.motion = if m == Motion::Breathe { MotionKind::Breathe } else { MotionKind::Drift };
                self.sy.source = crate::sync::Source::Look;
                self.go_live(inv);
            }
        }
        let ids: Vec<String> = match self.target(inv).group {
            Some(g) => vec![g.id.clone()],
            None => inv.rooms.iter().map(|g| g.id.clone()).collect(),
        };
        for id in ids {
            self.home.room_mood.insert(id, name.clone());
        }
        self.moods.applied = Some((self.studio.name.clone(), target));
    }
}

fn block_choice(ui: &mut egui::Ui, label: &str, sel: bool) -> egui::Response {
    let g = ui.painter().layout_job(spaced(label, display(16.0), Color32::PLACEHOLDER, 1.0));
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 28.0, 44.0), Sense::click());
    let h = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
    let p = ui.painter();
    p.rect_filled(r, 0.0, if sel { TEXT } else { mix(SURFACE2, SURFACE3, h) });
    p.galley(r.center() - g.size() / 2.0, g, if sel { INK } else { SOFT });
    resp
}

fn model_of(inv: &Inventory, l: &Light) -> String {
    inv.device(&l.owner.rid).map(|d| d.product_data.product_name.trim().to_string()).unwrap_or_default()
}

/// Returns the light whose FLASH was pressed.
fn light_list<'a>(ui: &mut egui::Ui, inv: &Inventory, lights: &[&'a Light], swatch: &dyn Fn(&egui::Painter, Rect, &Light, usize), flashing: Option<&str>) -> Option<&'a Light> {
    let iw = ui.available_width();
    let cols = if iw >= 760.0 { 2 } else { 1 };
    let gap = 24.0;
    let cw = (iw - gap * (cols as f32 - 1.0)) / cols as f32;
    let (sw, bw) = (96.0, 76.0);
    let text_w = (cw - 10.0 - sw - 14.0 - 14.0 - bw).max(60.0);
    let mut hit = None;
    for (ri, chunk) in lights.chunks(cols).enumerate() {
        let p = ui.painter().clone();
        let laid: Vec<_> = chunk
            .iter()
            .map(|l| (p.layout(l.metadata.name.trim().to_string(), semibold(15.0), TEXT, text_w), p.layout(model_of(inv, l), body(12.5), MUTED, text_w)))
            .collect();
        let h = laid.iter().map(|(a, b)| a.size().y + b.size().y + 18.0).fold(56.0, f32::max);
        let (row, _) = ui.allocate_exact_size(vec2(iw, h), Sense::hover());
        for (ci, (l, (name, sub))) in chunk.iter().zip(laid).enumerate() {
            let cell = Rect::from_min_size(pos2(row.left() + ci as f32 * (cw + gap), row.top()), vec2(cw, h));
            if ui.rect_contains_pointer(cell) {
                p.rect_filled(cell, 0.0, SURFACE2);
                p.rect_filled(Rect::from_min_size(pos2(cell.left(), cell.top() + 8.0), vec2(3.0, h - 16.0)), 0.0, ACCENT);
            }
            row_rule(&p, cell.top(), cell.left(), cell.right());
            let sr = Rect::from_min_size(pos2(cell.left() + 10.0, cell.center().y - 15.0), vec2(sw, 30.0));
            swatch(&p, sr, l, ri * cols + ci);
            let tx = sr.right() + 14.0;
            let (nh, th) = (name.size().y, name.size().y + 2.0 + sub.size().y);
            let ty = cell.center().y - th / 2.0;
            p.galley(pos2(tx, ty), name, TEXT);
            p.galley(pos2(tx, ty + nh + 2.0), sub, MUTED);
            let br = Rect::from_min_size(pos2(cell.right() - bw, cell.center().y - 20.0), vec2(bw, 40.0));
            let b = if flashing == Some(l.id.as_str()) { primary("Flash") } else { ghost("Flash") };
            if ui.put(br, b.small().height(40.0).min_size(br.size())).on_hover_text("Blink this light so you can find it").clicked() {
                hit = Some(*l);
            }
        }
    }
    hit
}
