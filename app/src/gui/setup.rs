//! Setup: the first-run picks (screen, room and sync area, sound output). Settings shows the same
//! lists, so a pick made in either place is the same pick.

use super::target::{self, area_lights, Fit};
use super::theme::*;
use super::LumenApp;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Margin, Rect, RichText, Sense, Stroke};
use lumen_cod::{AudioHandle, AudioLevels, SharedAudio};
use lumen_hue::{Inventory, Rgb};
use lumen_screen::MonitorInfo;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const STEPS: [&str; 4] = ["BRIDGE", "SCREEN", "ROOM", "SOUND"];
const DONE: usize = 4;

#[derive(Default)]
pub struct SetupUi {
    /// Shown even though setup is done (Run setup again).
    pub open: bool,
    pub step: usize,
    monitors: Option<Vec<MonitorInfo>>,
    devices: Option<(Vec<String>, Option<String>)>,
    meters: Option<Meters>,
    meters_seen: bool,
}

/// One loopback per output while a sound list is on screen, so you can see which one plays.
struct Meters {
    rows: Vec<(String, SharedAudio)>,
    _keep: Arc<Mutex<Vec<AudioHandle>>>,
}

impl Meters {
    fn open(devices: &[String]) -> Self {
        let keep: Arc<Mutex<Vec<AudioHandle>>> = Arc::default();
        let rows: Vec<(String, SharedAudio)> = devices.iter().map(|d| (d.clone(), SharedAudio::default())).collect();
        for (d, a) in &rows {
            let (d, a, keep) = (d.clone(), a.clone(), keep.clone());
            // opening can take a moment per device: never on the UI thread
            let _ = std::thread::Builder::new().name("meter".into()).spawn(move || {
                if let Ok(h) = lumen_cod::spawn_loopback_on(a, Some(d)) {
                    keep.lock().unwrap().push(h);
                }
            });
        }
        Self { rows, _keep: keep }
    }

    fn levels(&self, device: &str) -> Option<AudioLevels> {
        self.rows.iter().find(|(d, _)| d == device).and_then(|(_, a)| a.lock().ok().map(|l| l.numbers()))
    }
}

/// 0..1 on a 54 dB scale; silence (no fresh audio) is 0.
fn db_frac(rms: f32) -> f32 {
    ((20.0 * rms.max(1e-6).log10() + 54.0) / 54.0).clamp(0.0, 1.0)
}

/// A small extra display that is not the primary is almost always a soundbar or AV receiver on
/// HDMI: Windows lists it as a screen, but nothing shows on it.
pub fn looks_like_audio(m: &MonitorInfo, primary: Option<&str>) -> bool {
    primary != Some(m.name.as_str()) && (m.width as i64) * (m.height as i64) <= 1280 * 720
}

enum Bar<'a> {
    Accent,
    Colors(&'a [Rgb]),
}

struct Row<'a> {
    title: &'a str,
    caps: bool,
    sub: &'a str,
    sub_col: Color32,
    tag: &'a str,
    tag_col: Color32,
    bar: Bar<'a>,
    sel: bool,
    enabled: bool,
    meter: Option<f32>,
}

