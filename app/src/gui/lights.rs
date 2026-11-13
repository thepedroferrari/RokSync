//! The Lights page: lights by room with filters, and an editor panel for the selected light,
//! room, zone or entertainment area.

use super::home::apply_local;
use super::icons::Icon;
use super::theme::*;
use super::{LumenApp, Pending};
use eframe::egui::{self, pos2, vec2, Align2, Color32, Rect, RichText, Sense, Stroke, UiBuilder, Vec2};
use lumen_engine::sample_linear;
use lumen_hue::{Inventory, Light, LightState, Rgb};
use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, Instant};

pub const PRESETS: [[u8; 3]; 14] = [
    [255, 197, 143],
    [255, 244, 229],
    [255, 0, 0],
    [255, 80, 0],
    [255, 160, 0],
    [255, 230, 0],
    [120, 255, 0],
    [0, 255, 60],
    [0, 255, 200],
    [0, 170, 255],
    [0, 40, 255],
    [130, 0, 255],
    [255, 0, 200],
    [255, 60, 120],
];

#[derive(PartialEq, Clone, Debug, Default)]
pub enum Sel {
    #[default]
    None,
    Light(String),
    Room(String),
    Zone(String),
    Area(String),
    NewZone,
}


#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Filter {
    #[default]
    All,
    On,
    Gradient,
    Unreachable,
}

pub struct LightsUi {
    pub sel: Sel,
    loaded: Sel,
    name: String,
    bri: f32,
    color: Color32,
    grad: Vec<[u8; 3]>,
    per_light: BTreeMap<String, [u8; 3]>,
    pub stops: Vec<[u8; 3]>,
    members: HashSet<String>,
    confirm_delete: bool,
    pub filter: Filter,
    seg: usize,
    /// Mode picked in the panel before the bridge confirms it: `id` = white, `!id` = colour.
    white: HashSet<String>,
    /// Last light shown, so closing a room/zone editor goes back to it.
    last_light: Option<String>,
    identify_at: Option<Instant>,
    renaming: bool,
    pub show_map: bool,
}

impl Default for LightsUi {
    fn default() -> Self {
        Self {
            sel: Sel::None,
            loaded: Sel::None,
            name: String::new(),
            bri: 100.0,
            color: Color32::WHITE,
            grad: Vec::new(),
            per_light: BTreeMap::new(),
            stops: vec![[255, 0, 160], [0, 200, 255]],
            members: HashSet::new(),
            confirm_delete: false,
            filter: Filter::All,
            seg: 0,
            white: HashSet::new(),
            last_light: None,
            identify_at: None,
            renaming: false,
            show_map: false,
        }
    }
}

fn rgb(a: [u8; 3]) -> Rgb {
    Rgb::from_u8(a)
}

fn to32(c: Rgb) -> Color32 {
    c32(c)
}

fn from32(c: Color32) -> Rgb {
    Rgb::from_u8([c.r(), c.g(), c.b()])
}

pub fn picker(ui: &mut egui::Ui, c: &mut [u8; 3]) -> bool {
    egui::color_picker::color_edit_button_srgb(ui, c).changed()
}

pub fn preset_row(ui: &mut egui::Ui) -> Option<[u8; 3]> {
    let mut out = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(6.0);
        for p in PRESETS {
            if swatch(ui, rgb(p), 22.0).on_hover_text(rgb(p).to_hex()).clicked() {
                out = Some(p);
            }
        }
    });
    out
}

/// Keeps between 2 and `max` stops.
pub fn stops_editor(ui: &mut egui::Ui, stops: &mut Vec<[u8; 3]>, max: usize) -> bool {
    let mut changed = false;
    let cols: Vec<Rgb> = stops.iter().map(|c| rgb(*c)).collect();
    palette_bar(ui, &cols, 14.0);
    ui.horizontal_wrapped(|ui| {
        for s in stops.iter_mut() {
            changed |= picker(ui, s);
        }
        if stops.len() < max && ui.add(icon_btn(Icon::Plus)).on_hover_text("Add colour").clicked() {
            let last = *stops.last().unwrap_or(&[255, 255, 255]);
            stops.push(last);
            changed = true;
        }
        if stops.len() > 2 && ui.add(icon_btn(Icon::Minus)).on_hover_text("Remove colour").clicked() {
            stops.pop();
            changed = true;
        }
        if ui.add(icon_btn(Icon::Swap)).on_hover_text("Reverse").clicked() {
            stops.reverse();
            changed = true;
        }
    });
    changed
}

pub fn spread_across(lights: &[&Light], stops: &[Rgb]) -> Vec<(String, LightState)> {
    let n = lights.len();
    lights
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let u0 = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.5 };
            let k = l.gradient_points() as usize;
            if k >= 2 {
                let span = if n > 1 { 0.9 / (n - 1) as f32 } else { 1.0 };
                let pts = (0..k)
                    .map(|j| sample_linear(stops, (u0 - span / 2.0 + span * j as f32 / (k - 1) as f32).clamp(0.0, 1.0)))
                    .collect();
                (l.id.clone(), LightState::gradient(pts))
            } else {
                (l.id.clone(), LightState::color(sample_linear(stops, u0)))
            }
        })
        .collect()
}

