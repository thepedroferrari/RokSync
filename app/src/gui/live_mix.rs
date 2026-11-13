//! Live: mode cards and the layers under them (colour source, sound, game events).

use super::icons::Icon;
use super::theme::*;
use super::LumenApp;
use crate::config::{Mix, SoundStyle};
use crate::mix::LiveMode;
use crate::sync::Source;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Rect, Sense, Stroke, UiBuilder};
use lumen_engine::Look;
use lumen_hue::Rgb;

const LEFT_W: f32 = 230.0;
const SWITCH_W: f32 = 72.0;

impl LumenApp {
    pub fn mix(&self) -> Mix {
        Mix { source: self.sy.source, sound: self.cfg.sound.on, game: !self.cfg.fx_paused }
    }

    pub fn set_mix(&mut self, m: Mix) {
        if m == self.mix() {
            return;
        }
        self.sy.source = m.source;
        self.cfg.source = m.source;
        self.cfg.sound.on = m.sound;
        self.cfg.fx_paused = !m.game;
        if let Some(appid) = self.games.detected.as_ref().map(|d| d.appid) {
            if let Some(r) = self.cfg.game_rules.get_mut(&appid) {
                if r.mix.is_some() {
                    r.mix = Some(m);
                }
            }
        }
        self.save_cfg();
    }

