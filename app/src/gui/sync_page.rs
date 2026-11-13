use super::lights::{picker, preset_row, stops_editor};
use super::theme::*;
use super::LumenApp;
use crate::config::{AppConfig, SavedProgram};
use crate::sync::{base_frame, channel_positions, Source};
use super::icons::Icon;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2};
use lumen_engine::{Axis, Look, Motion, Program};
use lumen_hue::{Inventory, Rgb};
use lumen_screen::{MonitorInfo, NRect, SampleMode, ScreenSettings};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(PartialEq, Clone, Copy)]
pub enum LookKind {
    Solid,
    Gradient,
    PerChannel,
    Palette,
}

#[derive(PartialEq, Clone, Copy)]
pub enum MotionKind {
    Static,
    Breathe,
    Drift,
    Flicker,
}

pub struct SyncUi {
    pub area: Option<String>,
    pub source: Source,
    pub look: LookKind,
    pub solid: [u8; 3],
    pub stops: Vec<[u8; 3]>,
    pub axis: Axis,
    pub per_channel: BTreeMap<u8, [u8; 3]>,
    pub palette: Vec<[u8; 3]>,
    pub motion: MotionKind,
    pub period: f32,
    pub depth: f32,
    pub spread: f32,
    pub drift: f32,
    pub flicker: f32,
    pub flicker_speed: f32,
    pub brightness: f32,
    save_name: String,
    pub screen: ScreenSettings,
    monitors: Vec<MonitorInfo>,
    windows: Vec<lumen_screen::windows_list::WindowInfo>,
    follow: Option<(isize, String)>,
    tex: Option<egui::TextureHandle>,
    tex_seq: u64,
    drag_from: Option<egui::Pos2>,
    /// Sound response for the preview and the meters; the stream runs its own.
    pub reactor: crate::sound::Reactor,
    tick: Instant,
    lay: Option<(String, crate::sound::Layout)>,
    scratch: Vec<Rgb>,
}

impl SyncUi {
    pub fn new(cfg: &AppConfig) -> Self {
        Self {
            area: cfg.last_area.clone(),
            source: cfg.source,
            look: LookKind::Gradient,
            solid: [255, 120, 40],
            stops: vec![[255, 0, 160], [0, 200, 255]],
            axis: Axis::LeftRight,
            per_channel: BTreeMap::new(),
            palette: vec![[255, 60, 0], [255, 0, 120], [120, 0, 255]],
            motion: MotionKind::Static,
            period: 6.0,
            depth: 0.7,
            spread: 0.3,
            drift: 20.0,
            flicker: 0.35,
            flicker_speed: 6.0,
            brightness: 1.0,
            save_name: String::new(),
            screen: cfg.screen.settings.clone(),
            monitors: lumen_screen::capture::monitors().unwrap_or_default(),
            windows: Vec::new(),
            follow: None,
            tex: None,
            tex_seq: 0,
            drag_from: None,
            reactor: Default::default(),
            tick: Instant::now(),
            lay: None,
            scratch: Vec::new(),
        }
    }

    pub fn program(&self) -> Program {
        let look = match self.look {
            LookKind::Solid => Look::Solid(Rgb::from_u8(self.solid)),
            LookKind::Gradient => Look::Gradient { stops: self.stops.iter().map(|c| Rgb::from_u8(*c)).collect(), axis: self.axis },
            LookKind::PerChannel => Look::PerChannel(self.per_channel.iter().map(|(k, v)| (*k, Rgb::from_u8(*v))).collect()),
            LookKind::Palette => Look::Palette(self.palette.iter().map(|c| Rgb::from_u8(*c)).collect()),
        };
        let motion = match self.motion {
            MotionKind::Static => Motion::Static,
            MotionKind::Breathe => Motion::Breathe { period_s: self.period, depth: self.depth, spread: self.spread },
            MotionKind::Drift => Motion::Drift { period_s: self.drift },
            MotionKind::Flicker => Motion::Flicker { amount: self.flicker, speed: self.flicker_speed },
        };
        Program { look, motion, brightness: self.brightness }
    }