const POOL: [(&str, &str); 16] = [
    ("Red", "#FF2A00"),
    ("Flame", "#FF5A1F"),
    ("Amber", "#FF8A00"),
    ("Gold", "#FFD000"),
    ("Lime", "#A6FF3D"),
    ("Green", "#00E676"),
    ("Mint", "#00FFA3"),
    ("Cyan", "#00E5FF"),
    ("Sky", "#7FD3FF"),
    ("Blue", "#2D5BFF"),
    ("Indigo", "#3A00FF"),
    ("Violet", "#7B2FF7"),
    ("Purple", "#B000FF"),
    ("Pink", "#FF2BD6"),
    ("Rose", "#FF2E63"),
    ("Warm white", "#FFD29A"),
];

const KELVINS: [u32; 9] = [2200, 2700, 3000, 3500, 4000, 4500, 5000, 5700, 6500];

fn kind_label(l: &Light) -> String {
    let k = l.gradient_points();
    if k >= 2 {
        format!("GRADIENT · {k}")
    } else if l.supports_color() {
        "COLOUR".into()
    } else {
        "WHITE ONLY".into()
    }
}

fn shown_colors(l: &Light) -> Vec<Rgb> {
    let g = l.current_gradient();
    if g.len() > 1 {
        return g;
    }
    let mut c = l.current_rgb();
    if l.on.on {
        // normalise to full brightness; brightness is shown separately
        c = c.scale(1.0 / c.max_channel().max(0.05));
    }
    vec![c]
}

fn paint_swatch(p: &egui::Painter, r: Rect, l: &Light, lit: bool) {
    if !lit {
        hatch(p, r, SURFACE2);
        return;
    }
    let cols = shown_colors(l);
    if cols.len() > 1 {
        paint_gradient(p, r, &cols, 0.0);
    } else {
        p.rect_filled(r, 0.0, c32(cols[0]));
    }
    lit_edge(p, r);
}

fn kelvin_color(k: u32) -> Color32 {
    let c = Rgb::from_mirek(1_000_000.0 / k as f64);
    c32(c.scale(1.0 / c.max_channel().max(0.05)))
}