fn pick_row(ui: &mut egui::Ui, id: egui::Id, row: Row, h: f32) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), if row.enabled { Sense::click() } else { Sense::hover() });
    let hov = ease_bool(ui, id.with("h"), row.enabled && resp.hovered(), 0.15);
    let p = ui.painter();
    p.rect_filled(r, 0.0, if row.sel { SURFACE2 } else { mix(BG, HOVER, hov) });
    row_rule(p, r.top(), r.left(), r.right());
    let bar = Rect::from_min_size(r.min + vec2(0.0, 1.0), vec2(6.0, r.height() - 1.0));
    match row.bar {
        Bar::Colors(c) if !c.is_empty() => paint_gradient_v(p, bar, c),
        _ if row.sel => {
            p.rect_filled(bar, 0.0, ACCENT);
        }
        _ => {}
    }
    let x = bar.right() + 16.0;
    let ink = if row.enabled { TEXT } else { MUTED };
    let (font, sp) = if row.caps { (display_bold(21.0), 0.4) } else { (semibold(15.5), 0.0) };
    let title = if row.caps { row.title.to_uppercase() } else { row.title.to_string() };
    let right_w = if row.meter.is_some() { 150.0 } else { 0.0 } + if row.tag.is_empty() { 0.0 } else { 170.0 };
    let tw = ((r.width() - (x - r.left()) - right_w - 16.0) / 8.5).max(12.0) as usize;
    text_at(p, pos2(x, r.center().y - 10.0), Align2::LEFT_CENTER, &truncate(&title, tw), font, ink, sp);
    text_at(p, pos2(x, r.center().y + 13.0), Align2::LEFT_CENTER, &truncate(row.sub, tw + 10), body(13.0), row.sub_col, 0.0);
    let mut rx = r.right() - 16.0;
    if let Some(f) = row.meter {
        let mr = Rect::from_min_max(pos2(rx - 120.0, r.center().y - 9.0), pos2(rx, r.center().y + 9.0));
        meter(p, mr, f, 10, if row.sel { ACCENT } else { TEXT });
        rx = mr.left() - 24.0;
    }
    if !row.tag.is_empty() {
        let g = text_at(p, pos2(rx, r.center().y), Align2::RIGHT_CENTER, row.tag, display(14.0), row.tag_col, 1.2);
        chip(p, pos2(g.left() - 10.0, r.center().y), row.tag_col);
    }
    if row.sel {
        p.rect_stroke(r.shrink(1.0), 0.0, Stroke::new(2.0_f32, ACCENT));
    }
    if resp.has_focus() {
        focus_ring(p, r);
    }
    if row.enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    row.enabled && resp.clicked()
}

impl LumenApp {
    pub fn setup_due(&self) -> bool {
        self.setup.open || (!self.cfg.setup_done && !self.demo)
    }

    fn monitors(&mut self) -> Vec<MonitorInfo> {
        self.setup.monitors.get_or_insert_with(|| lumen_screen::capture::monitors().unwrap_or_default()).clone()
    }

    fn current_monitor(&mut self, mons: &[MonitorInfo]) -> Option<MonitorInfo> {
        let want = self.cfg.screen.monitor.clone();
        want.and_then(|n| mons.iter().find(|m| m.name == n).cloned()).or_else(|| lumen_screen::primary(mons.to_vec()))
    }

    fn pick_monitor(&mut self, m: MonitorInfo) {
        self.cfg.screen.monitor = Some(m.name.clone());
        self.screen.lock().unwrap().monitor = Some(m);
        self.save_cfg();
    }

    fn pick_device(&mut self, d: Option<String>) {
        self.sound.set_device(d.clone());
        self.cfg.sound.device = d;
        self.save_cfg();
    }

    fn devices(&mut self) -> (Vec<String>, Option<String>) {
        self.setup.devices.get_or_insert_with(|| (lumen_cod::output_devices(), lumen_cod::default_output())).clone()
    }

    /// Keeps the per-output meters open while a sound list is drawn this frame.
    fn meters(&mut self) -> &Meters {
        self.setup.meters_seen = true;
        let (devs, _) = self.devices();
        self.setup.meters.get_or_insert_with(|| Meters::open(&devs))
    }

    /// End of frame: meters close once no sound list is on screen.
    pub fn setup_frame_end(&mut self) {
        if !std::mem::take(&mut self.setup.meters_seen) {
            self.setup.meters = None;
        }
    }

