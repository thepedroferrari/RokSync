//! The target: the one room, zone or whole home that every page sends to.
//!
//! Sync only streams to an entertainment area whose lights all sit inside the target.

use super::theme::*;
use super::LumenApp;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Rect, Sense, Stroke};
use lumen_hue::{EntConfig, Group, Inventory, Light};
use std::collections::HashSet;

pub const HOME: &str = "home";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Room,
    Zone,
    Home,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Room => "ROOM",
            Kind::Zone => "ZONE",
            Kind::Home => "EVERY ROOM",
        }
    }
}

pub struct Target<'a> {
    /// Group id, or [`HOME`].
    pub id: String,
    pub kind: Kind,
    pub group: Option<&'a Group>,
    pub name: String,
    pub lights: Vec<&'a Light>,
}

impl<'a> Target<'a> {
    pub fn ids(&self) -> HashSet<&'a str> {
        self.lights.iter().map(|l| l.id.as_str()).collect()
    }

    #[cfg(test)]
    pub fn contains(&self, light_id: &str) -> bool {
        self.lights.iter().any(|l| l.id == light_id)
    }

    pub fn is_home(&self) -> bool {
        self.kind == Kind::Home
    }

    /// Rooms that share no light with this target.
    pub fn other_rooms(&self, inv: &'a Inventory) -> Vec<&'a Group> {
        let ids = self.ids();
        let mut out: Vec<&Group> = inv
            .rooms
            .iter()
            .filter(|r| Some(r.id.as_str()) != self.group.map(|g| g.id.as_str()))
            .filter(|r| !inv.group_lights(r).iter().any(|l| ids.contains(l.id.as_str())))
            .collect();
        out.sort_by_key(|r| r.metadata.name.to_lowercase());
        out
    }
}

pub enum Fit<'a> {
    Fits(&'a EntConfig),
    /// The closest area also reaches lights outside the target. `fix` is the smallest room or zone
    /// that holds the whole area, if there is one.
    Spills { area: &'a EntConfig, outside: usize, rooms: Vec<String>, fix: Option<&'a Group> },
    /// No area has a light in the target.
    None,
    /// Whole home: sync streams to one area at a time, so it stays off.
    Home,
}

impl<'a> Fit<'a> {
    pub fn area(&self) -> Option<&'a EntConfig> {
        match self {
            Fit::Fits(a) => Some(a),
            _ => None,
        }
    }
}

fn by_name(gs: &[Group]) -> Vec<&Group> {
    let mut v: Vec<&Group> = gs.iter().collect();
    v.sort_by_key(|g| g.metadata.name.to_lowercase());
    v
}

/// The saved target, or a sensible room if it's gone. Never falls back to the whole home.
pub fn resolve<'a>(inv: &'a Inventory, saved: Option<&str>) -> Target<'a> {
    if saved == Some(HOME) {
        return home(inv);
    }
    let found = saved.and_then(|id| inv.room(id).map(|g| (g, Kind::Room)).or_else(|| inv.zone(id).map(|g| (g, Kind::Zone))));
    if let Some((g, kind)) = found {
        return group(inv, g, kind);
    }
    match default_room(inv) {
        Some(g) => group(inv, g, Kind::Room),
        // no rooms at all: every light is unassigned, so there is nothing narrower to pick
        None => home(inv),
    }
}

/// First pick: a room that can sync (an area fits inside it), else the first room by name.
pub fn default_room(inv: &Inventory) -> Option<&Group> {
    let rooms = by_name(&inv.rooms);
    rooms.iter().find(|r| matches!(fit(inv, &group(inv, r, Kind::Room), None), Fit::Fits(_))).or(rooms.first()).copied()
}

fn group<'a>(inv: &'a Inventory, g: &'a Group, kind: Kind) -> Target<'a> {
    Target { id: g.id.clone(), kind, group: Some(g), name: g.metadata.name.trim().to_string(), lights: inv.group_lights(g) }
}

fn home(inv: &Inventory) -> Target<'_> {
    Target { id: HOME.into(), kind: Kind::Home, group: None, name: "Whole home".into(), lights: inv.lights.iter().collect() }
}

pub fn all(inv: &Inventory) -> Vec<Target<'_>> {
    let mut out: Vec<Target> = by_name(&inv.rooms).into_iter().map(|g| group(inv, g, Kind::Room)).collect();
    out.extend(by_name(&inv.zones).into_iter().map(|g| group(inv, g, Kind::Zone)));
    out.push(home(inv));
    out
}