impl LumenApp {
    pub fn lights_page(&mut self, ctx: &egui::Context, inv: &Inventory, frame: egui::Frame, ease: f32) {
        self.load_editor(inv);
        if self.lights.sel == Sel::None {
            let id = self.lights.last_light.clone().filter(|id| inv.light(id).is_some()).or_else(|| ordered_rooms(inv).iter().flat_map(|(_, ls)| ls.first()).next().map(|l| l.id.clone()));
            if let Some(id) = id {
                self.lights.sel = Sel::Light(id);
            }
        }
        if let Sel::Light(id) = &self.lights.sel {
            if self.lights.last_light.as_deref() != Some(id) {
                self.lights.seg = 0;
                self.lights.renaming = false;
            }
            self.lights.last_light = Some(id.clone());
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !matches!(self.lights.sel, Sel::Light(_)) {
            self.lights.sel = Sel::None;
        }
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            ui.set_opacity(ease);
            self.lights_header(ui, inv);
            ui.add_space(18.0);
            let full = ui.available_rect_before_wrap();
            let gap = 36.0;
            let right_w = (full.width() * 0.42).clamp(420.0, 600.0);
            let left = Rect::from_min_max(full.min, pos2(full.right() - right_w - gap, full.bottom()));
            let right = Rect::from_min_max(pos2(full.right() - right_w, full.top()), full.max);
            ui.allocate_new_ui(UiBuilder::new().max_rect(left), |ui| {
                egui::ScrollArea::vertical().id_salt("lights-list").auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(2.0);
                    self.lights_list(ui, inv);
                    ui.add_space(24.0);
                });
            });
            ui.allocate_new_ui(UiBuilder::new().max_rect(right), |ui| {
                egui::ScrollArea::vertical().id_salt("lights-panel").auto_shrink([false, false]).show(ui, |ui| {
                    match self.lights.sel.clone() {
                        Sel::Light(id) => self.light_panel(ui, inv, &id),
                        _ => {
                            egui::Frame::none().fill(SURFACE).inner_margin(egui::Margin::same(24.0)).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                self.editor(ui, inv);
                            });
                        }
                    }
                    ui.add_space(14.0);
                    for e in &inv.ent_configs {
                        self.area_map(ui, inv, e);
                        ui.add_space(14.0);
                    }
                    ui.add_space(20.0);
                });
            });
        });
    }

    fn lights_header(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        let areas = inv.ent_configs.len();
        let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let sub = format!(
            "{} · {} · {} · {}",
            plural(inv.lights.len(), "light", "lights"),
            plural(inv.rooms.len(), "room", "rooms"),
            plural(inv.zones.len(), "zone", "zones"),
            plural(areas, "entertainment area", "entertainment areas")
        );
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(headline("Lights", 64.0));
                ui.label(RichText::new(sub).font(body(14.0)).color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if ui.add(ghost("Refresh").icon(Icon::Refresh).height(HIT)).clicked() {
                    self.refresh();
                }
                ui.add_space(16.0);
                let unreachable = inv.lights.iter().filter(|l| !inv.reachable(l)).count();
                let opts = [(Filter::Unreachable, "Unreachable"), (Filter::Gradient, "Gradient"), (Filter::On, "On"), (Filter::All, "All")];
                for (f, label) in opts {
                    if f == Filter::Unreachable && unreachable == 0 && self.lights.filter != f {
                        continue;
                    }
                    let sel = self.lights.filter == f;
                    let text = if f == Filter::Unreachable { format!("{label} · {unreachable}") } else { label.to_string() };
                    if ui.add(HudButton::new(&text, if sel { Kind::Light } else { Kind::Ghost }).height(HIT).selected(sel)).clicked() {
                        self.lights.filter = f;
                    }
                }
            });
        });
    }

    fn lights_list(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        let filter = self.lights.filter;
        let keep = |l: &Light| match filter {
            Filter::All => true,
            Filter::On => l.on.on && inv.reachable(l),
            Filter::Gradient => l.gradient_points() >= 2,
            Filter::Unreachable => !inv.reachable(l),
        };
        let mut shown_any = false;
        for (room, lights) in ordered_rooms(inv) {
            let shown: Vec<&Light> = lights.iter().copied().filter(|l| keep(l)).collect();
            if shown.is_empty() {
                continue;
            }
            shown_any = true;
            let lit = lights.iter().filter(|l| l.on.on && inv.reachable(l)).count();
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::hover());
            let p = ui.painter();
            let name = room.map(|r| r.metadata.name.trim().to_uppercase()).unwrap_or_else(|| "NOT IN A ROOM".into());
            let g = text_at(p, pos2(row.left(), row.bottom() - 14.0), Align2::LEFT_BOTTOM, &name, display_bold(22.0), TEXT, 1.0);
            let sub = format!("{} {} · {lit} on", lights.len(), if lights.len() == 1 { "light" } else { "lights" });
            text_at(p, pos2(g.right() + 14.0, row.bottom() - 16.0), Align2::LEFT_BOTTOM, &sub, body(13.0), MUTED, 0.0);
            if let Some(r) = room {
                let br = Rect::from_min_size(pos2(row.right() - 90.0, row.bottom() - 46.0), vec2(90.0, 36.0));
                if ui.put(br, quiet("Edit room").height(36.0)).clicked() {
                    self.lights.sel = Sel::Room(r.id.clone());
                }
            }
            let (rr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            rule(ui.painter(), rr.top(), rr.left(), rr.right());
            for l in shown {
                self.light_row(ui, inv, l);
            }
            ui.add_space(18.0);
        }
        if !shown_any {
            ui.add_space(20.0);
            ui.label(RichText::new("No lights match this filter.").color(MUTED));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("ZONES").font(display_bold(22.0)).color(TEXT).extra_letter_spacing(1.0));
            ui.add_space(8.0);
            if ui.add(ghost("New zone").icon(Icon::Plus).height(36.0)).clicked() {
                self.lights.sel = Sel::NewZone;
            }
        });
        ui.add_space(6.0);
        if inv.zones.is_empty() {
            ui.label(RichText::new("No zones yet. A zone groups lights across rooms, like everything around your desk.").color(MUTED));
        }
        for z in &inv.zones {
            let lights = inv.group_lights(z);
            let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::click());
            let cur = self.lights.sel == Sel::Zone(z.id.clone());
            let p = ui.painter();
            p.rect_filled(row, 0.0, if cur || resp.hovered() { SURFACE } else { SURFACE.gamma_multiply(0.6) });
            row_cursor(ui, resp.id, row, resp.hovered() || cur);
            let p = ui.painter();
            text_at(p, pos2(row.left() + 20.0, row.center().y - 11.0), Align2::LEFT_CENTER, &format!("Zone · {}", z.metadata.name.trim()), semibold(16.0), TEXT, 0.0);
            let names: Vec<&str> = lights.iter().map(|l| l.metadata.name.trim()).collect();
            text_at(p, pos2(row.left() + 20.0, row.center().y + 12.0), Align2::LEFT_CENTER, &truncate(&names.join(", "), 70), body(13.0), MUTED, 0.0);
            text_at(p, pos2(row.right() - 20.0, row.center().y), Align2::RIGHT_CENTER, &format!("{} LIGHTS", lights.len()), display(16.0), SOFT, 1.0);
            if resp.clicked() {
                self.lights.sel = Sel::Zone(z.id.clone());
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            ui.add_space(8.0);
        }
    }

    fn light_row(&mut self, ui: &mut egui::Ui, inv: &Inventory, l: &Light) {
        let reach = inv.reachable(l);
        let lit = l.on.on && reach;
        let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 68.0), Sense::click());
        let cur = self.lights.sel == Sel::Light(l.id.clone());
        let hov = resp.hovered();
        if cur || hov {
            ui.painter().rect_filled(row, 0.0, if cur { SURFACE } else { hex("#17171B") });
        }
        row_cursor(ui, resp.id, row, hov || cur);
        let p = ui.painter();
        row_rule(p, row.bottom(), row.left(), row.right());
        let sw = Rect::from_min_size(pos2(row.left() + 12.0, row.center().y - 20.0), vec2(80.0, 40.0));
        paint_swatch(p, sw, l, lit);
        let x = sw.right() + 18.0;
        let g = text_at(p, pos2(x, row.center().y - 10.0), Align2::LEFT_CENTER, l.metadata.name.trim(), semibold(16.0), if lit { TEXT } else { MUTED }, 0.0);
        if !reach {
            let tg = p.layout_job(spaced("UNREACHABLE", display(12.0), Color32::PLACEHOLDER, 1.0));
            let tr = Rect::from_min_size(pos2(g.right() + 10.0, row.center().y - 19.0), vec2(tg.size().x + 12.0, 18.0));
            p.rect_filled(tr, 0.0, WARN);
            p.galley(tr.center() - tg.size() / 2.0, tg, INK);
        }
        let model = inv.device(&l.owner.rid).map(|d| d.product_data.product_name.clone()).unwrap_or_default();
        text_at(p, pos2(x, row.center().y + 12.0), Align2::LEFT_CENTER, &model, body(13.0), MUTED, 0.0);
        let sw_r = Rect::from_min_size(pos2(row.right() - 76.0, row.center().y - HIT / 2.0), vec2(64.0, HIT));
        let num_x = sw_r.left() - 22.0;
        if !reach {
            text_at(p, pos2(num_x, row.center().y), Align2::RIGHT_CENTER, "—", display_bold(24.0), MUTED, 0.0);
        } else if lit {
            num_unit(p, pos2(num_x, row.center().y), Align2::RIGHT_CENTER, &format!("{:.0}", l.brightness()), "%", 24.0, TEXT);
        } else {
            text_at(p, pos2(num_x, row.center().y), Align2::RIGHT_CENTER, "OFF", display_bold(20.0), MUTED, 1.0);
        }
        text_at(p, pos2(num_x - 92.0, row.center().y), Align2::RIGHT_CENTER, &kind_label(l), display(13.0), FAINT, 1.0);
        let r = ui.add_enabled_ui(reach, |ui| onoff_at(ui, sw_r, resp.id.with("pow"), l.on.on)).inner;
        if r.clicked() && reach {
            self.send_light(&l.id, LightState::power(!l.on.on));
        } else if resp.clicked() {
            self.lights.sel = Sel::Light(l.id.clone());
        }
        if hov {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }

    /// Change a light now on screen, then on the bridge (throttled).
    fn send_light(&mut self, id: &str, st: LightState) {
        let (id2, st2) = (id.to_string(), st.clone());
        self.local_edit(move |inv| apply_local(inv, &id2, &st2));
        self.pending = Some(Pending::Light(id.to_string(), st));
    }

    fn light_panel(&mut self, ui: &mut egui::Ui, inv: &Inventory, id: &str) {
        let Some(l) = inv.light(id).cloned() else {
            ui.label(RichText::new("This light is gone. Refresh?").color(MUTED));
            return;
        };
        let reach = inv.reachable(&l);
        let lit = l.on.on && reach;
        let white_only = !l.supports_color();
        let white = white_only || self.lights.white.contains(&l.id) || (l.kelvin().is_some() && !self.lights.white.contains(&format!("!{}", l.id)));
        let k = l.gradient_points() as usize;
        let is_grad = k >= 2 && !white;
        egui::Frame::none().fill(SURFACE).inner_margin(egui::Margin::same(24.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (big, _) = ui.allocate_exact_size(vec2(ui.available_width(), 132.0), Sense::hover());
            let cols = if lit { shown_colors(&l) } else { Vec::new() };
            if !lit {
                hatch(ui.painter(), big, SURFACE2);
            } else if is_grad && cols.len() > 1 {
                let n = cols.len();
                let w = big.width() / n as f32;
                for (i, c) in cols.iter().enumerate() {
                    let r = Rect::from_min_size(pos2(big.left() + w * i as f32, big.top()), vec2(w, big.height()));
                    let resp = ui.interact(r, ui.id().with(("seg", i)), Sense::click());
                    ui.painter().rect_filled(r, 0.0, c32(*c));
                    let selected = self.lights.seg.min(n - 1) == i;
                    if selected {
                        ui.painter().rect_stroke(r.shrink(1.5), 0.0, Stroke::new(3.0_f32, TEXT));
                    }
                    let tag = Rect::from_min_size(r.left_bottom() + vec2(8.0, -30.0), vec2(22.0, 22.0));
                    ui.painter().rect_filled(tag, 0.0, if selected { TEXT } else { INK.gamma_multiply(0.7) });
                    text_at(ui.painter(), tag.center(), Align2::CENTER_CENTER, &(i + 1).to_string(), display_bold(14.0), if selected { INK } else { TEXT }, 0.0);
                    if resp.clicked() {
                        self.lights.seg = i;
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
            } else {
                ui.painter().rect_filled(big, 0.0, c32(cols[0]));
            }
            if lit {
                lit_edge(ui.painter(), big);
            }
            ui.add_space(16.0);
            let model = inv.device(&l.owner.rid).map(|d| d.product_data.product_name.clone()).unwrap_or_default();
            let room = inv.room_of_light(&l).map(|r| r.metadata.name.trim().to_string()).unwrap_or_else(|| "No room".into());
            let identifying = self.lights.identify_at.map(|t| t.elapsed() < Duration::from_secs(2)).unwrap_or(false);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    if self.lights.renaming {
                        self.rename_row(ui, "light", &l.id);
                    } else {
                        ui.label(RichText::new(l.metadata.name.trim().to_uppercase()).font(display_bold(30.0)).color(TEXT).extra_letter_spacing(0.5));
                    }
                    ui.label(RichText::new(format!("{model} · {room}")).font(body(13.5)).color(MUTED));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui.add_enabled(reach, ghost(if identifying { "Blinking…" } else { "Identify" }).height(HIT)).clicked() {
                        self.lights.identify_at = Some(Instant::now());
                        let dev = l.owner.rid.clone();
                        self.task("Identify", false, move |b| b.identify(&dev).map(|_| String::new()));
                    }
                    if ui.add(quiet(if self.lights.renaming { "Done" } else { "Rename" }).height(HIT)).clicked() {
                        self.lights.renaming = !self.lights.renaming;
                        self.lights.name = l.metadata.name.trim().to_string();
                    }
                });
            });
            if identifying {
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            }
            ui.add_space(6.0);
            ui.label(RichText::new(used_in(inv, &l)).font(body(13.0)).color(SOFT));
            ui.add_space(14.0);
            if !reach {
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 86.0), Sense::hover());
                ui.painter().rect_filled(r, 0.0, SURFACE2);
                ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(4.0, r.height())), 0.0, WARN);
                text_at(ui.painter(), pos2(r.left() + 20.0, r.top() + 24.0), Align2::LEFT_CENTER, "CAN'T REACH THIS LIGHT", display_bold(18.0), TEXT, 1.0);
                let g = ui.painter().layout(
                    "Usually it's off at the wall switch. Switch it on and give it 10 seconds.".into(),
                    body(13.5),
                    SOFT,
                    r.width() - 40.0,
                );
                ui.painter().galley(pos2(r.left() + 20.0, r.top() + 40.0), g, SOFT);
                return;
            }
            let bri = if lit { l.brightness() } else { 0.0 };
            ui.horizontal(|ui| {
                ui.label(label_caps("Brightness", 15.0, MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(90.0, 26.0), Sense::hover());
                    if lit {
                        num_unit(ui.painter(), r.right_center(), Align2::RIGHT_CENTER, &format!("{bri:.0}"), "%", 24.0, TEXT);
                    } else {
                        text_at(ui.painter(), r.right_center(), Align2::RIGHT_CENTER, "OFF", display_bold(20.0), MUTED, 1.0);
                    }
                });
            });
            ui.add_space(6.0);
            let (steps, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
            let gap = 6.0;
            let sw = (steps.width() - gap * 9.0) / 10.0;
            for i in 0..10 {
                let v = (i + 1) as f64 * 10.0;
                let h = 18.0 + i as f32 * 2.6;
                let col = Rect::from_min_size(pos2(steps.left() + i as f32 * (sw + gap), steps.top()), vec2(sw, steps.height()));
                let bar = Rect::from_min_max(pos2(col.left(), col.bottom() - h), col.max);
                let resp = ui.interact(col, ui.id().with(("bri", i)), Sense::click());
                let on = lit && v <= bri + 0.5;
                let c = if on { TEXT } else if resp.hovered() { EDGE } else { SURFACE3 };
                ui.painter().rect_filled(bar, 0.0, c);
                if resp.clicked() {
                    self.send_light(&l.id, LightState { on: Some(true), brightness: Some(v), ..Default::default() });
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
            ui.add_space(18.0);
            let mut mode_white = white;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_enabled_ui(!white_only, |ui| {
                    if ui.add(HudButton::new("Colour", if !mode_white { Kind::Light } else { Kind::Ghost }).height(HIT).selected(!mode_white)).clicked() {
                        mode_white = false;
                    }
                });
                if ui.add(HudButton::new("White", if mode_white { Kind::Light } else { Kind::Ghost }).height(HIT).selected(mode_white)).clicked() {
                    mode_white = true;
                }
            });
            if mode_white != white {
                if mode_white {
                    self.lights.white.insert(l.id.clone());
                    self.lights.white.remove(&format!("!{}", l.id));
                } else {
                    self.lights.white.remove(&l.id);
                    self.lights.white.insert(format!("!{}", l.id));
                }
            }
            ui.add_space(14.0);
            let n = l.current_gradient().len().max(k);
            let seg = self.lights.seg.min(n.saturating_sub(1));
            let title = if mode_white {
                "WHITE".to_string()
            } else if is_grad {
                format!("COLOUR · SEGMENT {} OF {n}", seg + 1)
            } else {
                "COLOUR".to_string()
            };
            ui.label(label_caps(&title, 15.0, MUTED));
            ui.add_space(8.0);
            if mode_white {
                let cur_k = l.kelvin();
                let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
                let w = strip.width() / KELVINS.len() as f32;
                for (i, kv) in KELVINS.iter().enumerate() {
                    let r = Rect::from_min_size(pos2(strip.left() + w * i as f32, strip.top()), vec2(w, strip.height()));
                    let resp = ui.interact(r, ui.id().with(("k", i)), Sense::click());
                    ui.painter().rect_filled(r, 0.0, kelvin_color(*kv));
                    let selected = cur_k.map(|c| (c as i64 - *kv as i64).abs() < 120).unwrap_or(false);
                    if selected {
                        ui.painter().rect_stroke(r.shrink(3.0), 0.0, Stroke::new(2.0_f32, INK));
                        ui.painter().rect_stroke(r.shrink(5.0), 0.0, Stroke::new(2.0_f32, TEXT));
                    }
                    if resp.clicked() {
                        let mirek = (1_000_000.0 / *kv as f64).round() as u16;
                        self.send_light(&l.id, LightState { on: Some(true), mirek: Some(mirek), ..Default::default() });
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(label_caps("2200 K · candle", 13.0, MUTED));
                    let now = cur_k.map(|k| format!("{} K", (k + 50) / 100 * 100)).unwrap_or_else(|| "—".into());
                    let w = ui.available_width();
                    ui.add_sized(vec2(w - 150.0, 18.0), egui::Label::new(label_caps(&now, 15.0, TEXT)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(label_caps("6500 K · daylight", 13.0, MUTED)));
                });
                if white_only {
                    ui.add_space(6.0);
                    ui.label(RichText::new("This light does white only.").font(body(13.0)).color(MUTED));
                }
            } else {
                let cur_cols = l.current_gradient();
                let cur = if is_grad { cur_cols.get(seg).copied() } else { l.color.as_ref().map(|c| Rgb::from_xy(c.xy.x, c.xy.y)) };
                let per_row = 8;
                let gap = 6.0;
                let sw = (ui.available_width() - gap * (per_row as f32 - 1.0)) / per_row as f32;
                for row in POOL.chunks(per_row) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for (name, hx) in row {
                            let c = Rgb::from_hex(hx).unwrap_or(Rgb::WHITE);
                            let (r, resp) = ui.allocate_exact_size(vec2(sw, 40.0), Sense::click());
                            let grow = ui.ctx().animate_bool_with_time(resp.id.with("g"), resp.hovered(), 0.15);
                            let draw = r.expand(grow * 2.0);
                            ui.painter().rect_filled(draw, 0.0, c32(c));
                            ui.painter().rect_stroke(draw, 0.0, Stroke::new(1.0_f32, Color32::from_white_alpha(30)));
                            let same = cur.map(|k| (k.r - c.r).abs() + (k.g - c.g).abs() + (k.b - c.b).abs() < 0.12).unwrap_or(false);
                            if same {
                                ui.painter().rect_stroke(draw.expand(2.0), 0.0, Stroke::new(2.0_f32, TEXT));
                            }
                            let resp = resp.on_hover_text(*name);
                            if resp.clicked() {
                                let st = if is_grad {
                                    let mut pts = cur_cols.clone();
                                    pts.resize(n, c);
                                    pts[seg] = c;
                                    LightState::gradient(pts)
                                } else {
                                    LightState::color(c)
                                };
                                self.send_light(&l.id, LightState { on: Some(true), brightness: Some(l.brightness().max(1.0)), ..st });
                            }
                        }
                    });
                    ui.add_space(gap);
                }
                if is_grad {
                    ui.label(RichText::new("Click a segment above, then a colour.").font(body(13.0)).color(MUTED));
                }
            }
        });
    }

    /// Channels drawn at their Hue positions (x, y in -1..1), seen from above with the screen at the top.
    fn area_map(&mut self, ui: &mut egui::Ui, inv: &Inventory, e: &lumen_hue::EntConfig) {
        egui::Frame::none().fill(SURFACE).inner_margin(egui::Margin::same(24.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} AREA", e.metadata.name.trim().to_uppercase())).font(display_bold(20.0)).color(TEXT).extra_letter_spacing(1.0));
                ui.label(RichText::new(format!("{} channels · seen from above", e.channels.len())).font(body(13.0)).color(MUTED));
            });
            ui.add_space(10.0);
            let (map, _) = ui.allocate_exact_size(vec2(ui.available_width(), 260.0), Sense::hover());
            if self.lights.show_map {
                ui.scroll_to_rect(map.expand(80.0), Some(egui::Align::Max));
                self.lights.show_map = false;
            }
            let p = ui.painter();
            p.rect_filled(map, 0.0, SURFACE2);
            let scr = Rect::from_center_size(pos2(map.center().x, map.top() + 30.0), vec2(map.width() * 0.36, 8.0));
            p.rect_filled(scr, 0.0, SOFT);
            text_at(p, pos2(scr.center().x, scr.top() - 12.0), Align2::CENTER_CENTER, "SCREEN", display(12.0), MUTED, 1.5);
            let you = pos2(map.center().x, map.bottom() - 26.0);
            p.rect_filled(Rect::from_center_size(you, vec2(14.0, 14.0)), 0.0, EDGE);
            text_at(p, you + vec2(0.0, 16.0), Align2::CENTER_CENTER, "YOU", display(12.0), MUTED, 1.5);
            let inner = Rect::from_min_max(map.min + vec2(30.0, 56.0), map.max - vec2(30.0, 56.0));
            let sel_light = match &self.lights.sel {
                Sel::Light(id) => Some(id.clone()),
                _ => None,
            };
            for ch in &e.channels {
                let x = inner.left() + ((ch.position.x as f32 + 1.0) / 2.0).clamp(0.0, 1.0) * inner.width();
                let y = inner.top() + (1.0 - (ch.position.y as f32 + 1.0) / 2.0).clamp(0.0, 1.0) * inner.height();
                let r = Rect::from_center_size(pos2(x, y), vec2(26.0, 26.0));
                let members = inv.channel_lights(ch);
                let (col, name, lid) = match members.first() {
                    Some((l, idx)) => {
                        let g = l.current_gradient();
                        let c = if !l.on.on {
                            EDGE
                        } else if g.len() > 1 {
                            c32(g[(*idx as usize).min(g.len() - 1)])
                        } else {
                            c32(l.current_rgb().scale(1.0 / l.current_rgb().max_channel().max(0.05)))
                        };
                        (c, l.metadata.name.trim().to_string(), Some(l.id.clone()))
                    }
                    None => (EDGE, "Empty channel".into(), None),
                };
                let resp = ui.interact(r, ui.id().with(("ch", ch.channel_id)), Sense::click()).on_hover_text(&name);
                let p = ui.painter();
                if lid.is_some() && lid == sel_light {
                    p.rect_stroke(r.expand(2.0), 0.0, Stroke::new(2.0_f32, INK));
                    p.rect_stroke(r.expand(4.0), 0.0, Stroke::new(2.0_f32, ACCENT));
                }
                p.rect_filled(r, 0.0, col);
                text_at(p, pos2(r.center().x, r.bottom() + 9.0), Align2::CENTER_CENTER, &format!("{:02}", ch.channel_id), display(11.0), MUTED, 0.5);
                if resp.clicked() {
                    if let Some(id) = lid {
                        self.lights.sel = Sel::Light(id);
                    }
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if ui.add(primary("Open in Live").icon(Icon::Arrow).height(HIT)).clicked() {
                    self.open_area_in_live(inv, &e.id.clone());
                }
                if ui.add(ghost("Channels").height(HIT)).clicked() {
                    self.lights.sel = Sel::Area(e.id.clone());
                }
            });
        });
    }
}