    pub fn screen_summary(&mut self) -> (String, String) {
        let mons = self.monitors();
        let primary = lumen_screen::primary(mons.clone()).map(|m| m.name);
        match self.current_monitor(&mons) {
            Some(m) => {
                let name = m.name.trim_start_matches(r"\\.\").replace("DISPLAY", "Display ");
                let note = if primary.as_deref() == Some(m.name.as_str()) { format!("{name} · primary") } else { name };
                (format!("{} × {}", m.width, m.height), note)
            }
            None => ("No screen found".into(), String::new()),
        }
    }

    pub fn sound_summary(&mut self) -> String {
        match &self.cfg.sound.device {
            Some(d) => d.clone(),
            None => "Whatever Windows plays to".into(),
        }
    }

    pub fn screen_list(&mut self, ui: &mut egui::Ui) {
        let mons = self.monitors();
        let primary = lumen_screen::primary(mons.clone()).map(|m| m.name);
        let cur = self.current_monitor(&mons).map(|m| m.name);
        let mut pick = None;
        for m in &mons {
            let audio = looks_like_audio(m, primary.as_deref());
            let name = m.name.trim_start_matches(r"\\.\").replace("DISPLAY", "Display ");
            let title = format!("{name} · {} × {}", m.width, m.height);
            let (sub, col) = if audio {
                ("Probably a soundbar or receiver on HDMI. Nothing shows on it.", WARN)
            } else if primary.as_deref() == Some(m.name.as_str()) {
                ("Primary", MUTED)
            } else {
                ("", MUTED)
            };
            let row = Row { title: &title, caps: false, sub, sub_col: col, tag: "", tag_col: MUTED, bar: Bar::Accent, sel: cur.as_deref() == Some(m.name.as_str()), enabled: true, meter: None };
            if pick_row(ui, ui.id().with(("mon", &m.name)), row, 60.0) {
                pick = Some(m.clone());
            }
        }
        if mons.is_empty() {
            ui.label(RichText::new("No screen found. Screen sync needs a display on this PC's graphics card.").color(MUTED));
        }
        if let Some(m) = pick {
            self.pick_monitor(m);
        }
    }

    /// The monitors drawn to scale, for the first-run screen step.
    fn screen_cards(&mut self, ui: &mut egui::Ui) {
        let mons = self.monitors();
        let primary = lumen_screen::primary(mons.clone()).map(|m| m.name);
        let cur = self.current_monitor(&mons).map(|m| m.name);
        let max_w = mons.iter().map(|m| m.width).max().unwrap_or(1).max(1) as f32;
        let mut pick = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(16.0, 16.0);
            for m in &mons {
                let audio = looks_like_audio(m, primary.as_deref());
                let aw = (320.0 * m.width as f32 / max_w).max(96.0);
                let ah = aw * m.height as f32 / m.width.max(1) as f32;
                let card_w = aw.max(300.0) + 36.0;
                let card_h = 200.0 + 36.0 + 96.0;
                let (r, resp) = ui.allocate_exact_size(vec2(card_w, card_h), Sense::click());
                let sel = cur.as_deref() == Some(m.name.as_str());
                let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
                let p = ui.painter();
                p.rect_filled(r, 0.0, mix(SURFACE, SURFACE2, hov));
                let art = Rect::from_min_size(pos2(r.left() + 18.0, r.top() + 18.0 + (200.0 - ah)), vec2(aw, ah));
                if audio {
                    hatch(p, art, SURFACE);
                } else {
                    p.rect_filled(art, 0.0, if sel { hex("#2D1B12") } else { SURFACE3 });
                }
                p.rect_stroke(art, 0.0, Stroke::new(1.0_f32, EDGE));
                let tag = if audio { ("AUDIO?", WARN) } else if primary.as_deref() == Some(m.name.as_str()) { ("MAIN", TEXT) } else { ("", TEXT) };
                text_at(p, art.left_bottom() + vec2(8.0, -10.0), Align2::LEFT_CENTER, tag.0, display_bold(14.0), tag.1, 1.0);
                let ty = r.top() + 18.0 + 200.0 + 22.0;
                num_unit(p, pos2(r.left() + 18.0, ty), Align2::LEFT_CENTER, &format!("{} × {}", m.width, m.height), "", 26.0, TEXT);
                let name = m.name.trim_start_matches(r"\\.\").replace("DISPLAY", "DISPLAY ");
                let role = if primary.as_deref() == Some(m.name.as_str()) { format!("{name} · PRIMARY") } else { name };
                text_at(p, pos2(r.left() + 18.0, ty + 24.0), Align2::LEFT_CENTER, &role, display(13.0), MUTED, 1.2);
                let note = if audio { "Probably a soundbar or receiver on HDMI. Windows lists it as a screen, but nothing shows on it." } else if sel { "Screen sync reads this one." } else { "" };
                let job = egui::text::LayoutJob::simple(note.into(), body(13.5), if audio { WARN } else { SOFT }, card_w - 36.0);
                p.galley(pos2(r.left() + 18.0, ty + 40.0), p.layout_job(job), SOFT);
                if sel {
                    p.rect_stroke(r.shrink(1.0), 0.0, Stroke::new(2.0_f32, ACCENT));
                }
                if resp.has_focus() {
                    focus_ring(p, r);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    pick = Some(m.clone());
                }
            }
        });
        if let Some(m) = pick {
            self.pick_monitor(m);
        }
    }

    pub fn room_list(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        let cur = self.target(inv).id;
        let mut pick: Option<(String, Option<String>)> = None;
        for t in target::all(inv).into_iter().filter(|t| !t.is_home()) {
            let f = target::fit(inv, &t, None);
            let (tag, col) = match &f {
                Fit::Fits(_) => ("SYNC READY", GOOD),
                Fit::Spills { .. } => ("NO SYNC HERE", MUTED),
                _ => ("NO AREA", MUTED),
            };
            let sub = format!("{} · {}", if t.kind == target::Kind::Zone { "Zone" } else { "Room" }, plural(t.lights.len()));
            let cols = super::home::room_colors(&t.lights);
            let row = Row { title: &t.name, caps: true, sub: &sub, sub_col: MUTED, tag, tag_col: col, bar: Bar::Colors(&cols), sel: t.id == cur, enabled: true, meter: None };
            if pick_row(ui, ui.id().with(("room", &t.id)), row, 64.0) {
                pick = Some((t.id.clone(), f.area().map(|a| a.id.clone())));
            }
        }
        if let Some((id, area)) = pick {
            self.set_target(inv, &id);
            if let Some(a) = area {
                self.sy.area = Some(a.clone());
                self.cfg.last_area = Some(a);
                self.save_cfg();
            }
        }
    }

    pub fn area_summary(&self, inv: &Inventory) -> (String, String) {
        let t = self.target(inv);
        match target::fit(inv, &t, self.sy.area.as_deref()) {
            Fit::Fits(a) => (a.metadata.name.trim().to_string(), format!("{} in {}", plural(area_lights(inv, a).len()), t.name)),
            _ => ("None".into(), format!("No sync area fits inside {}. Make one in the Hue app.", t.name)),
        }
    }

    pub fn area_list(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        let t = self.target(inv);
        let ids = t.ids();
        let cur = target::fit(inv, &t, self.sy.area.as_deref()).area().map(|a| a.id.clone());
        let mut pick = None;
        let mut any = false;
        for e in &inv.ent_configs {
            let ls = area_lights(inv, e);
            let inside = ls.iter().filter(|l| ids.contains(l.id.as_str())).count();
            if inside == 0 {
                continue;
            }
            any = true;
            let out: Vec<_> = ls.iter().filter(|l| !ids.contains(l.id.as_str())).collect();
            let ok = out.is_empty();
            let sub = if ok {
                format!("{}, all in {}", plural(inside), t.name)
            } else {
                let mut rooms: Vec<String> = out.iter().filter_map(|l| inv.room_of_light(l).map(|r| r.metadata.name.trim().to_string())).collect();
                rooms.sort();
                rooms.dedup();
                format!("Also lights {} in {}", plural(out.len()), target::join(&rooms))
            };
            let row = Row {
                title: e.metadata.name.trim(),
                caps: true,
                sub: &sub,
                sub_col: MUTED,
                tag: if ok { "" } else { "SPILLS OUT" },
                tag_col: MUTED,
                bar: Bar::Accent,
                sel: cur.as_deref() == Some(e.id.as_str()),
                enabled: ok,
                meter: None,
            };
            if pick_row(ui, ui.id().with(("area", &e.id)), row, 64.0) {
                pick = Some(e.id.clone());
            }
        }
        if !any {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 84.0), Sense::hover());
            hatch(ui.painter(), r, SURFACE);
            text_at(ui.painter(), pos2(r.left() + 18.0, r.top() + 28.0), Align2::LEFT_CENTER, "NO SYNC AREA HERE", display_bold(20.0), TEXT, 0.4);
            text_at(ui.painter(), pos2(r.left() + 18.0, r.top() + 56.0), Align2::LEFT_CENTER, "Colours and moods still work. For screen and sound sync, make an entertainment area in the Hue app.", body(13.0), SOFT, 0.0);
        }
        if let Some(id) = pick {
            self.sy.area = Some(id.clone());
            self.cfg.last_area = Some(id);
            self.save_cfg();
        }
    }

    pub fn sound_list(&mut self, ui: &mut egui::Ui) {
        let (devs, default) = self.devices();
        let now = Instant::now();
        let levels: Vec<Option<AudioLevels>> = {
            let m = self.meters();
            devs.iter().map(|d| m.levels(d)).collect()
        };
        let frac = |l: &Option<AudioLevels>| l.as_ref().filter(|l| l.fresh(now)).map(|l| db_frac(l.rms)).unwrap_or(0.0);
        let mut pick: Option<Option<String>> = None;
        let def_i = default.as_ref().and_then(|d| devs.iter().position(|x| x == d));
        let def_sub = match &default {
            Some(d) => format!("Follows your default output, now {d}"),
            None => "Follows your default output".into(),
        };
        let row = Row {
            title: "Whatever Windows plays to",
            caps: false,
            sub: &def_sub,
            sub_col: MUTED,
            tag: "",
            tag_col: MUTED,
            bar: Bar::Accent,
            sel: self.cfg.sound.device.is_none(),
            enabled: true,
            meter: Some(def_i.map(|i| frac(&levels[i])).unwrap_or(0.0)),
        };
        if pick_row(ui, ui.id().with("snd-default"), row, 64.0) {
            pick = Some(None);
        }
        for (i, d) in devs.iter().enumerate() {
            let playing = levels[i].as_ref().is_some_and(|l| l.fresh(now));
            let sub = if Some(d) == default.as_ref() { "Default output" } else if playing { "Playing" } else { "Not playing anything" };
            let row = Row {
                title: d,
                caps: false,
                sub,
                sub_col: MUTED,
                tag: "",
                tag_col: MUTED,
                bar: Bar::Accent,
                sel: self.cfg.sound.device.as_deref() == Some(d.as_str()),
                enabled: true,
                meter: Some(frac(&levels[i])),
            };
            if pick_row(ui, ui.id().with(("snd", d)), row, 64.0) {
                pick = Some(Some(d.clone()));
            }
        }
        if let Some(d) = self.cfg.sound.device.clone().filter(|d| !devs.contains(d)) {
            let row = Row { title: &d, caps: false, sub: "Not connected right now", sub_col: WARN, tag: "", tag_col: MUTED, bar: Bar::Accent, sel: true, enabled: false, meter: None };
            pick_row(ui, ui.id().with("snd-gone"), row, 64.0);
        }
        if let Some(d) = pick {
            self.pick_device(d);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(50));
    }

    /// Bass, mids and highs of the chosen output, for the HEARING NOW panel.
    fn hearing(&mut self) -> [f32; 3] {
        let (_, default) = self.devices();
        let dev = self.cfg.sound.device.clone().or(default);
        let now = Instant::now();
        let l = dev.and_then(|d| self.meters().levels(&d)).filter(|l| l.fresh(now));
        match l {
            Some(l) => [db_frac(l.low * 2.0), db_frac(l.mid * 2.0), db_frac(l.high * 4.0)],
            None => [0.0; 3],
        }
    }

    pub fn finish_setup(&mut self) {
        self.cfg.setup_done = true;
        self.setup.open = false;
        self.setup.step = 0;
        self.setup.meters = None;
        self.save_cfg();
    }

    pub fn setup_view(&mut self, ctx: &egui::Context, inv: &Arc<Inventory>) {
        let step = self.setup.step.min(DONE);
        let t = self.target(inv);
        let room = t.name.clone();
        let (area_name, _) = self.area_summary(inv);
        let has_area = target::fit(inv, &t, self.sy.area.as_deref()).area().is_some();
        drop(t);

        egui::TopBottomPanel::top("setup-rail").exact_height(76.0).frame(egui::Frame::none().fill(BG)).show_separator_line(false).show(ctx, |ui| {
            let full = ui.max_rect();
            let w = full.width() / 4.0;
            rule(ui.painter(), full.bottom() - 1.0, full.left(), full.right());
            for (i, label) in STEPS.iter().enumerate() {
                let r = Rect::from_min_size(pos2(full.left() + w * i as f32, full.top()), vec2(w, full.height()));
                let resp = ui.interact(r, ui.id().with(("rail", i)), Sense::click());
                let (cur, done) = (step == i, step > i);
                let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
                let p = ui.painter();
                let state = if done { "DONE" } else if cur { "NOW" } else { "NEXT" };
                let nc = if done { GOOD } else if cur { ACCENT } else { MUTED };
                text_at(p, pos2(r.left() + PAD_X, r.top() + 24.0), Align2::LEFT_CENTER, &format!("0{} · {state}", i + 1), display(13.0), nc, 1.2);
                let fg = if cur || done { TEXT } else { mix(MUTED, TEXT, hov) };
                text_at(p, pos2(r.left() + PAD_X, r.top() + 48.0), Align2::LEFT_CENTER, label, display_bold(24.0), fg, 0.4);
                let line = if cur { ACCENT } else if done { TEXT } else { mix(Color32::TRANSPARENT, SURFACE3, hov) };
                p.rect_filled(Rect::from_min_max(pos2(r.left(), r.bottom() - 5.0), pos2(r.right(), r.bottom() - 1.0)), 0.0, line);
                if resp.clicked() {
                    self.setup.step = i;
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
        });

        let note = match step {
            0 => "Takes about a minute".to_string(),
            1 => "Your main screen is picked for you".into(),
            2 if has_area => format!("Sync will use {area_name}"),
            2 => format!("Colours and moods only in {room}"),
            3 => "Change it any time in Settings".into(),
            _ => String::new(),
        };
        let next_label = match step {
            0 => "Start  →".to_string(),
            1 => "Use this screen  →".into(),
            2 => format!("Use {}  →", truncate(&room, 18)),
            3 => "Use this output  →".into(),
            _ => "Open RokSync".into(),
        };
        let mut go_next = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let mut later = false;
        egui::TopBottomPanel::bottom("setup-foot").exact_height(64.0).frame(egui::Frame::none().fill(BG)).show_separator_line(false).show(ctx, |ui| {
            let full = ui.max_rect();
            rule(ui.painter(), full.top(), full.left(), full.right());
            ui.allocate_ui_with_layout(full.size(), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if step > 0 && ui.add(ghost("←  Back").height(64.0).min_size(vec2(140.0, 64.0))).clicked() {
                    self.setup.step -= 1;
                }
                ui.add_space(24.0);
                ui.label(RichText::new(note.to_uppercase()).font(display(15.0)).color(MUTED).extra_letter_spacing(1.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(primary(&next_label).height(64.0).min_size(vec2(300.0, 64.0))).clicked() {
                        go_next = true;
                    }
                    if step < DONE && ui.add(quiet("Set up later").height(64.0).min_size(vec2(160.0, 64.0))).clicked() {
                        later = true;
                    }
                });
            });
        });
        if later {
            self.finish_setup();
            return;
        }
        if go_next {
            if step >= DONE {
                self.finish_setup();
                return;
            }
            self.setup.step = step + 1;
        }

        let frame = egui::Frame::none().fill(BG).inner_margin(Margin { left: PAD_X, right: PAD_X, top: 36.0, bottom: 24.0 });
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            egui::ScrollArea::vertical().id_salt("setup").auto_shrink([false, false]).show(ui, |ui| {
                ui.set_max_width(1180.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                match self.setup.step.min(DONE) {
                    0 => self.setup_bridge(ui, inv),
                    1 => {
                        heading(ui, "Which screen do you play on?", "Screen sync reads this one. Drawn to scale, as Windows sees them.");
                        self.screen_cards(ui);
                    }
                    2 => {
                        heading(ui, "Where do you play?", "RokSync controls this room and leaves the rest of the house alone. Sync runs on an area that sits fully inside it.");
                        let w = ui.available_width();
                        let col = (w - 24.0) / 2.0;
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 24.0;
                            ui.allocate_ui_with_layout(vec2(col, 10.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                caption(ui, "ROOM OR ZONE");
                                self.room_list(ui, inv);
                            });
                            ui.allocate_ui_with_layout(vec2(col, 10.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                let room = self.target(inv).name.to_uppercase();
                                caption(ui, &format!("SYNC AREA IN {room}"));
                                self.area_list(ui, inv);
                            });
                        });
                    }
                    3 => {
                        heading(ui, "What should the lights hear?", "Sound sync and sound hits in games listen to this output. Put some music on: the bars should move.");
                        let w = ui.available_width();
                        let side = 320.0;
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 24.0;
                            ui.allocate_ui_with_layout(vec2(w - side - 24.0, 10.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                self.sound_list(ui);
                            });
                            let bands = self.hearing();
                            let (r, _) = ui.allocate_exact_size(vec2(side, 220.0), Sense::hover());
                            hearing_panel(ui.painter(), r, bands);
                        });
                    }
                    _ => self.setup_done_view(ui, inv),
                }
            });
        });
    }

    fn setup_bridge(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        ui.label(headline("Found your bridge", 64.0));
        ui.add_space(20.0);
        let name = self.cfg.bridge.as_ref().map(|b| b.name.clone()).unwrap_or_else(|| "Hue Bridge".into());
        let (r, _) = ui.allocate_exact_size(vec2(760.0_f32.min(ui.available_width()), 76.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(r, 0.0, SURFACE);
        let g = p.layout_job(spaced("PAIRED", display_bold(16.0), Color32::PLACEHOLDER, 1.2));
        let tr = Rect::from_min_size(r.min, vec2(g.size().x + 36.0, r.height()));
        p.rect_filled(tr, 0.0, GOOD);
        p.galley(tr.center() - g.size() / 2.0, g, INK);
        text_at(p, pos2(tr.right() + 18.0, r.center().y - 11.0), Align2::LEFT_CENTER, &name.to_uppercase(), display_bold(26.0), TEXT, 0.3);
        let facts = format!("{} · {} rooms · {} sync {}", plural(inv.lights.len()), inv.rooms.len(), inv.ent_configs.len(), if inv.ent_configs.len() == 1 { "area" } else { "areas" });
        text_at(p, pos2(tr.right() + 18.0, r.center().y + 15.0), Align2::LEFT_CENTER, &facts, body(14.0), SOFT, 0.0);
        ui.add_space(20.0);
        ui.label(RichText::new("Three quick picks and you're done: which screen to read, which room to light, and what to listen to. Change any of them later in Settings.").font(body(16.0)).color(SOFT));
    }

    fn setup_done_view(&mut self, ui: &mut egui::Ui, inv: &Inventory) {
        ui.label(headline("You're set", 64.0));
        ui.add_space(20.0);
        let (res, _) = self.screen_summary();
        let room = self.target(inv).name;
        let (area, _) = self.area_summary(inv);
        let sound = self.sound_summary();
        let rows = [("SCREEN", res, 1), ("ROOM", room, 2), ("SYNC AREA", area, 2), ("SOUND", sound, 3)];
        let w = 820.0_f32.min(ui.available_width());
        for (k, v, go) in rows {
            let (r, _) = ui.allocate_exact_size(vec2(w, 60.0), Sense::hover());
            row_rule(ui.painter(), r.top(), r.left(), r.right());
            text_at(ui.painter(), pos2(r.left(), r.center().y), Align2::LEFT_CENTER, k, display(14.0), MUTED, 1.2);
            text_at(ui.painter(), pos2(r.left() + 180.0, r.center().y), Align2::LEFT_CENTER, &truncate(&v, 48).to_uppercase(), display_bold(20.0), TEXT, 0.3);
            let br = Rect::from_min_size(pos2(r.right() - 120.0, r.center().y - HIT / 2.0), vec2(120.0, HIT));
            if ui.put(br, ghost("Change").height(HIT)).clicked() {
                self.setup.step = go;
            }
        }
        ui.add_space(16.0);
        ui.label(RichText::new("All of this lives in Settings under Setup.").font(body(14.0)).color(MUTED));
    }
}

fn heading(ui: &mut egui::Ui, title: &str, sub: &str) {
    ui.label(headline(title, 64.0));
    ui.add_space(8.0);
    ui.label(RichText::new(sub).font(body(16.0)).color(SOFT));
    ui.add_space(24.0);
}

fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(label_caps(text, 14.0, MUTED));
    ui.add_space(8.0);
}

