use super::sync_page::LookKind;
use super::theme::*;
use super::{LumenApp, Page};
use crate::config::AppConfig;
use super::icons::Icon;
use eframe::egui::{self, vec2, RichText, Sense, Vec2};
use lumen_hue::{Inventory, Rgb};
use lumen_mood::{catalog, download, keywords, server::LlmServer, MoodPlan};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const EXAMPLES: &[&str] = &[
    "final boss, second phase",
    "neon cyberpunk alley at 3am",
    "stealth mission, guards close",
    "victory screen, gold confetti",
    "survival horror, flickering corridor",
    "underwater temple level",
    "cozy farming sim evening",
    "rainy Tokyo night, calm",
];

#[derive(Default)]
pub struct MoodState {
    pub busy: bool,
    pub status: String,
    pub plan: Option<MoodPlan>,
    pub err: Option<String>,
    pub downloads: HashMap<&'static str, (u64, u64)>,
    pub dl_err: Option<String>,
    pub last_used: Option<std::time::Instant>,
    /// model id whose warm-up failed, so it isn't retried every frame
    pub warm_failed: Option<&'static str>,
}

fn run_opts() -> lumen_mood::server::RunOpts {
    let p = crate::perf::profile();
    lumen_mood::server::RunOpts { gpu: p.model_gpu, threads: p.model_threads }
}

pub struct MoodUi {
    pub(super) prompt: String,
    pub(super) scope: Option<String>,
    pub(super) state: Arc<Mutex<MoodState>>,
    pub(super) server: Arc<Mutex<Option<LlmServer>>>,
    pub(super) warming: Arc<std::sync::atomic::AtomicBool>,
    pub(super) used_ai: bool,
}

impl MoodUi {
    pub fn new(_cfg: &AppConfig) -> Self {
        Self {
            prompt: String::new(),
            scope: None,
            state: Default::default(),
            server: Default::default(),
            warming: Default::default(),
            used_ai: false,
        }
    }
}

fn mb(b: u64) -> String {
    format!("{:.0} MB", b as f64 / 1_048_576.0)
}