    pub fn mode_cards(&mut self, ui: &mut egui::Ui) {
        let cur = LiveMode::of(self.mix()).map(|(l, _)| l);
        let gap = 10.0;
        let w = (ui.available_width() - gap * 3.0) / 4.0;
        let h = 128.0;
        let mut pick = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for l in LiveMode::ALL {
                let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
                let sel = cur == Some(l);
                let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
                let p = ui.painter();
                p.rect_filled(r, 0.0, mix(SURFACE, SURFACE2, hov));
                p.rect_filled(Rect::from_min_size(r.min, vec2(r.width(), 4.0)), 0.0, if sel { ACCENT } else { SURFACE3 });
                if sel {
                    p.rect_stroke(r.shrink(1.0), 0.0, Stroke::new(2.0_f32, ACCENT));
                }
                let x = r.left() + 16.0;
                text_at(p, pos2(x, r.top() + 30.0), Align2::LEFT_CENTER, &l.name().to_uppercase(), display_bold(26.0), TEXT, 0.6);
                let blurb = egui::text::LayoutJob::simple(l.blurb().into(), body(13.0), MUTED, r.width() - 32.0);
                p.galley(pos2(x, r.top() + 48.0), p.layout_job(blurb), MUTED);
                let foot = r.bottom() - 28.0;
                row_rule(p, foot, r.left() + 1.0, r.right() - 1.0);
                text_at(p, pos2(x, foot + 14.0), Align2::LEFT_CENTER, l.recipe(), display(13.0), if sel { TEXT } else { SOFT }, 1.0);
                if resp.has_focus() {
                    focus_ring(p, r);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    pick = Some(l);
                }
            }
        });
        if let Some(l) = pick {
            self.set_mix(l.mix());
        }
    }

    pub fn layers(&mut self, ui: &mut egui::Ui) {
        let w = ui.available_width();
        let m = self.mix();
        ui.spacing_mut().item_spacing.y = 0.0;

        // colour source
        let (r, _) = ui.allocate_exact_size(vec2(w, 76.0), Sense::hover());
        let mid = left_label(ui, r, "COLOUR FROM", "One source sets the colours");
        let seg = Rect::from_min_size(pos2(mid.left(), r.center().y - HIT / 2.0), vec2(220.0, HIT));
        let mut src = m.source;
        let changed = ui.allocate_new_ui(UiBuilder::new().max_rect(seg), |ui| segmented_h(ui, &mut src, &[(Source::Look, "Mood"), (Source::Screen, "Screen")], HIT, true)).inner;
        if changed {
            self.set_mix(Mix { source: src, ..m });
        }
        let meta = Rect::from_min_max(pos2(seg.right() + 20.0, r.top()), pos2(r.right(), r.bottom()));
        if m.source == Source::Screen {
            self.tuning_button(ui, meta);
        } else {
            let cols = look_colors(&self.sy.program().look);
            let sw = Rect::from_min_size(pos2(meta.left(), r.center().y - 11.0), vec2(150.0, 22.0));
            paint_gradient(ui.painter(), sw, &cols, 0.0);
            let br = Rect::from_min_size(pos2(sw.right() + 16.0, r.center().y - HIT / 2.0), vec2(150.0, HIT));
            if ui.put(br, ghost("Pick mood").height(HIT)).clicked() {
                self.page = super::Page::Palettes;
            }
        }

        // sound
        let (r, _) = ui.allocate_exact_size(vec2(w, 96.0), Sense::hover());
        let (on_air, dev_name, status) = self.sound.state();
        let device = match &self.cfg.sound.device {
            Some(d) => d.clone(),
            None if on_air && !dev_name.is_empty() => format!("Windows output · {dev_name}"),
            None => "Whatever Windows plays to".into(),
        };
        let mid = left_label(ui, r, "SOUND", &device);
        let desc = if m.source == Source::Look {
            "The mood moves to the music. Pick a style below."
        } else {
            "Bass and loud hits push the lights brighter. Explosions land in the room."
        };
        let silent = m.sound && !on_air && !status.is_empty() && status != "listening" && status != "stopped";
        let (line, col) = if silent { (format!("Can't hear it: {status}"), WARN) } else { (desc.to_string(), SOFT) };
        text_at(ui.painter(), pos2(mid.left(), r.top() + 26.0), Align2::LEFT_CENTER, &truncate(&line, 90), body(14.0), col, 0.0);
        let (amt_label, amt) = if m.source == Source::Look { ("REACTION", self.cfg.sound.reaction) } else { ("PUNCH", self.cfg.sound.punch) };
        if let Some(v) = amount_row(ui, mid, r.top() + 62.0, amt_label, amt, m.sound, ui.id().with("sound-amt")) {
            if m.source == Source::Look {
                self.cfg.sound.reaction = v;
            } else {
                self.cfg.sound.punch = v;
            }
            self.save_cfg();
        }
        if m.sound {
            let env = self.sy.reactor.env;
            let br = Rect::from_min_size(pos2(mid.right() - 86.0, r.top() + 14.0), vec2(86.0, 26.0));
            for (i, (e, c)) in env.iter().zip([ACCENT, WARN, ACCENT2]).enumerate() {
                let bw = (br.width() - 8.0) / 3.0;
                let col = Rect::from_min_size(pos2(br.left() + i as f32 * (bw + 4.0), br.top()), vec2(bw, br.height()));
                ui.painter().rect_filled(col, 0.0, SURFACE3);
                let hgt = col.height() * e.clamp(0.0, 1.0);
                ui.painter().rect_filled(Rect::from_min_max(pos2(col.left(), col.bottom() - hgt), col.max), 0.0, c);
            }
        }
        if layer_switch(ui, r, "sound-on", m.sound) {
            self.set_mix(Mix { sound: !m.sound, ..m });
        }

        if m.sound && m.source == Source::Look {
            let (r, _) = ui.allocate_exact_size(vec2(w, 72.0), Sense::hover());
            let mid = left_label(ui, r, "SOUND STYLE", "How the colours move");
            let seg = Rect::from_min_size(pos2(mid.left(), r.center().y - HIT / 2.0), vec2(360.0, HIT));
            let mut st = self.cfg.sound.style;
            let changed = ui
                .allocate_new_ui(UiBuilder::new().max_rect(seg), |ui| {
                    segmented_h(ui, &mut st, &[(SoundStyle::Pulse, "Pulse"), (SoundStyle::Spectrum, "Spectrum"), (SoundStyle::Beat, "Beat")], HIT, true)
                })
                .inner;
            let hint = match st {
                SoundStyle::Pulse => "The room breathes with the bass",
                SoundStyle::Spectrum => "Bass on the left lights, highs on the right",
                SoundStyle::Beat => "Every light jumps to the next colour on each beat",
            };
            text_at(ui.painter(), pos2(seg.right() + 20.0, r.center().y), Align2::LEFT_CENTER, hint, body(13.5), MUTED, 0.0);
            if changed {
                self.cfg.sound.style = st;
                self.save_cfg();
            }
        }

        // game events
        let (r, _) = ui.allocate_exact_size(vec2(w, 96.0), Sense::hover());
        let linked: Vec<&str> = [(self.cfg.cs2.enabled, "CS2"), (self.cfg.dota.enabled, "Dota 2"), (self.cfg.cod.enabled, "Call of Duty")]
            .into_iter()
            .filter_map(|(on, n)| on.then_some(n))
            .collect();
        let sub = if linked.is_empty() { "No game linked yet".to_string() } else { linked.join(" · ") };
        let mid = left_label(ui, r, "GAME EVENTS", &sub);
        let desc = if linked.is_empty() { "Link CS2, Dota 2 or Call of Duty on the Games page." } else { "Flashbangs, bomb, kills and low health, straight from the game." };
        text_at(ui.painter(), pos2(mid.left(), r.top() + 26.0), Align2::LEFT_CENTER, desc, body(14.0), SOFT, 0.0);
        if let Some(v) = amount_row(ui, mid, r.top() + 62.0, "STRENGTH", self.cfg.fx_gain, m.game, ui.id().with("fx-amt")) {
            self.cfg.fx_gain = v;
            self.save_cfg();
        }
        if layer_switch(ui, r, "game-on", m.game) {
            self.set_mix(Mix { game: !m.game, ..m });
        }

        // remember for the running game
        if let Some(d) = self.games.detected.clone() {
            let (r, _) = ui.allocate_exact_size(vec2(w, 64.0), Sense::hover());
            row_rule(ui.painter(), r.top(), r.left(), r.right());
            let mut rules = self.rules_for(d.appid);
            let saved = rules.mix.is_some();
            let p = ui.painter();
            text_at(p, pos2(r.left(), r.center().y - 10.0), Align2::LEFT_CENTER, &format!("Use this mix every time {} starts", d.name), semibold(15.0), TEXT, 0.0);
            let sub = if saved { format!("Saved: {}", crate::mix::describe(m)) } else { format!("{} uses Game mode", d.name) };
            text_at(p, pos2(r.left(), r.center().y + 12.0), Align2::LEFT_CENTER, &sub, body(13.0), MUTED, 0.0);
            if layer_switch(ui, r, "remember", saved) {
                rules.mix = if saved { None } else { Some(m) };
                self.cfg.game_rules.insert(d.appid, rules);
                self.save_cfg();
            }
        }
        let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
        row_rule(ui.painter(), r.top(), r.left(), r.right());
        ui.spacing_mut().item_spacing.y = 8.0;
    }

    /// Screen colours: which game mode tunes the reading, and which monitor.
    fn tuning_button(&mut self, ui: &mut egui::Ui, meta: Rect) {
        let label = match self.cfg.game_mode.as_deref().and_then(crate::modes::find) {
            Some(m) if m.matches(&self.sy.screen) => format!("Tuned for {}", m.name),
            Some(m) => format!("Tuned for {} (changed)", m.name),
            None => "Custom tuning".into(),
        };
        let br = Rect::from_min_size(pos2(meta.left(), meta.center().y - HIT / 2.0), vec2(280.0, HIT));
        let resp = ui.put(br, ghost(&label).icon(Icon::Games).height(HIT));
        let id = ui.make_persistent_id("tuning-popup");
        if resp.clicked() {
            ui.memory_mut(|m| m.toggle_popup(id));
        }
        let mut pick = None;
        egui::popup_below_widget(ui, id, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(280.0);
            for m in crate::modes::MODES {
                let sel = self.cfg.game_mode.as_deref() == Some(m.id);
                if ui.add(ghost(m.name).selected(sel).tint(Color32::from_rgb(m.art[0][0], m.art[0][1], m.art[0][2])).min_size(vec2(270.0, 30.0))).on_hover_text(m.blurb).clicked() {
                    pick = Some(m.id);
                }
            }
        });
        if let Some(id) = pick {
            self.apply_mode(id);
        }
        let mon = self.screen.lock().unwrap().monitor.as_ref().map(|m| m.label()).unwrap_or_else(|| "Main screen".into());
        text_at(ui.painter(), pos2(br.right() + 16.0, meta.center().y), Align2::LEFT_CENTER, &mon, body(13.5), MUTED, 0.0);
    }
}