/// Rooms in bridge order with their lights; lights in no room come last.
fn ordered_rooms(inv: &Inventory) -> Vec<(Option<&lumen_hue::Group>, Vec<&Light>)> {
    let mut out: Vec<(Option<&lumen_hue::Group>, Vec<&Light>)> = inv.rooms.iter().map(|r| (Some(r), inv.group_lights(r))).collect();
    let orphans: Vec<&Light> = inv.lights.iter().filter(|l| inv.room_of_light(l).is_none()).collect();
    if !orphans.is_empty() {
        out.push((None, orphans));
    }
    out
}

fn used_in(inv: &Inventory, l: &Light) -> String {
    let mut parts = Vec::new();
    for e in &inv.ent_configs {
        let ids: Vec<u8> = e.channels.iter().filter(|c| inv.channel_lights(c).iter().any(|(x, _)| x.id == l.id)).map(|c| c.channel_id).collect();
        match ids.as_slice() {
            [] => {}
            [one] => parts.push(format!("Channel {one:02} in {} area", e.metadata.name.trim())),
            [first, .., last] => parts.push(format!("Channels {first:02} to {last:02} in {} area", e.metadata.name.trim())),
        }
    }
    for z in &inv.zones {
        if inv.group_lights(z).iter().any(|x| x.id == l.id) {
            parts.push(format!("in zone {}", z.metadata.name.trim()));
        }
    }
    if parts.is_empty() {
        "Not in an entertainment area or zone".into()
    } else {
        parts.join(" · ")
    }
}