impl LumenApp {
    pub(super) fn active_model(&self) -> Option<&'static catalog::ModelSpec> {
        let id = self.cfg.mood_model.as_deref()?;
        let m = catalog::find(id)?;
        (catalog::model_installed(m) && catalog::server_exe().is_some()).then_some(m)
    }

    pub(super) fn generate(&mut self, inv: Arc<Inventory>) {
        let prompt = self.mood.prompt.trim().to_string();
        if prompt.is_empty() {
            return;
        }
        let scope = self.mood.scope.clone();
        let model = self.active_model();
        self.mood.used_ai = model.is_some();
        let state = self.mood.state.clone();
        let server = self.mood.server.clone();
        let ctx = self.ctx.clone();
        let opts = run_opts();
        {
            let mut s = state.lock().unwrap();
            s.busy = true;
            s.err = None;
            s.status = if model.is_some() { "Thinking…".into() } else { String::new() };
        }
        std::thread::spawn(move || {
            let result: anyhow::Result<MoodPlan> = (|| match model {
                Some(m) => {
                    let mut srv = server.lock().unwrap();
                    if srv.as_ref().map(|s| s.model.id != m.id || s.opts != opts).unwrap_or(true) {
                        *srv = None;
                        state.lock().unwrap().status = format!("Loading {}…", m.name);
                        ctx.request_repaint();
                        *srv = Some(LlmServer::start_with(m, opts)?);
                        state.lock().unwrap().status = "Thinking…".into();
                        ctx.request_repaint();
                    }
                    srv.as_ref().unwrap().ask(&prompt, &inv, scope.as_deref())
                }
                None => Ok(keywords::interpret(&prompt, &inv, scope.as_deref())),
            })();
            let mut s = state.lock().unwrap();
            s.busy = false;
            s.status.clear();
            s.last_used = Some(std::time::Instant::now());
            match result {
                Ok(p) => s.plan = Some(p),
                Err(e) => s.err = Some(format!("{e:#}")),
            }
            ctx.request_repaint();
        });
    }

    /// While the Mood page is open, keeps the chosen model loading or loaded (≈1 s cold start).
    /// Nothing is started before the page is opened.
    pub fn keep_warm(&self) {
        use std::sync::atomic::Ordering;
        if self.is_away() {
            return;
        }
        let Some(m) = self.active_model() else { return };
        let opts = run_opts();
        if self.mood.state.lock().unwrap().warm_failed == Some(m.id) {
            return;
        }
        // loaded with the right model → just keep the idle timer fresh
        match self.mood.server.try_lock() {
            Ok(g) if g.as_ref().map_or(false, |s| s.model.id == m.id && s.opts == opts) => {
                self.mood.state.lock().unwrap().last_used = Some(std::time::Instant::now());
                return;
            }
            Ok(_) => {}
            Err(_) => return, // starting or answering right now
        }
        if self.mood.warming.swap(true, Ordering::SeqCst) {
            return;
        }
        let (server, state, warming, ctx) = (self.mood.server.clone(), self.mood.state.clone(), self.mood.warming.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            {
                let mut srv = server.lock().unwrap();
                if srv.as_ref().map_or(true, |s| s.model.id != m.id || s.opts != opts) {
                    *srv = None;
                    match LlmServer::start_with(m, opts) {
                        Ok(s) => *srv = Some(s),
                        Err(e) => {
                            let mut st = state.lock().unwrap();
                            st.warm_failed = Some(m.id);
                            st.err = Some(format!("Couldn't start {}: {e:#}", m.name));
                        }
                    }
                }
            }
            state.lock().unwrap().last_used = Some(std::time::Instant::now());
            warming.store(false, Ordering::SeqCst);
            ctx.request_repaint();
        });
    }

    pub(super) fn model_status(&self) -> &'static str {
        if self.mood.warming.load(std::sync::atomic::Ordering::SeqCst) {
            return "waking up…";
        }
        match self.mood.server.try_lock() {
            Ok(g) if g.is_some() => "ready",
            Ok(_) => "asleep",
            Err(_) => "busy",
        }
    }

    pub(super) fn is_away(&self) -> bool {
        self.ctx.input(|i| i.viewport().minimized == Some(true)) || self.tray.as_ref().map_or(false, |t| t.is_hidden())
    }

    /// Called every frame: stops llama-server after the idle timeout or as soon as the window is
    /// away. Returns true if a stop is wanted but the model is mid-answer or still loading, so the
    /// caller should check again shortly.
    pub fn mood_housekeeping(&mut self, away: bool) -> bool {
        let idle = {
            let s = self.mood.state.lock().unwrap();
            let limit = crate::perf::profile().model_idle;
            !s.busy && limit.map_or(false, |lim| s.last_used.map_or(false, |t| t.elapsed() > lim))
        };
        // no active model means Quick mode: free the RAM/VRAM
        let unwanted = self.active_model().is_none();
        if !(idle || unwanted || away) {
            return false;
        }
        match self.mood.server.try_lock() {
            Ok(mut srv) => {
                let was_loaded = srv.take().is_some();
                if idle || (away && was_loaded) {
                    self.mood.state.lock().unwrap().last_used = None;
                }
                away && self.mood.warming.load(std::sync::atomic::Ordering::SeqCst)
            }
            Err(_) => away,
        }
    }

    fn start_download(&self, m: &'static catalog::ModelSpec) {
        let state = self.mood.state.clone();
        let ctx = self.ctx.clone();
        state.lock().unwrap().dl_err = None;
        state.lock().unwrap().downloads.insert(m.id, (0, m.bytes + catalog::RUNTIME.bytes));
        std::thread::spawn(move || {
            let rt = catalog::RUNTIME.bytes;
            let r = download::install_runtime(&|d, _| {
                state.lock().unwrap().downloads.insert(m.id, (d, m.bytes + rt));
                ctx.request_repaint_after(Duration::from_millis(100));
            })
            .and_then(|_| {
                download::install_model(m, &|d, t| {
                    state.lock().unwrap().downloads.insert(m.id, (rt + d, rt + t.max(m.bytes)));
                    ctx.request_repaint_after(Duration::from_millis(100));
                })
            });
            let mut s = state.lock().unwrap();
            s.downloads.remove(m.id);
            if let Err(e) = r {
                s.dl_err = Some(format!("{}: {e:#}", m.name));
            }
            ctx.request_repaint();
        });
    }

    pub fn mood_page(&mut self, ui: &mut egui::Ui, inv: &Arc<Inventory>) {
        page_title(ui, "05", "Vibe", "Describe a scene and get lights to match.");
        self.keep_warm();
        let (busy, status, plan, err, downloads, dl_err) = {
            let s = self.mood.state.lock().unwrap();
            (s.busy, s.status.clone(), s.plan.clone(), s.err.clone(), s.downloads.clone(), s.dl_err.clone())
        };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            card(ui, |ui| {
                section(ui, "Prompt");
                let te = ui
                    .horizontal_top(|ui| {
                        ui.label(RichText::new(">").font(mono(22.0)).color(ACCENT));
                        ui.add(
                            egui::TextEdit::multiline(&mut self.mood.prompt)
                                .hint_text("final boss, second phase, the desk is the focus…")
                                .font(egui::FontId::proportional(20.0))
                                .frame(false)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        )
                    })
                    .inner;
                let enter = te.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                if enter {
                    // drop the newline the TextEdit just inserted
                    while self.mood.prompt.ends_with('\n') {
                        self.mood.prompt.pop();
                    }
                }
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(6.0);
                    for ex in EXAMPLES {
                        let chip = ghost(ex).plain().small();
                        if ui.add(chip).clicked() {
                            self.mood.prompt = ex.to_string();
                        }
                    }
                });
                ui.add_space(8.0);
                let mut go = enter;
                ui.horizontal(|ui| {
                    ui.label(label_caps("Target", 11.5, MUTED));
                    let t = self.target(inv);
                    self.mood.scope = t.group.map(|g| g.metadata.name.clone());
                    ui.label(RichText::new(format!("{} · {} lights", t.name, t.lights.len())).color(TEXT));
                    ui.label(label_caps("Engine", 11.5, MUTED));
                    let engine = self.active_model().map(|m| m.name).unwrap_or("Quick");
                    egui::ComboBox::from_id_salt("engine").selected_text(engine).width(240.0).show_ui(ui, |ui| {
                        if ui.selectable_label(self.cfg.mood_model.is_none(), "Quick").clicked() {
                            self.cfg.mood_model = None;
                            self.save_cfg();
                        }
                        for m in catalog::MODELS.iter().filter(|m| !m.retired || catalog::model_installed(m)) {
                            let ok = catalog::model_installed(m);
                            if ui.add_enabled(ok, egui::SelectableLabel::new(self.cfg.mood_model.as_deref() == Some(m.id), m.name)).clicked() {
                                self.cfg.mood_model = Some(m.id.to_string());
                                self.save_cfg();
                            }
                        }
                    });
                    if self.active_model().is_some() {
                        let st = self.model_status();
                        let col = match st {
                            "ready" => GOOD,
                            "asleep" => FAINT,
                            _ => ACCENT2,
                        };
                        pill(ui, st, col);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!busy && !self.mood.prompt.trim().is_empty(), primary("Create").icon(Icon::Vibe).min_size(vec2(150.0, 40.0))).clicked() {
                            go = true;
                        }
                        if busy {
                            ui.spinner();
                            ui.label(RichText::new(&status).color(MUTED));
                        }
                    });
                });
                ui.label(
                    RichText::new("Create previews the plan. Lights change only when you press Apply.")
                        .size(12.0)
                        .color(FAINT),
                );
                if go && !busy {
                    self.generate(inv.clone());
                }
            });

            if let Some(e) = &err {
                ui.add_space(8.0);
                ui.label(RichText::new(e).color(BAD));
            }

            if let Some(plan) = &plan {
                ui.add_space(12.0);
                self.plan_card(ui, inv, plan, busy);
            }

            ui.add_space(12.0);
            card(ui, |ui| {
                section(ui, "AI models");
                ui.label(
                    RichText::new(
                        "Optional. Pick one and it downloads from Hugging Face. \
                         Without one, Quick mode picks from a built-in mood table. \
                         The model sleeps when you leave this page or minimise RokSync.",
                    )
                    .size(12.5)
                    .color(FAINT),
                );
                ui.add_space(6.0);
                for m in catalog::MODELS.iter().filter(|m| !m.retired || catalog::model_installed(m)) {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(label_caps(m.name, 15.0, TEXT));
                            ui.label(RichText::new(m.blurb).size(12.5).color(MUTED));
                            ui.label(RichText::new(format!("{} · {}", mb(m.bytes), m.license)).size(12.0).color(FAINT));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some((d, t)) = downloads.get(m.id) {
                                let frac = if *t > 0 { *d as f32 / *t as f32 } else { 0.0 };
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(format!("{} / {}", mb(*d), mb(*t))).font(mono(11.0)).color(ACCENT));
                                    let (r, _) = ui.allocate_exact_size(vec2(200.0, 6.0), Sense::hover());
                                    meter(ui.painter(), r, frac, 24, ACCENT);
                                });
                            } else if catalog::model_installed(m) {
                                if ui.add(icon_btn(Icon::Trash)).on_hover_text("Remove").clicked() {
                                    if self.cfg.mood_model.as_deref() == Some(m.id) {
                                        self.cfg.mood_model = None;
                                        self.save_cfg();
                                    }
                                    *self.mood.server.lock().unwrap() = None;
                                    let _ = download::remove_model(m);
                                }
                                let using = self.cfg.mood_model.as_deref() == Some(m.id);
                                if using {
                                    pill(ui, "in use", GOOD);
                                } else if ui.add(ghost("Use").small()).clicked() {
                                    self.cfg.mood_model = Some(m.id.to_string());
                                    self.save_cfg();
                                }
                            } else if ui.add(primary(&format!("Download {}", mb(m.bytes))).icon(Icon::Save).small()).clicked() {
                                self.start_download(m);
                            }
                        });
                    });
                }
                if let Some(e) = &dl_err {
                    ui.label(RichText::new(e).color(BAD));
                }
            });
        });
        if busy || !downloads.is_empty() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    }

    fn plan_card(&mut self, ui: &mut egui::Ui, inv: &Arc<Inventory>, plan: &MoodPlan, busy: bool) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(plan.title.to_uppercase()).font(display_bold(26.0)).extra_letter_spacing(2.0));
                ui.label(RichText::new(format!("{} · {}", plan.motion, plan.speed)).color(FAINT));
                if !self.mood.used_ai {
                    ui.label(RichText::new("quick mode").size(12.0).color(FAINT));
                }
            });
            palette_bar(ui, &plan.palette_rgb(), 26.0);
            ui.add_space(6.0);
            egui::Grid::new("plan").num_columns(3).spacing(Vec2::new(14.0, 6.0)).show(ui, |ui| {
                for a in &plan.assignments {
                    let cols: Vec<Rgb> = a.colors.iter().filter_map(|c| Rgb::from_hex(c)).collect();
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                    glow(ui.painter(), r.center(), 6.0, cols.first().copied().unwrap_or(Rgb::BLACK), true);
                    ui.label(truncate(a.target.trim(), 30));
                    ui.horizontal(|ui| {
                        for c in &cols {
                            swatch(ui, *c, 14.0);
                        }
                        ui.label(RichText::new(format!("{:.0}%", a.brightness)).color(FAINT));
                    });
                    ui.end_row();
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let t = self.target(inv);
                let ids = t.ids();
                let states: Vec<_> = plan.to_states(inv).into_iter().filter(|(id, _, _)| ids.contains(id.as_str())).collect();
                let states_n = states.len();
                if ui.add(primary(&format!("Apply to {states_n} lights in {}", t.name)).icon(Icon::Play)).clicked() {
                    let stopped = self.is_streaming();
                    if stopped {
                        self.stop_live();
                    }
                    let title = plan.title.clone();
                    self.task("Mood", true, move |b| {
                        if stopped {
                            std::thread::sleep(Duration::from_millis(400));
                        }
                        for (id, _, st) in &states {
                            b.set_light(id, st)?;
                            std::thread::sleep(Duration::from_millis(110));
                        }
                        Ok(format!("\"{title}\" applied to {} lights", states.len()))
                    });
                }
                if ui.add(ghost("Animate in Live").icon(Icon::Live)).on_hover_text("Use the palette and motion on your entertainment area").clicked() {
                    self.sy.palette = plan.palette_rgb().iter().map(|c| c.to_u8()).collect();
                    self.sy.look = LookKind::Palette;
                    self.sy.source = crate::sync::Source::Look;
                    let mut p = self.sy.program();
                    p.motion = plan.motion();
                    p.look = lumen_engine::Look::Palette(plan.palette_rgb());
                    self.sy.load(&p);
                    self.page = Page::Sync;
                }
                if ui.add(ghost("Edit in Moods").icon(Icon::Loadouts)).on_hover_text("Open these colours in the mood editor").clicked() {
                    self.open_in_studio(&plan.title, &plan.palette_rgb());
                }
                if ui.add_enabled(!busy, ghost("Reroll").icon(Icon::Dice)).clicked() {
                    self.generate(inv.clone());
                }
            });
        });
    }
}