/// Title and subtitle in the left column with a rule on top; returns the middle column.
fn left_label(ui: &egui::Ui, r: Rect, title: &str, sub: &str) -> Rect {
    let p = ui.painter();
    row_rule(p, r.top(), r.left(), r.right());
    text_at(p, pos2(r.left(), r.center().y - 11.0), Align2::LEFT_CENTER, title, display_bold(22.0), TEXT, 0.6);
    text_at(p, pos2(r.left(), r.center().y + 13.0), Align2::LEFT_CENTER, &truncate(sub, 34), body(13.0), MUTED, 0.0);
    Rect::from_min_max(pos2(r.left() + LEFT_W, r.top()), pos2(r.right() - SWITCH_W - 24.0, r.bottom()))
}

/// ON/OFF switch at the right edge of a row; true when clicked.
fn layer_switch(ui: &mut egui::Ui, r: Rect, id: &str, on: bool) -> bool {
    let sw = Rect::from_min_size(pos2(r.right() - SWITCH_W, r.center().y - HIT / 2.0), vec2(SWITCH_W, HIT));
    onoff_at(ui, sw, ui.id().with(("layer", id)), on).clicked()
}

/// LABEL ──────●── 40 %; returns a new value while dragged.
fn amount_row(ui: &mut egui::Ui, mid: Rect, y: f32, label: &str, v: f32, enabled: bool, id: egui::Id) -> Option<f32> {
    let p = ui.painter();
    let ink = if enabled { MUTED } else { FAINT };
    text_at(p, pos2(mid.left(), y), Align2::LEFT_CENTER, label, display(13.0), ink, 1.2);
    let track = Rect::from_min_max(pos2(mid.left() + 100.0, y - HIT / 2.0), pos2(mid.right() - 70.0, y + HIT / 2.0));
    let (_, out) = bri_slider_at(ui, track, id, v, enabled);
    let shown = out.unwrap_or(v);
    num_unit(ui.painter(), pos2(mid.right(), y), Align2::RIGHT_CENTER, &format!("{:.0}", shown * 100.0), "%", 20.0, if enabled { TEXT } else { FAINT });
    out.map(|x| (x * 20.0).round() / 20.0).filter(|x| (x - v).abs() > 1e-4)
}

pub fn look_colors(look: &Look) -> Vec<Rgb> {
    match look {
        Look::Solid(c) => vec![*c],
        Look::Gradient { stops, .. } => stops.clone(),
        Look::PerChannel(m) => m.values().copied().collect(),
        Look::Palette(p) => p.clone(),
    }
}