pub fn area_lights<'a>(inv: &'a Inventory, e: &EntConfig) -> Vec<&'a Light> {
    let mut out: Vec<&Light> = Vec::new();
    for ch in &e.channels {
        for (l, _) in inv.channel_lights(ch) {
            if !out.iter().any(|o| o.id == l.id) {
                out.push(l);
            }
        }
    }
    out
}

/// Which entertainment area sync may use for this target. `prefer` wins a tie between areas that fit.
pub fn fit<'a>(inv: &'a Inventory, t: &Target, prefer: Option<&str>) -> Fit<'a> {
    if t.is_home() {
        return Fit::Home;
    }
    let ids = t.ids();
    let mut fits: Vec<(&EntConfig, usize)> = Vec::new();
    let mut spills: Vec<(&EntConfig, usize, Vec<&Light>)> = Vec::new();
    for e in &inv.ent_configs {
        let ls = area_lights(inv, e);
        let inside = ls.iter().filter(|l| ids.contains(l.id.as_str())).count();
        if inside == 0 {
            continue;
        }
        let out: Vec<&Light> = ls.into_iter().filter(|l| !ids.contains(l.id.as_str())).collect();
        if out.is_empty() {
            fits.push((e, inside));
        } else {
            spills.push((e, inside, out));
        }
    }
    if let Some(p) = prefer {
        if let Some((e, _)) = fits.iter().find(|(e, _)| e.id == p) {
            return Fit::Fits(e);
        }
    }
    if let Some((e, _)) = fits.iter().max_by_key(|(_, n)| *n) {
        return Fit::Fits(e);
    }
    let Some((area, _, out)) = spills.into_iter().max_by_key(|(_, n, _)| *n) else { return Fit::None };
    let mut rooms: Vec<String> = out.iter().filter_map(|l| inv.room_of_light(l).map(|r| r.metadata.name.trim().to_string())).collect();
    rooms.sort();
    rooms.dedup();
    let need: HashSet<String> = area_lights(inv, area).iter().map(|l| l.id.clone()).collect();
    let fix = inv
        .zones
        .iter()
        .chain(inv.rooms.iter())
        .map(|g| (g, inv.group_lights(g)))
        .filter(|(_, ls)| need.iter().all(|id| ls.iter().any(|l| &l.id == id)))
        .min_by_key(|(_, ls)| ls.len())
        .map(|(g, _)| g);
    Fit::Spills { area, outside: out.len(), rooms, fix }
}

pub fn fit_line(f: &Fit) -> String {
    match f {
        Fit::Fits(a) => format!("Sync: {}", a.metadata.name.trim()),
        Fit::Spills { outside, rooms, .. } => format!("No sync: its area also covers {} in {}", plural(*outside), join(rooms)),
        Fit::None => "No sync area here".into(),
        Fit::Home => "No sync for the whole home".into(),
    }
}

pub fn join(names: &[String]) -> String {
    match names.len() {
        0 => "another room".into(),
        1 => names[0].clone(),
        _ => format!("{} and {}", names[..names.len() - 1].join(", "), names[names.len() - 1]),
    }
}

fn plural(n: usize) -> String {
    if n == 1 {
        "1 light".into()
    } else {
        format!("{n} lights")
    }
}

#[derive(Default)]
pub struct TargetUi {
    pub open: bool,
    confirm_home: bool,
}