fn hearing_panel(p: &egui::Painter, r: Rect, bands: [f32; 3]) {
    p.rect_filled(r, 0.0, SURFACE);
    text_at(p, pos2(r.left() + 18.0, r.top() + 24.0), Align2::LEFT_CENTER, "HEARING NOW", display(14.0), MUTED, 1.2);
    let area = Rect::from_min_max(pos2(r.left() + 18.0, r.top() + 48.0), pos2(r.right() - 18.0, r.bottom() - 40.0));
    let bw = (area.width() - 12.0) / 3.0;
    for (i, ((v, c), name)) in bands.iter().zip([ACCENT, WARN, ACCENT2]).zip(["BASS", "MIDS", "HIGHS"]).enumerate() {
        let col = Rect::from_min_size(pos2(area.left() + i as f32 * (bw + 6.0), area.top()), vec2(bw, area.height()));
        p.rect_filled(col, 0.0, SURFACE3);
        let h = col.height() * v;
        p.rect_filled(Rect::from_min_max(pos2(col.left(), col.bottom() - h), col.max), 0.0, c);
        text_at(p, pos2(col.left(), r.bottom() - 20.0), Align2::LEFT_CENTER, name, display(13.0), MUTED, 1.0);
    }
}

fn plural(n: usize) -> String {
    if n == 1 {
        "1 light".into()
    } else {
        format!("{n} lights")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(name: &str, left: i32, w: i32, h: i32) -> MonitorInfo {
        MonitorInfo { adapter: 0, output: 0, name: name.into(), left, top: 0, width: w, height: h }
    }

    #[test]
    fn a_small_extra_display_reads_as_audio() {
        let main = mon(r"\\.\DISPLAY2", 0, 3840, 2160);
        let bar = mon(r"\\.\DISPLAY1", -1280, 1280, 720);
        let side = mon(r"\\.\DISPLAY3", 3840, 2560, 1440);
        let p = Some(main.name.as_str());
        assert!(!looks_like_audio(&main, p));
        assert!(looks_like_audio(&bar, p));
        assert!(!looks_like_audio(&side, p));
    }

    #[test]
    fn meters_read_in_decibels() {
        assert_eq!(db_frac(0.0), 0.0);
        assert!((db_frac(1.0) - 1.0).abs() < 1e-6);
        assert!(db_frac(0.1) > 0.6 && db_frac(0.1) < 0.65);
    }
}