    pub fn load(&mut self, p: &Program) {
        match &p.look {
            Look::Solid(c) => {
                self.look = LookKind::Solid;
                self.solid = c.to_u8();
            }
            Look::Gradient { stops, axis } => {
                self.look = LookKind::Gradient;
                self.stops = stops.iter().map(|c| c.to_u8()).collect();
                self.axis = *axis;
            }
            Look::PerChannel(m) => {
                self.look = LookKind::PerChannel;
                self.per_channel = m.iter().map(|(k, v)| (*k, v.to_u8())).collect();
            }
            Look::Palette(c) => {
                self.look = LookKind::Palette;
                self.palette = c.iter().map(|c| c.to_u8()).collect();
            }
        }
        match p.motion {
            Motion::Static => self.motion = MotionKind::Static,
            Motion::Breathe { period_s, depth, spread } => {
                self.motion = MotionKind::Breathe;
                self.period = period_s;
                self.depth = depth;
                self.spread = spread;
            }
            Motion::Drift { period_s } => {
                self.motion = MotionKind::Drift;
                self.drift = period_s;
            }
            Motion::Flicker { amount, speed } => {
                self.motion = MotionKind::Flicker;
                self.flicker = amount;
                self.flicker_speed = speed;
            }
        }
        self.brightness = p.brightness;
    }
}

impl SyncUi {
    pub fn follow_window(&mut self, hwnd: isize, title: String) {
        self.follow = Some((hwnd, title));
        self.source = Source::Screen;
    }
}