impl LumenApp {
    pub fn target<'a>(&self, inv: &'a Inventory) -> Target<'a> {
        resolve(inv, self.cfg.target.as_deref())
    }

    pub fn sync_area<'a>(&self, inv: &'a Inventory) -> Option<&'a EntConfig> {
        let t = self.target(inv);
        fit(inv, &t, self.sy.area.as_deref()).area()
    }

    /// Opens the area in Live by targeting the smallest room or zone that holds all its lights.
    pub fn open_area_in_live(&mut self, inv: &Inventory, area_id: &str) {
        let Some(e) = inv.ent_config(area_id) else { return };
        let need: HashSet<&str> = area_lights(inv, e).iter().map(|l| l.id.as_str()).collect();
        let holder = inv
            .rooms
            .iter()
            .chain(inv.zones.iter())
            .map(|g| (g, inv.group_lights(g)))
            .filter(|(_, ls)| need.iter().all(|id| ls.iter().any(|l| l.id == *id)))
            .min_by_key(|(_, ls)| ls.len())
            .map(|(g, _)| g.id.clone());
        match holder {
            Some(id) => {
                self.sy.area = Some(area_id.to_string());
                self.set_target(inv, &id);
                self.page = super::Page::Sync;
            }
            None => self.toast(
                format!("No room or zone holds all of {}. Make a zone with its lights in Lights, then sync it.", e.metadata.name.trim()),
                true,
            ),
        }
    }

    pub fn set_target(&mut self, inv: &Inventory, id: &str) {
        if self.cfg.target.as_deref() == Some(id) {
            return;
        }
        if self.is_streaming() {
            self.stop_live();
        }
        self.cfg.target = Some(id.to_string());
        self.save_cfg();
        let t = self.target(inv);
        self.home.open = t.group.map(|g| g.id.clone());
        self.bg.lock().unwrap().note("Controlling", &t.name, ACCENT);
    }

    pub fn target_bar(&mut self, ctx: &egui::Context, inv: &Inventory) {
        let t = self.target(inv);
        if self.cfg.target.is_none() {
            // pin the first pick so it doesn't move when rooms change
            self.cfg.target = Some(t.id.clone());
            self.save_cfg();
        }
        let f = fit(inv, &t, self.sy.area.as_deref());
        let live = self.is_streaming();
        let mut toggle = false;
        let mut change_rect = Rect::NOTHING;
        egui::TopBottomPanel::top("target").exact_height(76.0).frame(egui::Frame::none().fill(SURFACE)).show_separator_line(false).show(ctx, |ui| {
            let r = ui.max_rect();
            let p = ui.painter().clone();
            if t.is_home() {
                let mut x = r.left() - r.height();
                while x < r.right() {
                    p.line_segment([pos2(x, r.bottom()), pos2(x + r.height(), r.top())], Stroke::new(5.0_f32, hex("#1A1410")));
                    x += 14.0;
                }
            }
            rule(&p, r.bottom() - 1.0, r.left(), r.right());
            let bar = Rect::from_min_size(r.min, vec2(10.0, r.height() - 1.0));
            let cols = super::home::room_colors(&t.lights);
            if !t.is_home() && !cols.is_empty() {
                paint_gradient_v(&p, bar, &cols);
            } else {
                hatch(&p, bar, SURFACE);
            }
            let x = bar.right() + 16.0;
            let k = text_at(&p, pos2(x, r.center().y - 9.0), Align2::LEFT_CENTER, "CONTROLLING", display(13.0), MUTED, 1.6);
            text_at(&p, pos2(x, r.center().y + 9.0), Align2::LEFT_CENTER, t.kind.label(), display(12.0), if t.is_home() { ACCENT } else { SOFT }, 1.2);
            let nx = k.right() + 22.0;
            // Centre the visible caps and the line under them, not the text boxes: a 40 px Barlow
            // box carries ~11 px of air above the caps.
            const SIZE: f32 = 40.0;
            let (cap_air, cap_h, gap) = (SIZE * 0.275, SIZE * 0.725, 9.0);
            let on = t.lights.iter().filter(|l| l.on.on).count();
            let sub = p.layout_no_wrap(format!("{} · {on} on · {}", plural(t.lights.len()), fit_line(&f)), body(13.5), SOFT);
            let sub_air = sub.size().y * 0.18; // above the digits and capitals of the body face
            let seen = cap_h + gap + sub.size().y - sub_air * 2.0;
            let caps_top = r.center().y - seen / 2.0;
            let name = text_at(&p, pos2(nx, caps_top - cap_air), Align2::LEFT_TOP, &t.name.to_uppercase(), display_bold(SIZE), TEXT, -0.4);
            p.galley(pos2(nx, caps_top + cap_h + gap - sub_air), sub, SOFT);
            if live {
                let g = p.layout_job(spaced("LIVE HERE", display_bold(14.0), Color32::PLACEHOLDER, 1.2));
                let lr = Rect::from_min_size(pos2(name.right() + 22.0, r.center().y - 14.0), vec2(g.size().x + 20.0, 28.0));
                p.rect_filled(lr, 0.0, ACCENT);
                p.galley(lr.center() - g.size() / 2.0, g, INK);
            }
            let g = p.layout_job(spaced("CHANGE", display_bold(18.0), Color32::PLACEHOLDER, 1.0));
            let cw = (g.size().x + 64.0).max(168.0);
            let cr = Rect::from_min_max(pos2(r.right() - cw, r.top()), pos2(r.right(), r.bottom() - 1.0));
            change_rect = cr;
            let resp = ui.interact(cr, ui.id().with("target-change"), Sense::click()).on_hover_text("Pick the room or zone RokSync controls");
            let hov = ease_bool(ui, resp.id.with("h"), resp.hovered() || self.target_ui.open, 0.15);
            p.rect_filled(cr, 0.0, mix(SURFACE, TEXT, hov));
            p.line_segment([cr.left_top(), cr.left_bottom()], Stroke::new(1.0_f32, RULE));
            let fg = mix(TEXT, INK, hov);
            p.galley(pos2(cr.center().x - g.size().x / 2.0 - 10.0, cr.center().y - g.size().y / 2.0), g, fg);
            let rot = ease_bool(ui, resp.id.with("rot"), self.target_ui.open, 0.25);
            super::home::chevron(&p, pos2(cr.center().x + 46.0, cr.center().y), rot, fg);
            if resp.has_focus() {
                focus_ring(&p, cr);
            }
            let hint = egui::text::LayoutJob::simple("Switches, moods and sync on every tab go here.".into(), body(13.0), MUTED, 230.0);
            let hg = p.layout_job(hint);
            let hx = cr.left() - 18.0 - hg.size().x;
            if hx > name.right().max(nx) + 140.0 {
                p.line_segment([pos2(hx - 18.0, r.top()), pos2(hx - 18.0, r.bottom())], Stroke::new(1.0_f32, RULE));
                p.galley(pos2(hx, r.center().y - hg.size().y / 2.0), hg, MUTED);
            }
            toggle = resp.clicked();
        });
        if toggle {
            self.target_ui.open = !self.target_ui.open;
            self.target_ui.confirm_home = false;
        }
        if self.target_ui.open {
            self.target_picker(ctx, inv, change_rect, live);
        }
    }

    fn target_picker(&mut self, ctx: &egui::Context, inv: &Inventory, anchor: Rect, live: bool) {
        let cur = self.target(inv).id;
        let rows = all(inv);
        let w = 720.0_f32.min(ctx.screen_rect().width() - 2.0 * PAD_X);
        let mut pick: Option<String> = None;
        let mut close = false;
        let area = egui::Area::new(egui::Id::new("target-picker"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(anchor.right() - PAD_X - w, anchor.bottom()))
            .show(ctx, |ui| {
                ui.set_width(w);
                let start = ui.cursor().min;
                let bg = ui.painter().add(egui::Shape::Noop);
                ui.spacing_mut().item_spacing.y = 0.0;
                if live {
                    let (r, _) = ui.allocate_exact_size(vec2(w, 42.0), Sense::hover());
                    ui.painter().rect_filled(r, 0.0, hex("#2A1A12"));
                    ui.painter().text(pos2(r.left() + 18.0, r.center().y), Align2::LEFT_CENTER, "Sync is live. Picking somewhere else stops it first.", body(13.5), TEXT);
                }
                let mut last: Option<Kind> = None;
                for t in &rows {
                    if last != Some(t.kind) {
                        let label = match t.kind {
                            Kind::Room => "ROOMS",
                            Kind::Zone => "ZONES",
                            Kind::Home => "EVERYTHING",
                        };
                        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
                        text_at(ui.painter(), pos2(r.left() + 18.0, r.bottom() - 8.0), Align2::LEFT_BOTTOM, label, display(13.0), MUTED, 1.6);
                        last = Some(t.kind);
                    }
                    if self.picker_row(ui, inv, t, t.id == cur, w) {
                        if t.is_home() && cur != HOME {
                            self.target_ui.confirm_home = true;
                        } else {
                            pick = Some(t.id.clone());
                        }
                    }
                }
                if self.target_ui.confirm_home {
                    let n = inv.lights.len();
                    let rooms = inv.rooms.len();
                    egui::Frame::none().fill(SURFACE2).inner_margin(egui::Margin::symmetric(18.0, 16.0)).show(ui, |ui| {
                        ui.set_width(w - 36.0);
                        ui.spacing_mut().item_spacing.y = 10.0;
                        ui.label(headline(&format!("Control all {n} lights?"), 22.0));
                        ui.label(
                            egui::RichText::new(format!("Moods, colours and power reach all {rooms} rooms. Sync stays off (it runs on one area at a time)."))
                                .font(body(14.0))
                                .color(SOFT),
                        );
                        ui.horizontal(|ui| {
                            if ui.add(primary("Yes, whole home").height(44.0)).clicked() {
                                pick = Some(HOME.into());
                            }
                            ui.add_space(8.0);
                            if ui.add(ghost("Cancel").height(44.0)).clicked() {
                                self.target_ui.confirm_home = false;
                            }
                        });
                    });
                }
                let (r, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
                rule(ui.painter(), r.top(), r.left(), r.right());
                ui.painter().text(pos2(r.left() + 18.0, r.center().y), Align2::LEFT_CENTER, "RokSync remembers your pick, but never starts on Whole home.", body(12.5), MUTED);
                let block = Rect::from_min_max(start, pos2(start.x + w, ui.min_rect().bottom()));
                ui.painter().set(bg, egui::Shape::rect_filled(block, 0.0, SURFACE));
                ui.painter().rect_stroke(block, 0.0, Stroke::new(1.0_f32, hex("#2A2A2F")));
            });
        if area.response.clicked_elsewhere() && !ctx.input(|i| i.pointer.interact_pos().map(|p| anchor.contains(p)).unwrap_or(false)) {
            close = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if let Some(id) = pick {
            self.set_target(inv, &id);
            close = true;
        }
        if close {
            self.target_ui.open = false;
            self.target_ui.confirm_home = false;
        }
    }

    fn picker_row(&self, ui: &mut egui::Ui, inv: &Inventory, t: &Target, sel: bool, w: f32) -> bool {
        let (r, resp) = ui.allocate_exact_size(vec2(w, 56.0), Sense::click());
        let p = ui.painter();
        let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
        let base = if sel { hex("#1F1712") } else { SURFACE };
        p.rect_filled(r, 0.0, mix(base, SURFACE2, hov));
        rule(p, r.top(), r.left(), r.right());
        row_cursor(ui, resp.id, r, resp.hovered());
        let bar = Rect::from_min_size(r.min + vec2(0.0, 1.0), vec2(10.0, r.height() - 1.0));
        let cols = super::home::room_colors(&t.lights);
        if !t.is_home() && !cols.is_empty() {
            paint_gradient_v(p, bar, &cols);
        } else {
            hatch(p, bar, SURFACE);
        }
        let on = t.lights.iter().filter(|l| l.on.on).count();
        let nm = text_at(p, pos2(bar.right() + 14.0, r.top() + 8.0), Align2::LEFT_TOP, &t.name.to_uppercase(), display_bold(22.0), TEXT, 0.0);
        let sub = if t.is_home() { format!("{} rooms · asks first", inv.rooms.len()) } else if on == 0 { "All off".into() } else { format!("{on} on") };
        p.text(pos2(nm.left(), nm.bottom() + 1.0), Align2::LEFT_TOP, sub, body(12.5), MUTED);
        let tag_x = r.right() - 18.0;
        text_at(p, pos2(tag_x, r.center().y), Align2::RIGHT_CENTER, if sel { "CURRENT" } else { "CONTROL" }, display_bold(14.0), if sel { ACCENT } else { MUTED }, 1.2);
        let f = fit(inv, t, self.sy.area.as_deref());
        let (c, fg) = match &f {
            Fit::Fits(_) => (GOOD, SOFT),
            Fit::Spills { .. } => (WARN, WARN),
            _ => (EDGE, MUTED),
        };
        let sx = tag_x - 96.0 - 250.0;
        chip(p, pos2(sx, r.center().y), c);
        let line = egui::text::LayoutJob::simple(fit_line(&f), body(13.0), fg, 240.0);
        let g = p.layout_job(line);
        p.galley(pos2(sx + 12.0, r.center().y - g.size().y / 2.0), g, fg);
        num_unit(p, pos2(sx - 24.0, r.center().y), Align2::RIGHT_CENTER, &t.lights.len().to_string(), " LIGHTS", 18.0, TEXT);
        if resp.has_focus() {
            focus_ring(p, r);
        }
        resp.clicked()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inv() -> Inventory {
        crate::demo::inventory()
    }

    #[test]
    fn unknown_target_never_means_whole_home() {
        let inv = inv();
        let t = resolve(&inv, Some("gone"));
        assert_ne!(t.kind, Kind::Home);
        let t = resolve(&inv, None);
        assert_ne!(t.kind, Kind::Home);
    }

    #[test]
    fn home_only_when_saved() {
        let inv = inv();
        assert_eq!(resolve(&inv, Some(HOME)).kind, Kind::Home);
        assert!(matches!(fit(&inv, &resolve(&inv, Some(HOME)), None), Fit::Home));
    }

    #[test]
    fn sync_never_reaches_outside_the_target() {
        let inv = inv();
        for t in all(&inv) {
            if let Fit::Fits(a) = fit(&inv, &t, None) {
                for l in area_lights(&inv, a) {
                    assert!(t.contains(&l.id), "{} streams to {} outside {}", a.metadata.name, l.metadata.name, t.name);
                }
            }
        }
    }
}