impl LumenApp {
    fn load_editor(&mut self, inv: &Inventory) {
        let ed = &mut self.lights;
        if ed.loaded == ed.sel {
            return;
        }
        ed.loaded = ed.sel.clone();
        ed.confirm_delete = false;
        match &ed.sel {
            Sel::Light(id) => {
                if let Some(l) = inv.light(id) {
                    ed.name = l.metadata.name.trim().to_string();
                    ed.bri = l.brightness() as f32;
                    let c = l.color.as_ref().map(|c| Rgb::from_xy(c.xy.x, c.xy.y)).unwrap_or(Rgb::WHITE);
                    ed.color = to32(c);
                    let k = l.gradient_points() as usize;
                    let mut g: Vec<[u8; 3]> = l.current_gradient().iter().map(|c| c.to_u8()).collect();
                    g.resize(k, c.to_u8());
                    ed.grad = g;
                }
            }
            Sel::Room(id) | Sel::Zone(id) => {
                if let Some(g) = inv.room(id).or_else(|| inv.zone(id)) {
                    ed.name = g.metadata.name.trim().to_string();
                    let lights = inv.group_lights(g);
                    ed.per_light = lights
                        .iter()
                        .map(|l| (l.id.clone(), l.color.as_ref().map(|c| Rgb::from_xy(c.xy.x, c.xy.y).to_u8()).unwrap_or([255, 255, 255])))
                        .collect();
                    ed.members = lights.iter().map(|l| l.id.clone()).collect();
                }
            }
            Sel::NewZone => {
                ed.name = "New zone".into();
                ed.members.clear();
            }
            _ => {}
        }
    }