impl LumenApp {
    pub fn sync_page(&mut self, ctx: &egui::Context, inv: &Inventory, frame: egui::Frame, ease: f32) {
        // the entertainment area must sit wholly inside the target
        let area = self.sync_area(inv).cloned();
        if let Some(a) = &area {
            self.sy.area = Some(a.id.clone());
        }
        if let Some(a) = &area {
            let chans = channel_positions(a);
            let mut s = self.screen.lock().unwrap();
            if s.chans.len() != chans.len() {
                s.chans = chans;
            }
        }

        let program = std::sync::Arc::new(self.sy.program());
        {
            let mut sh = self.sync_shared.lock().unwrap();
            if sh.program != program {
                sh.program = program.clone();
            }
            sh.source = self.sy.source;
        }
        {
            let mut s = self.screen.lock().unwrap();
            s.settings = self.sy.screen.clone();
            s.follow_window = self.sy.follow.as_ref().map(|f| f.0);
        }
        let now = Instant::now();
        let dt = (now - self.sy.tick).as_secs_f32();
        self.sy.tick = now;
        if self.cfg.sound.on {
            self.sound.want(0.6);
            let lv = self.sound.levels();
            self.sy.reactor.update(&lv, now, dt);
        }

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            ui.set_opacity(0.15 + 0.85 * ease);
            page_title(ui, "03", "Live", "Pick a mode, then fine-tune what the lights follow.");
            let stage_h = (ui.available_height() * 0.26).clamp(150.0, 260.0);
            let (stage, _) = ui.allocate_exact_size(vec2(ui.available_width(), stage_h), Sense::hover());
            self.stage(ui, stage, &area, &program);
            ui.add_space(12.0);
            self.command_strip(ui, inv, &area);
            ui.add_space(12.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                self.mode_cards(ui);
                ui.add_space(14.0);
                self.layers(ui);
                ui.add_space(18.0);
                let two = ui.available_width() > 1000.0;
                match self.sy.source {
                    Source::Look => {
                        if two {
                            ui.columns(2, |c| {
                                self.look_card(&mut c[0], inv, &area);
                                self.motion_card(&mut c[1]);
                            });
                        } else {
                            self.look_card(ui, inv, &area);
                            ui.add_space(12.0);
                            self.motion_card(ui);
                        }
                    }
                    Source::Screen => {
                        if two {
                            ui.columns(2, |c| {
                                self.screen_area_card(&mut c[0]);
                                self.picture_card(&mut c[1]);
                            });
                        } else {
                            self.screen_area_card(ui);
                            ui.add_space(12.0);
                            self.picture_card(ui);
                        }
                    }
                }
                if self.sy.source == Source::Look {
                    ui.add_space(12.0);
                    self.saved_card(ui);
                }
                ui.add_space(16.0);
            });
        });
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }

    fn stage(&mut self, ui: &mut egui::Ui, rect: Rect, area: &Option<lumen_hue::EntConfig>, program: &Program) {
        let (running, connecting, status) = {
            let s = self.sync_shared.lock().unwrap();
            (s.running, s.connecting, s.status.clone())
        };
        let p = ui.painter_at(rect);
        p.add(chamfer_shape(rect, 16.0, Color32::from_rgba_premultiplied(3, 4, 7, 245), Stroke::NONE));
        let t = anim_time(ui);
        let frame: Vec<(u8, Rgb)> = match area {
            Some(a) if running => {
                let _ = a;
                self.sync_shared.lock().unwrap().last_frame.clone()
            }
            Some(a) => {
                let chans = channel_positions(a);
                let mut f = base_frame(program, self.sy.source, &chans, self.started.elapsed().as_secs_f32(), &self.screen);
                if self.cfg.sound.on {
                    if self.sy.lay.as_ref().is_none_or(|(id, l)| *id != a.id || l.u.len() != chans.len()) {
                        self.sy.lay = Some((a.id.clone(), crate::sound::Layout::new(&chans)));
                    }
                    if let Some((_, lay)) = &self.sy.lay {
                        let layer = self.cfg.sound.layer(self.sy.source);
                        crate::sound::apply(&mut f, lay, &self.sy.reactor, layer, self.sy.source, &mut self.sy.scratch);
                    }
                }
                let on = !self.cfg.fx_paused;
                crate::sync::apply_game_fx(
                    &mut f,
                    &self.fx_sources(),
                    Instant::now(),
                    self.cfg.fx_gain,
                    on && self.cfg.cs2.enabled,
                    on && self.cfg.dota.enabled,
                    on && self.cfg.cod.enabled,
                    program.brightness,
                );
                f
            }
            None => Vec::new(),
        };

        let horizon = rect.top() + rect.height() * 0.36;
        p.rect_filled(Rect::from_min_max(pos2(rect.left(), horizon), rect.max), 0.0, SURFACE2);
        rule(&p, horizon, rect.left(), rect.right());

        let tw = (rect.width() * 0.3).min((horizon - rect.top() - 24.0) * 16.0 / 9.0 * 1.25);
        let th = tw * 9.0 / 16.0;
        let tv = Rect::from_center_size(pos2(rect.center().x, horizon - th / 2.0 - 6.0), vec2(tw, th));
        let avg = {
            let n = frame.len().max(1) as f32;
            let a = frame.iter().fold(Rgb::BLACK, |a, (_, c)| Rgb::new(a.r + c.r / n, a.g + c.g / n, a.b + c.b / n));
            c32(a)
        };
        bloom(&p, tv.center(), tw * 1.1, avg, 0.35, 0.7);
        p.rect_filled(tv.expand(3.0), 2.0, Color32::from_rgb(8, 9, 12));
        let showed_screen = if self.sy.source == Source::Screen {
            if let Some(tex) = &self.sy.tex {
                p.image(tex.id(), tv, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                true
            } else {
                false
            }
        } else {
            false
        };
        if !showed_screen {
            let mut cols: Vec<(f32, Rgb)> = match area {
                Some(a) => a.channels.iter().filter_map(|ch| frame.iter().find(|(id, _)| *id == ch.channel_id).map(|(_, c)| (ch.position.x as f32, *c))).collect(),
                None => Vec::new(),
            };
            cols.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let cs: Vec<Rgb> = cols.into_iter().map(|(_, c)| c.scale(0.55)).collect();
            paint_gradient(&p, tv, &cs, 0.0);
        }
        p.rect_stroke(tv.expand(3.0), 2.0, Stroke::new(1.0_f32, EDGE));
        text_at(&p, tv.center_bottom() + vec2(0.0, 14.0), Align2::CENTER_CENTER, if self.sy.source == Source::Screen { "SCREEN" } else { "MOOD" }, mono(9.5), FAINT, 1.5);

        // lights: map Hue's room coordinates onto the floor in perspective
        if let Some(a) = area {
            let mut items: Vec<(f32, Pos2, f32, Rgb, u8)> = a
                .channels
                .iter()
                .map(|ch| {
                    let col = frame.iter().find(|(id, _)| *id == ch.channel_id).map(|(_, c)| *c).unwrap_or(Rgb::BLACK);
                    let depth = 1.0 - (ch.position.y as f32 + 1.0) / 2.0; // 0 = screen side, 1 = viewer side
                    let s = 0.55 + 0.45 * depth;
                    let fy = egui::lerp((horizon + 18.0)..=(rect.bottom() - 30.0), 0.1 + 0.85 * depth);
                    let x = rect.center().x + ch.position.x as f32 * rect.width() * 0.42 * s;
                    let lift = (ch.position.z as f32).max(-1.0) * 26.0 * s;
                    (depth, pos2(x, fy - lift), s, col, ch.channel_id)
                })
                .collect();
            items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            for (_, pos, s, col, _) in &items {
                let floor = pos2(pos.x, pos.y + 18.0 * s);
                bloom(&p, floor, rect.width() * 0.2 * s, c32(*col), 0.7 * col.max_channel().min(1.0), 0.32);
            }
            for (_, pos, s, col, id) in &items {
                glow(&p, *pos, 7.0 + 6.0 * s, *col, true);
                text_at(&p, *pos + vec2(0.0, 18.0 + 8.0 * s), Align2::CENTER_CENTER, &format!("{id:02}"), mono(9.5), FAINT, 0.5);
            }
        } else {
            text_at(&p, rect.center(), Align2::CENTER_CENTER, "NO ENTERTAINMENT AREA", display(16.0), MUTED, 2.0);
            text_at(&p, rect.center() + vec2(0.0, 22.0), Align2::CENTER_CENTER, "Create one in the Hue app under Settings → Entertainment areas", FontId::proportional(12.5), FAINT, 0.0);
        }

        let edge = if running { GOOD } else if connecting { WARN } else { BORDER };
        p.add(Shape::closed_line(chamfer(rect, 16.0), Stroke::new(1.0_f32, edge)));
        if running {
            brackets(&p, rect.shrink(8.0), GOOD.gamma_multiply(0.8), 22.0, 2.0);
        }
        let tl = rect.left_top() + vec2(22.0, 22.0);
        let (badge, col) = if running {
            ("● LIVE", GOOD)
        } else if connecting {
            ("◌ LINKING", WARN)
        } else {
            ("PREVIEW", MUTED)
        };
        let pulse = if running { 0.65 + 0.35 * (t * 3.5).sin().abs() } else { 1.0 };
        let br = text_at(&p, tl, Align2::LEFT_CENTER, badge, display_bold(15.0), col.gamma_multiply(pulse), 2.0);
        if running {
            text_at(&p, br.right_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, &format!("{} Hz", crate::perf::profile().stream_hz), mono(12.0), MUTED, 0.3);
        }
        if !status.is_empty() {
            let bad = status.starts_with("Couldn't") || status.starts_with("Stream lost");
            text_at(&p, tl + vec2(0.0, 20.0), Align2::LEFT_CENTER, &truncate(&status, 70), mono(11.0), if bad { BAD } else { FAINT }, 0.2);
        }
        if let Some(a) = area {
            text_at(&p, rect.right_top() + vec2(-22.0, 22.0), Align2::RIGHT_CENTER, &a.metadata.name.to_uppercase(), display(13.0), TEXT, 1.6);
            text_at(&p, rect.right_top() + vec2(-22.0, 40.0), Align2::RIGHT_CENTER, &format!("{} CHANNELS", a.channels.len()), mono(10.5), FAINT, 0.6);
        }
        if self.sy.source == Source::Screen {
            let (fps, st) = {
                let s = self.screen.lock().unwrap();
                (s.fps, s.status.clone())
            };
            text_at(&p, rect.left_bottom() + vec2(22.0, -18.0), Align2::LEFT_CENTER, &format!("CAPTURE {fps:>3.0} FPS · {}", st.to_uppercase()), mono(10.5), FAINT, 0.3);
        }
        if let Some((_, title)) = &self.sy.follow {
            text_at(&p, rect.right_bottom() + vec2(-22.0, -18.0), Align2::RIGHT_CENTER, &format!("FOLLOWING {}", truncate(title, 34).to_uppercase()), mono(10.5), ACCENT2, 0.3);
        }
    }

    fn command_strip(&mut self, ui: &mut egui::Ui, inv: &Inventory, area: &Option<lumen_hue::EntConfig>) {
        let (running, connecting) = {
            let s = self.sync_shared.lock().unwrap();
            (s.running, s.connecting)
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let m = self.mix();
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(crate::mix::LiveMode::label(m)).font(display_bold(26.0)).color(TEXT).extra_letter_spacing(0.6));
                ui.label(RichText::new(crate::mix::describe(m)).font(body(13.5)).color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let big = vec2(190.0, 44.0);
                if running {
                    if ui.add(danger("Stop").icon(Icon::Stop).min_size(big)).on_hover_text("F").clicked() {
                        self.stop_live();
                    }
                } else if connecting {
                    ui.add_enabled(false, ghost("Linking…").min_size(big));
                } else if ui.add_enabled(area.is_some(), primary("Go live").icon(Icon::Play).min_size(big)).on_hover_text("F. Close Philips Hue Sync first. Only one app can stream at a time.").clicked() {
                    self.go_live(inv);
                }
                let t = self.target(inv);
                let line = super::target::fit_line(&super::target::fit(inv, &t, self.sy.area.as_deref()));
                ui.label(RichText::new(line).color(if area.is_some() { SOFT } else { WARN }));
            });
        });
    }

    fn look_card(&mut self, ui: &mut egui::Ui, inv: &Inventory, area: &Option<lumen_hue::EntConfig>) {
        card(ui, |ui| {
            section(ui, "Look");
            segmented(
                ui,
                &mut self.sy.look,
                &[(LookKind::Solid, "Colour"), (LookKind::Gradient, "Gradient"), (LookKind::PerChannel, "Per light"), (LookKind::Palette, "Palette")],
            );
            ui.add_space(8.0);
            match self.sy.look {
                LookKind::Solid => {
                    ui.horizontal(|ui| {
                        picker(ui, &mut self.sy.solid);
                        ui.label(RichText::new(Rgb::from_u8(self.sy.solid).to_hex().to_uppercase()).font(mono(12.5)).color(MUTED));
                    });
                    if let Some(p) = preset_row(ui) {
                        self.sy.solid = p;
                    }
                }
                LookKind::Gradient => {
                    stops_editor(ui, &mut self.sy.stops, 8);
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(label_caps("Direction", 11.5, MUTED));
                        segmented(ui, &mut self.sy.axis, &[(Axis::LeftRight, "Left → right"), (Axis::FrontBack, "Front → back"), (Axis::BottomTop, "Low → high")]);
                    });
                }
                LookKind::PerChannel => {
                    if let Some(a) = area {
                        for ch in &a.channels {
                            ui.horizontal(|ui| {
                                let c = self.sy.per_channel.entry(ch.channel_id).or_insert([255, 140, 60]);
                                picker(ui, c);
                                let names: Vec<String> = inv.channel_lights(ch).iter().map(|(l, i)| format!("{} #{}", l.metadata.name.trim(), i)).collect();
                                ui.label(RichText::new(format!("{:02}", ch.channel_id)).font(mono(12.0)).color(ACCENT));
                                ui.label(names.join(", "));
                            });
                        }
                    }
                }
                LookKind::Palette => {
                    stops_editor(ui, &mut self.sy.palette, 12);
                    egui::ComboBox::from_id_salt("from_scene").selected_text("Import from a scene…").show_ui(ui, |ui| {
                        for s in &inv.scenes {
                            let cols = s.colors();
                            if !cols.is_empty() && ui.selectable_label(false, s.metadata.name.trim()).clicked() {
                                self.sy.palette = cols.iter().map(|c| c.to_u8()).collect();
                            }
                        }
                    });
                }
            }
        });
    }

    fn motion_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            section(ui, "Motion");
            segmented(
                ui,
                &mut self.sy.motion,
                &[(MotionKind::Static, "Still"), (MotionKind::Breathe, "Breathe"), (MotionKind::Drift, "Drift"), (MotionKind::Flicker, "Flicker")],
            );
            ui.add_space(8.0);
            ui.spacing_mut().slider_width = (ui.available_width() - 190.0).max(120.0);
            match self.sy.motion {
                MotionKind::Breathe => {
                    ui.add(egui::Slider::new(&mut self.sy.period, 1.0..=20.0).text("seconds per breath"));
                    ui.add(egui::Slider::new(&mut self.sy.depth, 0.05..=1.0).text("depth"));
                    ui.add(egui::Slider::new(&mut self.sy.spread, 0.0..=1.0).text("wave across the room"));
                }
                MotionKind::Drift => {
                    ui.add(egui::Slider::new(&mut self.sy.drift, 2.0..=120.0).text("seconds per cycle"));
                }
                MotionKind::Flicker => {
                    ui.add(egui::Slider::new(&mut self.sy.flicker, 0.05..=0.9).text("amount"));
                    ui.add(egui::Slider::new(&mut self.sy.flicker_speed, 0.5..=20.0).text("speed"));
                }
                MotionKind::Static => {}
            }
            ui.add(egui::Slider::new(&mut self.sy.brightness, 0.02..=1.0).text("brightness"));
        });
    }

    fn screen_area_card(&mut self, ui: &mut egui::Ui) {
        let (thumb, seq, region, content, status, fps, cur_mon) = {
            let s = self.screen.lock().unwrap();
            (s.thumb.clone(), s.thumb_seq, s.region, s.content, s.status.clone(), s.fps, s.monitor.clone())
        };
        let before = self.sy.screen.clone();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                section(ui, "Screen area");
                ui.label(RichText::new(format!("{status} · {fps:.0} fps")).size(12.0).color(FAINT));
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Monitor").color(MUTED));
                let label = cur_mon.as_ref().map(|m| m.label()).unwrap_or_else(|| "Primary".into());
                egui::ComboBox::from_id_salt("monitor").selected_text(label).show_ui(ui, |ui| {
                    for m in self.sy.monitors.clone() {
                        if ui.selectable_label(cur_mon.as_ref() == Some(&m), m.label()).clicked() {
                            self.screen.lock().unwrap().monitor = Some(m.clone());
                            self.cfg.screen.monitor = Some(m.name.clone());
                            self.save_cfg();
                        }
                    }
                });
                if ui.small_button("⟳").on_hover_text("Rescan monitors").clicked() {
                    self.sy.monitors = lumen_screen::capture::monitors().unwrap_or_default();
                }
            });
            ui.add_space(6.0);

            if let Some(f) = &thumb {
                if seq != self.sy.tex_seq || self.sy.tex.is_none() {
                    let mut rgba = Vec::with_capacity(f.bgra.len());
                    for px in f.bgra.chunks_exact(4) {
                        rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
                    }
                    let img = egui::ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &rgba);
                    match &mut self.sy.tex {
                        Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                        None => self.sy.tex = Some(ui.ctx().load_texture("screen", img, egui::TextureOptions::LINEAR)),
                    }
                    self.sy.tex_seq = seq;
                }
                if let Some(tex) = &self.sy.tex {
                    let w = ui.available_width();
                    let h = w * f.height as f32 / f.width as f32;
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::drag());
                    let p = ui.painter_at(rect);
                    p.image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    let to_px = |n: NRect| {
                        egui::Rect::from_min_size(
                            rect.min + Vec2::new(n.x * rect.width(), n.y * rect.height()),
                            Vec2::new(n.w * rect.width(), n.h * rect.height()),
                        )
                    };
                    let rr = to_px(region);
                    let dim = Color32::from_black_alpha(150);
                    p.rect_filled(egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rr.min.y)), 0.0, dim);
                    p.rect_filled(egui::Rect::from_min_max(egui::pos2(rect.min.x, rr.max.y), rect.max), 0.0, dim);
                    p.rect_filled(egui::Rect::from_min_max(egui::pos2(rect.min.x, rr.min.y), egui::pos2(rr.min.x, rr.max.y)), 0.0, dim);
                    p.rect_filled(egui::Rect::from_min_max(egui::pos2(rr.max.x, rr.min.y), egui::pos2(rect.max.x, rr.max.y)), 0.0, dim);
                    p.rect_stroke(rr, 2.0, Stroke::new(2.0_f32,ACCENT));
                    if self.sy.screen.detect_bars && content != region {
                        p.rect_stroke(to_px(content), 0.0, Stroke::new(1.5_f32, GOOD));
                    }
                    let norm = |pos: egui::Pos2| {
                        egui::pos2(((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0), ((pos.y - rect.min.y) / rect.height()).clamp(0.0, 1.0))
                    };
                    if resp.drag_started() {
                        self.sy.drag_from = resp.interact_pointer_pos().map(norm);
                    }
                    if let (Some(a), Some(cur)) = (self.sy.drag_from, resp.interact_pointer_pos().map(norm)) {
                        let r = egui::Rect::from_two_pos(a, cur);
                        p.rect_stroke(to_px(NRect { x: r.min.x, y: r.min.y, w: r.width(), h: r.height() }), 2.0, Stroke::new(2.0_f32,Color32::WHITE));
                        if resp.drag_stopped() {
                            if r.width() > 0.04 && r.height() > 0.04 {
                                self.sy.screen.region = NRect { x: r.min.x, y: r.min.y, w: r.width(), h: r.height() };
                                self.sy.follow = None;
                            }
                            self.sy.drag_from = None;
                        }
                    }
                    resp.on_hover_text("Drag to choose the part of the screen your lights follow");
                }
            } else {
                ui.label(RichText::new("Starting capture…").color(MUTED));
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new("Purple: the area being read. Green: the picture without black bars. Drag on the image to pick an area.")
                    .size(12.0)
                    .color(FAINT),
            );
            ui.horizontal_wrapped(|ui| {
                if ui.add(ghost("Whole screen")).clicked() {
                    self.sy.screen.region = NRect::FULL;
                    self.sy.follow = None;
                }
                if ui.add(ghost("Left half")).clicked() {
                    self.sy.screen.region = NRect { x: 0.0, y: 0.0, w: 0.5, h: 1.0 };
                    self.sy.follow = None;
                }
                if ui.add(ghost("Right half")).clicked() {
                    self.sy.screen.region = NRect { x: 0.5, y: 0.0, w: 0.5, h: 1.0 };
                    self.sy.follow = None;
                }
                let follow_label = self.sy.follow.as_ref().map(|f| format!("Following: {}", truncate(&f.1, 28))).unwrap_or_else(|| "Follow a window…".into());
                let cb = egui::ComboBox::from_id_salt("follow").selected_text(follow_label).width(240.0).show_ui(ui, |ui| {
                    if ui.selectable_label(self.sy.follow.is_none(), "Don't follow a window").clicked() {
                        self.sy.follow = None;
                    }
                    for w in &self.sy.windows {
                        if ui.selectable_label(self.sy.follow.as_ref().map(|f| f.0) == Some(w.hwnd), truncate(&w.title, 50)).clicked() {
                            self.sy.follow = Some((w.hwnd, w.title.clone()));
                        }
                    }
                });
                if cb.response.clicked() {
                    self.sy.windows = lumen_screen::windows_list::list().into_iter().filter(|w| w.title != "RokSync").collect();
                }
            });
        });
        if self.sy.screen != before {
            self.cfg.screen.settings = self.sy.screen.clone();
            self.save_cfg();
        }
    }

    fn picture_card(&mut self, ui: &mut egui::Ui) {
        let before = self.sy.screen.clone();
        card(ui, |ui| {
            section(ui, "Picture");
            ui.horizontal(|ui| {
                toggle(ui, &mut self.sy.screen.detect_bars);
                ui.label("Ignore black bars (letterbox / pillarbox)");
            });
            ui.add_space(4.0);
            ui.label(RichText::new("Aspect ratio lock").color(MUTED));
            let mut asp = self.sy.screen.aspect.map(|a| (a * 100.0).round() as i32).unwrap_or(0);
            if segmented(ui, &mut asp, &[(0, "Off"), (178, "16:9"), (233, "21:9"), (239, "2.39:1"), (133, "4:3"), (356, "32:9")]) {
                self.sy.screen.aspect = match asp {
                    0 => None,
                    178 => Some(16.0 / 9.0),
                    233 => Some(21.0 / 9.0),
                    239 => Some(2.39),
                    133 => Some(4.0 / 3.0),
                    _ => Some(32.0 / 9.0),
                };
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Colour reading").color(MUTED));
            segmented(ui, &mut self.sy.screen.mode, &[(SampleMode::Vibrant, "Vibrant"), (SampleMode::Average, "Natural")]);
            ui.add_space(6.0);
            ui.add(egui::Slider::new(&mut self.sy.screen.zone, 0.08..=0.8).text("area each light reads"));
            ui.add(egui::Slider::new(&mut self.sy.screen.smoothing, 0.0..=1.0).text("smoothness"));
            ui.add(egui::Slider::new(&mut self.sy.screen.saturation, 0.5..=2.0).text("saturation"));
            ui.add(egui::Slider::new(&mut self.sy.screen.min_brightness, 0.0..=0.4).text("minimum brightness"));
            ui.add(egui::Slider::new(&mut self.sy.screen.max_brightness, 0.2..=1.0).text("maximum brightness"));
            ui.add(egui::Slider::new(&mut self.sy.brightness, 0.02..=1.0).text("master brightness"));
        });
        if self.sy.screen != before {
            self.cfg.screen.settings = self.sy.screen.clone();
            self.save_cfg();
        }
    }
    fn saved_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            section(ui, "Saved looks");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.sy.save_name).hint_text("Name this look…").desired_width(220.0));
                if ui.add_enabled(!self.sy.save_name.trim().is_empty(), ghost("Save")).clicked() {
                    let name = self.sy.save_name.trim().to_string();
                    let program = self.sy.program();
                    self.cfg.saved.retain(|s| s.name != name);
                    self.cfg.saved.push(SavedProgram { name, program });
                    self.sy.save_name.clear();
                    self.save_cfg();
                }
            });
            let mut load = None;
            let mut del = None;
            ui.horizontal_wrapped(|ui| {
                for (i, s) in self.cfg.saved.iter().enumerate() {
                    egui::Frame::none().fill(SURFACE2).rounding(10.0).inner_margin(egui::Margin::symmetric(10.0, 6.0)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let cols: Vec<Rgb> = match &s.program.look {
                                Look::Solid(c) => vec![*c],
                                Look::Gradient { stops, .. } => stops.clone(),
                                Look::PerChannel(m) => m.values().cloned().collect(),
                                Look::Palette(p) => p.clone(),
                            };
                            for c in cols.iter().take(5) {
                                swatch(ui, *c, 14.0);
                            }
                            if ui.link(&s.name).clicked() {
                                load = Some(s.program.clone());
                            }
                            if ui.small_button("✕").clicked() {
                                del = Some(i);
                            }
                        });
                    });
                }
            });
            if let Some(p) = load {
                self.sy.load(&p);
                self.sy.source = Source::Look;
            }
            if let Some(i) = del {
                self.cfg.saved.remove(i);
                self.save_cfg();
            }
        });
    }
}