    fn editor_header(&mut self, ui: &mut egui::Ui, kind: &str) {
        ui.horizontal(|ui| {
            ui.label(label_caps(&format!("Edit · {kind}"), 15.0, ACCENT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(quiet("Back to light").height(36.0)).on_hover_text("Esc").clicked() {
                    self.lights.sel = Sel::None;
                }
            });
        });
    }

    fn rename_row(&mut self, ui: &mut egui::Ui, rtype: &'static str, id: &str) {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.lights.name).font(display_bold(22.0)).desired_width(ui.available_width() - 80.0).frame(false));
            if ui.add(ghost("Rename").small()).clicked() {
                let (id, name) = (id.to_string(), self.lights.name.clone());
                self.task("Rename", true, move |b| b.rename(rtype, &id, &name).map(|_| format!("Renamed to \"{name}\"")));
            }
        });
    }


    fn editor(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        match self.lights.sel.clone() {
            Sel::None | Sel::Light(_) => {}
            Sel::Room(id) => self.group_editor(ui, inv, &id, false),
            Sel::Zone(id) => self.group_editor(ui, inv, &id, true),
            Sel::Area(id) => self.area_editor(ui, inv, &id),
            Sel::NewZone => self.new_zone_editor(ui, inv),
        }
    }

    fn group_editor(&mut self, ui: &mut egui::Ui, inv: &Inventory, id: &str, is_zone: bool) {
        let Some(g) = (if is_zone { inv.zone(id) } else { inv.room(id) }).cloned() else {
            ui.label("Not found. Refresh?");
            return;
        };
        let mut lights: Vec<Light> = inv.group_lights(&g).into_iter().cloned().collect();
        lights.sort_by_key(|l| l.metadata.name.to_lowercase());
        let gl = inv.grouped_light_of(&g).map(str::to_string);

        self.editor_header(ui, if is_zone { "Zone" } else { "Room" });
        self.rename_row(ui, if is_zone { "zone" } else { "room" }, &g.id);
        ui.add_space(10.0);

        if let Some(gl) = &gl {
            section(ui, "Everything at once");
            let mut changed = false;
            ui.horizontal(|ui| {
                changed |= egui::color_picker::color_edit_button_srgba(ui, &mut self.lights.color, egui::color_picker::Alpha::Opaque).changed();
                ui.spacing_mut().slider_width = ui.available_width() - 60.0;
                if ui.add(egui::Slider::new(&mut self.lights.bri, 1.0..=100.0).suffix("%")).changed() {
                    self.pending = Some(Pending::Group(gl.clone(), LightState { brightness: Some(self.lights.bri as f64), ..Default::default() }));
                }
            });
            if let Some(p) = preset_row(ui) {
                self.lights.color = to32(rgb(p));
                changed = true;
            }
            if changed {
                self.pending = Some(Pending::Group(gl.clone(), LightState::color(from32(self.lights.color))));
            }
            ui.add_space(12.0);
        }

        section(ui, "Gradient across the lights");
        stops_editor(ui, &mut self.lights.stops, 6);
        if ui.add(primary("Apply gradient")).clicked() {
            let stops: Vec<Rgb> = self.lights.stops.iter().map(|c| rgb(*c)).collect();
            let refs: Vec<&Light> = lights.iter().collect();
            let plan = spread_across(&refs, &stops);
            for (id, st) in &plan {
                if let (Some(c), Some(e)) = (st.color, self.lights.per_light.get_mut(id)) {
                    *e = c.to_u8();
                }
            }
            self.task("Gradient", true, move |b| {
                for (id, st) in &plan {
                    b.set_light(id, st)?;
                    std::thread::sleep(Duration::from_millis(110));
                }
                Ok(format!("Gradient applied to {} lights", plan.len()))
            });
        }

        ui.add_space(14.0);
        section(ui, "Each light");
        for l in &lights {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::hover());
                glow(ui.painter(), r.center(), 7.0, l.current_rgb(), l.on.on);
                if ui.link(RichText::new(truncate(l.metadata.name.trim(), 26)).color(TEXT)).clicked() {
                    self.lights.sel = Sel::Light(l.id.clone());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if l.supports_color() {
                        let c = self.lights.per_light.entry(l.id.clone()).or_insert([255, 255, 255]);
                        if picker(ui, c) {
                            self.pending = Some(Pending::Light(l.id.clone(), LightState::color(rgb(*c))));
                        }
                    } else {
                        ui.label(RichText::new("white").color(FAINT));
                    }
                });
            });
        }

        if is_zone {
            ui.add_space(14.0);
            section(ui, "Members");
            self.members_picker(ui, inv);
            ui.horizontal(|ui| {
                if ui.add(primary("Save members")).clicked() {
                    let ids: Vec<String> = self.lights.members.iter().cloned().collect();
                    let zid = g.id.clone();
                    self.task("Zone", true, move |b| b.set_zone_lights(&zid, &ids).map(|_| "Zone updated".into()));
                }
            });
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.lights.confirm_delete, "I want to delete this zone");
                if ui.add_enabled(self.lights.confirm_delete, danger("Delete")).clicked() {
                    let zid = g.id.clone();
                    self.lights.sel = Sel::None;
                    self.task("Delete", true, move |b| b.delete(&format!("zone/{zid}")).map(|_| "Zone deleted".into()));
                }
            });
        }
    }

    fn members_picker(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        let mut all: Vec<&Light> = inv.lights.iter().collect();
        all.sort_by_key(|l| l.metadata.name.to_lowercase());
        for l in all {
            let mut on = self.lights.members.contains(&l.id);
            if ui.checkbox(&mut on, l.metadata.name.trim()).changed() {
                if on {
                    self.lights.members.insert(l.id.clone());
                } else {
                    self.lights.members.remove(&l.id);
                }
            }
        }
    }

    fn new_zone_editor(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        self.editor_header(ui, "New zone");
        ui.add(egui::TextEdit::singleline(&mut self.lights.name).font(semibold(20.0)).desired_width(f32::INFINITY));
        ui.add_space(8.0);
        section(ui, "Lights in this zone");
        self.members_picker(ui, inv);
        ui.add_space(8.0);
        if ui.add_enabled(!self.lights.members.is_empty(), primary("Create zone")).clicked() {
            let ids: Vec<String> = self.lights.members.iter().cloned().collect();
            let name = self.lights.name.clone();
            self.lights.sel = Sel::None;
            self.task("Create zone", true, move |b| b.create_zone(&name, &ids).map(|_| format!("Created zone \"{name}\"")));
        }
    }

    fn area_editor(&mut self, ui: &mut egui::Ui, inv: &Inventory, id: &str) {
        let Some(e) = inv.ent_config(id).cloned() else { return };
        self.editor_header(ui, "Entertainment area");
        ui.label(RichText::new(&e.metadata.name).font(semibold(20.0)));
        ui.label(RichText::new(format!("{} · {}", e.configuration_type, e.status)).color(FAINT));
        ui.add_space(10.0);
        for ch in &e.channels {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{:>2}", ch.channel_id)).monospace().color(ACCENT));
                let names: Vec<String> = inv.channel_lights(ch).iter().map(|(l, i)| format!("{} #{}", l.metadata.name.trim(), i)).collect();
                ui.label(names.join(", "));
            });
            ui.label(RichText::new(format!("     x {:+.2}  y {:+.2}  z {:+.2}", ch.position.x, ch.position.y, ch.position.z)).monospace().size(11.0).color(FAINT));
        }
        ui.add_space(10.0);
        if ui.add(primary("Open in Live").icon(Icon::Arrow)).clicked() {
            self.open_area_in_live(inv, &e.id.clone());
        }
    }
}
