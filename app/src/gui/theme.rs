//! Colours, fonts and shared widgets. All motion stops when the perf preset turns UI animations off.

use super::icons::{self, Icon};
use eframe::egui::{
    self, epaint, pos2, text::LayoutJob, vec2, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Margin, Mesh, Painter, Pos2,
    Rect, Response, RichText, Rounding, Sense, Shape, Stroke, TextFormat, TextStyle, Ui, Vec2, Widget,
};
use lumen_hue::Rgb;

pub const BG: Color32 = Color32::from_rgb(13, 13, 15);
pub const SIDEBAR: Color32 = Color32::from_rgb(20, 20, 23);
pub const SURFACE: Color32 = Color32::from_rgb(20, 20, 23);
pub const SURFACE2: Color32 = Color32::from_rgb(28, 28, 32);
pub const SURFACE3: Color32 = Color32::from_rgb(38, 38, 43);
pub const HOVER: Color32 = Color32::from_rgb(23, 23, 27);
pub const OPENED: Color32 = Color32::from_rgb(17, 17, 20);
/// rgba(255, 255, 255, 0.075), premultiplied.
pub const RULE: Color32 = Color32::from_rgba_premultiplied(19, 19, 19, 19);
pub const BORDER: Color32 = SURFACE3;
pub const EDGE: Color32 = Color32::from_rgb(58, 58, 64);
pub const HATCH: Color32 = Color32::from_rgb(46, 46, 52);
pub const TEXT: Color32 = Color32::from_rgb(243, 241, 234);
pub const SOFT: Color32 = Color32::from_rgb(201, 199, 192);
pub const MUTED: Color32 = Color32::from_rgb(140, 140, 147);
pub const FAINT: Color32 = Color32::from_rgb(88, 88, 96);
pub const ACCENT: Color32 = Color32::from_rgb(255, 90, 31);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(255, 116, 64);
pub const ACCENT2: Color32 = Color32::from_rgb(79, 209, 255);
pub const INFO: Color32 = ACCENT2;
pub const GOOD: Color32 = Color32::from_rgb(124, 255, 107);
pub const WARN: Color32 = Color32::from_rgb(255, 176, 0);
pub const BAD: Color32 = Color32::from_rgb(215, 38, 61);
pub const INK: Color32 = BG;
pub const HIT: f32 = 40.0;
pub const PAD_X: f32 = 40.0;

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}
pub fn display_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display-bold".into()))
}
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}
pub fn body(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn track(size: f32) -> f32 {
    if size >= 40.0 {
        -0.4
    } else if size >= 26.0 {
        -0.2
    } else if size >= 20.0 {
        0.5
    } else {
        1.0
    }
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |f: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        f.font_data.insert(name.into(), FontData::from_static(bytes));
    };
    add(&mut fonts, "barlow", include_bytes!("../../assets/fonts/Barlow-Regular.ttf"));
    add(&mut fonts, "barlow-md", include_bytes!("../../assets/fonts/Barlow-Medium.ttf"));
    add(&mut fonts, "barlow-sb", include_bytes!("../../assets/fonts/Barlow-SemiBold.ttf"));
    add(&mut fonts, "bc-sb", include_bytes!("../../assets/fonts/BarlowCondensed-SemiBold.ttf"));
    add(&mut fonts, "bc-b", include_bytes!("../../assets/fonts/BarlowCondensed-Bold.ttf"));
    add(&mut fonts, "bc-xb", include_bytes!("../../assets/fonts/BarlowCondensed-ExtraBold.ttf"));
    // Arrows and geometric marks (← → ▾ ↻) aren't in Barlow or egui's defaults; Windows ships them.
    let symbols = std::env::var("WINDIR")
        .ok()
        .and_then(|w| std::fs::read(std::path::Path::new(&w).join("Fonts").join("seguisym.ttf")).ok());
    if let Some(bytes) = symbols {
        fonts.font_data.insert("symbols".into(), FontData::from_owned(bytes));
        for fam in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.get_mut(&fam).unwrap().push("symbols".into());
        }
    }
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "barlow".into());
    fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "bc-sb".into());
    let family = |first: &str| {
        let mut v = vec![first.to_string(), "barlow".to_string()];
        v.extend(fallbacks.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Name("semibold".into()), family("barlow-sb"));
    fonts.families.insert(FontFamily::Name("medium".into()), family("barlow-md"));
    fonts.families.insert(FontFamily::Name("display".into()), family("bc-b"));
    fonts.families.insert(FontFamily::Name("display-bold".into()), family("bc-xb"));
    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading, display_bold(30.0)),
        (TextStyle::Body, body(14.5)),
        (TextStyle::Button, display(16.0)),
        (TextStyle::Small, body(12.5)),
        (TextStyle::Monospace, mono(14.0)),
    ]
    .into();
    style.spacing.item_spacing = Vec2::new(10.0, 8.0);
    style.spacing.button_padding = Vec2::new(14.0, 8.0);
    style.spacing.interact_size.y = 32.0;
    style.spacing.slider_width = 220.0;
    style.spacing.combo_width = 220.0;
    style.spacing.scroll = egui::style::ScrollStyle::thin();
    style.spacing.scroll.bar_width = 6.0;
    style.spacing.scroll.floating = true;

    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = SIDEBAR;
    v.extreme_bg_color = SURFACE2;
    v.faint_bg_color = SURFACE;
    v.code_bg_color = SURFACE2;
    v.window_stroke = Stroke::new(1.0_f32, SURFACE3);
    v.window_rounding = Rounding::ZERO;
    v.menu_rounding = Rounding::ZERO;
    v.window_shadow = epaint::Shadow { offset: vec2(0.0, 8.0), blur: 24.0, spread: 0.0, color: Color32::from_black_alpha(150) };
    v.popup_shadow = v.window_shadow;
    v.hyperlink_color = TEXT;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0_f32, INK);
    v.text_cursor.stroke = Stroke::new(2.0_f32, ACCENT);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.3 };
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);
    let r = Rounding::ZERO;
    v.widgets.noninteractive.bg_fill = SURFACE;
    v.widgets.noninteractive.weak_bg_fill = SURFACE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, SURFACE3);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.noninteractive.rounding = r;
    v.widgets.inactive.bg_fill = SURFACE3;
    v.widgets.inactive.weak_bg_fill = SURFACE2;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.inactive.rounding = r;
    v.widgets.hovered.bg_fill = EDGE;
    v.widgets.hovered.weak_bg_fill = SURFACE3;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, TEXT);
    v.widgets.hovered.rounding = r;
    v.widgets.hovered.expansion = 0.0;
    v.widgets.active.bg_fill = TEXT;
    v.widgets.active.weak_bg_fill = SURFACE3;
    v.widgets.active.bg_stroke = Stroke::new(2.0_f32, ACCENT);
    v.widgets.active.fg_stroke = Stroke::new(1.5_f32, TEXT);
    v.widgets.active.rounding = r;
    v.widgets.active.expansion = 0.0;
    v.widgets.open = v.widgets.active;
    v.widgets.open.weak_bg_fill = SURFACE3;
    ctx.set_style(style);
}

pub fn c32(c: Rgb) -> Color32 {
    let [r, g, b] = c.to_u8();
    Color32::from_rgb(r, g, b)
}

pub fn hex(s: &str) -> Color32 {
    c32(Rgb::from_hex(s).unwrap_or(Rgb::WHITE))
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n.saturating_sub(1)).collect::<String>())
    }
}

pub fn motion() -> bool {
    crate::perf::profile().ui_animations
}

pub fn anim_time(ui: &Ui) -> f32 {
    if motion() {
        ui.input(|i| i.time) as f32
    } else {
        0.0
    }
}

pub fn ease_bool(ui: &Ui, id: egui::Id, on: bool, secs: f32) -> f32 {
    let t = ui.ctx().animate_bool_with_time(id, on, if motion() { secs } else { 0.0 });
    ease(t)
}

/// Quartic ease-out, close to cubic-bezier(.2,0,0,1): fast out, gentle settle.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(4)
}

/// Single-line text with letter spacing; colour PLACEHOLDER takes the painter's fallback colour.
pub fn spaced(text: &str, font: FontId, color: Color32, spacing: f32) -> LayoutJob {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat { font_id: font, color, extra_letter_spacing: spacing, ..Default::default() });
    job.wrap.max_width = f32::INFINITY;
    job
}

pub fn text_at(p: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32, spacing: f32) -> Rect {
    let g = p.layout_job(spaced(text, font, color, spacing));
    let r = align.anchor_size(pos, g.size());
    p.galley(r.min, g, color);
    r
}

pub fn num_unit(p: &Painter, pos: Pos2, align: Align2, num: &str, unit: &str, size: f32, color: Color32) -> Rect {
    let mut job = LayoutJob::default();
    job.append(num, 0.0, TextFormat { font_id: display(size), color, ..Default::default() });
    if !unit.is_empty() {
        job.append(unit, 1.0, TextFormat { font_id: display(size * 0.6), color: MUTED, extra_letter_spacing: 0.5, valign: egui::Align::BOTTOM, ..Default::default() });
    }
    job.wrap.max_width = f32::INFINITY;
    let g = p.layout_job(job);
    let r = align.anchor_size(pos, g.size());
    p.galley(r.min, g, color);
    r
}

pub fn label_caps(text: &str, size: f32, color: Color32) -> RichText {
    RichText::new(text.to_uppercase()).font(display(size)).color(color).extra_letter_spacing(track(size))
}

pub fn headline(text: &str, size: f32) -> RichText {
    RichText::new(text.to_uppercase()).font(display_bold(size)).color(TEXT).extra_letter_spacing(track(size))
}

pub fn chamfer(rect: Rect, _cut: f32) -> Vec<Pos2> {
    vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()]
}

pub fn chamfer_shape(rect: Rect, _cut: f32, fill: Color32, stroke: Stroke) -> Shape {
    Shape::rect_filled(rect, 0.0, fill).also_stroke(rect, stroke)
}

trait AlsoStroke {
    fn also_stroke(self, rect: Rect, stroke: Stroke) -> Shape;
}
impl AlsoStroke for Shape {
    fn also_stroke(self, rect: Rect, stroke: Stroke) -> Shape {
        if stroke.width <= 0.0 || stroke.color == Color32::TRANSPARENT {
            self
        } else {
            Shape::Vec(vec![self, Shape::rect_stroke(rect.shrink(stroke.width / 2.0), 0.0, stroke)])
        }
    }
}

pub fn brackets(p: &Painter, rect: Rect, color: Color32, _len: f32, width: f32) {
    p.rect_stroke(rect.shrink(width / 2.0), 0.0, Stroke::new(width, color));
}

pub fn focus_ring(p: &Painter, rect: Rect) {
    p.rect_stroke(rect.expand(1.0), 0.0, Stroke::new(2.0_f32, BG));
    p.rect_stroke(rect.expand(3.0), 0.0, Stroke::new(2.0_f32, ACCENT));
}

pub fn hatch(p: &Painter, rect: Rect, base: Color32) {
    p.rect_filled(rect, 0.0, base);
    let clip = p.with_clip_rect(rect.intersect(p.clip_rect()));
    let step = 7.0;
    let st = Stroke::new(2.1_f32, HATCH);
    let span = rect.width() + rect.height();
    let mut d = -rect.height();
    while d < span {
        let a = pos2(rect.left() + d, rect.bottom());
        let b = pos2(rect.left() + d + rect.height(), rect.top());
        clip.line_segment([a, b], st);
        d += step;
    }
}

pub fn lit_edge(p: &Painter, rect: Rect) {
    p.rect_stroke(rect.shrink(0.5), 0.0, Stroke::new(1.0_f32, Color32::from_white_alpha(18)));
    p.line_segment([rect.left_top() + vec2(0.0, 0.5), rect.right_top() + vec2(0.0, 0.5)], Stroke::new(1.0_f32, Color32::from_white_alpha(56)));
}

pub fn rule(p: &Painter, y: f32, x0: f32, x1: f32) {
    p.line_segment([pos2(x0, y + 0.5), pos2(x1, y + 0.5)], Stroke::new(1.0_f32, RULE));
}

pub fn row_rule(p: &Painter, y: f32, x0: f32, x1: f32) {
    p.line_segment([pos2(x0, y + 0.5), pos2(x1, y + 0.5)], Stroke::new(1.0_f32, SURFACE3));
}

pub fn chip(p: &Painter, center: Pos2, color: Color32) {
    p.rect_filled(Rect::from_center_size(center, Vec2::splat(8.0)), 0.0, color);
}

pub fn bloom(_p: &Painter, _center: Pos2, _radius: f32, _color: Color32, _strength: f32, _squash: f32) {}

pub fn glow(p: &Painter, center: Pos2, r: f32, c: Rgb, on: bool) {
    let rect = Rect::from_center_size(center, Vec2::splat(r * 2.0));
    if on && c.max_channel() > 0.02 {
        p.rect_filled(rect, 0.0, c32(c));
        lit_edge(p, rect);
    } else {
        hatch(p, rect, SURFACE2);
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct PanelStyle {
    pub fill: Color32,
    pub edge: Color32,
    pub accent: Option<Color32>,
    pub glow: Option<Color32>,
    pub cut: f32,
}

impl Default for PanelStyle {
    fn default() -> Self {
        Self { fill: SURFACE, edge: Color32::TRANSPARENT, accent: None, glow: None, cut: 0.0 }
    }
}

pub fn panel_shapes(_p: &Painter, rect: Rect, st: PanelStyle) -> Vec<Shape> {
    let mut out = vec![Shape::rect_filled(rect, 0.0, st.fill)];
    if st.edge != Color32::TRANSPARENT && st.edge != SURFACE3 && st.edge != BORDER {
        out.push(Shape::rect_stroke(rect.shrink(1.0), 0.0, Stroke::new(2.0_f32, st.edge)));
    }
    if let Some(a) = st.accent {
        out.push(Shape::rect_filled(Rect::from_min_size(rect.min, vec2(4.0, rect.height())), 0.0, a));
    }
    out
}

pub fn panel<R>(ui: &mut Ui, st: PanelStyle, margin: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let bg = ui.painter().add(Shape::Noop);
    let r = egui::Frame::none().inner_margin(Margin::same(margin)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui)
    });
    let shapes = panel_shapes(ui.painter(), r.response.rect, st);
    ui.painter().set(bg, Shape::Vec(shapes));
    r.inner
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    panel(ui, PanelStyle::default(), 16.0, add)
}

pub fn section(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title.to_uppercase()).font(display(18.0)).color(TEXT).extra_letter_spacing(1.5));
}

pub fn section_note(ui: &mut Ui, title: &str, note: &str) {
    ui.horizontal(|ui| {
        section(ui, title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(note).font(body(13.5)).color(MUTED));
        });
    });
}

pub fn kicker(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let (r, _) = ui.allocate_exact_size(vec2(28.0, 16.0), Sense::hover());
        ui.painter().rect_filled(Rect::from_center_size(r.center(), vec2(28.0, 4.0)), 0.0, ACCENT);
        ui.label(RichText::new(text.to_uppercase()).font(display(15.0)).color(ACCENT).extra_letter_spacing(2.0));
    });
}

pub fn page_title(ui: &mut Ui, kick: &str, text: &str, sub: &str) {
    let k = kick.trim().trim_start_matches("//").trim();
    let k = k.trim_start_matches(|c: char| c.is_ascii_digit()).trim();
    if !k.is_empty() {
        kicker(ui, k);
    }
    ui.label(headline(text, 44.0));
    if !sub.is_empty() {
        ui.label(RichText::new(sub).font(body(14.0)).color(MUTED));
    }
    ui.add_space(10.0);
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Primary,
    Ghost,
    Danger,
    Quiet,
    Light,
}

pub struct HudButton {
    text: String,
    kind: Kind,
    min: Vec2,
    small: bool,
    icon: Option<Icon>,
    tint: Option<Color32>,
    selected: bool,
    plain: bool,
    height: Option<f32>,
}

impl HudButton {
    pub fn new(text: &str, kind: Kind) -> Self {
        Self { text: text.to_string(), kind, min: Vec2::ZERO, small: false, icon: None, tint: None, selected: false, plain: false, height: None }
    }
    pub fn min_size(mut self, s: Vec2) -> Self {
        self.min = s;
        self
    }
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }
    pub fn icon(mut self, i: Icon) -> Self {
        self.icon = Some(i);
        self
    }
    pub fn tint(mut self, c: Color32) -> Self {
        self.tint = Some(c);
        self
    }
    pub fn selected(mut self, s: bool) -> Self {
        self.selected = s;
        self
    }
    pub fn plain(mut self) -> Self {
        self.plain = true;
        self
    }
    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }
}

impl Widget for HudButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let enabled = ui.is_enabled();
        let big = matches!(self.kind, Kind::Primary | Kind::Light) && !self.small;
        let fsize = if self.small { 14.0 } else if big { 18.0 } else { 16.0 };
        let (label, font, sp) = if self.plain {
            (self.text.clone(), body(fsize - 1.5), 0.0)
        } else if big {
            (self.text.to_uppercase(), display_bold(fsize), 1.0)
        } else {
            (self.text.to_uppercase(), display(fsize), 1.0)
        };
        let galley = ui.painter().layout_job(spaced(&label, font, Color32::PLACEHOLDER, sp));
        let pad = if self.small { vec2(12.0, 6.0) } else { vec2(18.0, 8.0) };
        let isz = if self.small { 13.0 } else { 14.0 };
        let icon_w = match (self.icon, label.is_empty()) {
            (Some(_), true) => isz,
            (Some(_), false) => isz + 10.0,
            _ => 0.0,
        };
        let h = self.height.unwrap_or(if self.small { 32.0 } else if big { 48.0 } else { HIT });
        let mut size = vec2(galley.size().x + icon_w + pad.x * 2.0, h);
        if label.is_empty() {
            size.x = size.y;
        }
        let size = size.max(self.min);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        if !ui.is_rect_visible(rect) {
            return resp;
        }
        let hov = ease_bool(ui, resp.id.with("hov"), enabled && resp.hovered(), 0.15);
        let down = enabled && resp.is_pointer_button_down_on();
        let draw = if down { Rect::from_center_size(rect.center(), rect.size() * 0.96) } else { rect };
        let p = ui.painter();
        let base = self.tint.unwrap_or(match self.kind {
            Kind::Danger => BAD,
            Kind::Light => TEXT,
            _ => ACCENT,
        });
        let (fill, stroke, ink) = match self.kind {
            Kind::Primary if enabled => (mix(base, ACCENT_HOVER, hov), Stroke::NONE, INK),
            Kind::Light if enabled => (mix(base, Color32::WHITE, hov * 0.5), Stroke::NONE, INK),
            Kind::Primary | Kind::Light => (SURFACE3, Stroke::NONE, MUTED),
            Kind::Danger => (mix(base, mix(base, Color32::WHITE, 0.15), hov), Stroke::NONE, TEXT),
            Kind::Ghost => {
                if self.selected {
                    (TEXT, Stroke::NONE, INK)
                } else if !enabled {
                    (Color32::TRANSPARENT, Stroke::new(1.0_f32, SURFACE3), MUTED)
                } else {
                    (mix(Color32::TRANSPARENT, TEXT, hov), Stroke::new(1.0_f32, TEXT), mix(TEXT, INK, hov))
                }
            }
            Kind::Quiet => (mix(Color32::TRANSPARENT, SURFACE2, hov), Stroke::NONE, if enabled { mix(MUTED, TEXT, hov) } else { FAINT }),
        };
        p.rect_filled(draw, 0.0, fill);
        if stroke.width > 0.0 {
            p.rect_stroke(draw.shrink(0.5), 0.0, stroke);
        }
        let content_w = icon_w + galley.size().x;
        let mut x = draw.center().x - content_w / 2.0;
        if let Some(i) = self.icon {
            icons::paint(p, Rect::from_center_size(pos2(x + isz / 2.0, draw.center().y), Vec2::splat(isz)), i, ink);
            x += icon_w;
        }
        p.galley(pos2(x, draw.center().y - galley.size().y / 2.0), galley, ink);
        if resp.has_focus() {
            focus_ring(p, rect);
        }
        resp
    }
}

pub fn primary(text: &str) -> HudButton {
    HudButton::new(text, Kind::Primary)
}
pub fn danger(text: &str) -> HudButton {
    HudButton::new(text, Kind::Danger)
}
pub fn ghost(text: &str) -> HudButton {
    HudButton::new(text, Kind::Ghost)
}
pub fn quiet(text: &str) -> HudButton {
    HudButton::new(text, Kind::Quiet)
}
pub fn light(text: &str) -> HudButton {
    HudButton::new(text, Kind::Light)
}
pub fn icon_btn(i: Icon) -> HudButton {
    HudButton::new("", Kind::Quiet).icon(i).small()
}

pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let r = onoff(ui, *on, vec2(64.0, HIT));
    let mut r = r;
    if r.clicked() {
        *on = !*on;
        r.mark_changed();
    }
    r
}

pub fn onoff(ui: &mut Ui, on: bool, size: Vec2) -> Response {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    onoff_at(ui, rect, ui.next_auto_id(), on)
}

pub fn onoff_at(ui: &mut Ui, rect: Rect, id: egui::Id, on: bool) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let t = ease_bool(ui, resp.id.with("wipe"), on, 0.22);
    let down = resp.is_pointer_button_down_on();
    let draw = if down { Rect::from_center_size(rect.center(), rect.size() * 0.96) } else { rect };
    let p = ui.painter();
    p.rect_filled(draw, 0.0, SURFACE3);
    if t > 0.0 {
        p.rect_filled(Rect::from_min_size(draw.min, vec2(draw.width() * t, draw.height())), 0.0, TEXT);
    }
    let label = if on { "ON" } else { "OFF" };
    let ink = if t > 0.5 { INK } else { SOFT };
    text_at(p, draw.center(), Align2::CENTER_CENTER, label, display_bold(16.0), ink, 1.0);
    if resp.has_focus() {
        focus_ring(p, rect);
    }
    resp
}

pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    segmented_h(ui, value, options, HIT, false)
}

pub fn segmented_h<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)], h: f32, fill: bool) -> bool {
    let font = display(15.0);
    let galleys: Vec<_> = options.iter().map(|(_, l)| ui.painter().layout_job(spaced(&l.to_uppercase(), font.clone(), Color32::PLACEHOLDER, 1.0))).collect();
    let pad = 14.0;
    let mut widths: Vec<f32> = galleys.iter().map(|g| g.size().x + pad * 2.0).collect();
    let natural: f32 = widths.iter().sum();
    if fill && natural < ui.available_width() {
        let w = ui.available_width() / widths.len() as f32;
        widths.iter_mut().for_each(|x| *x = w.max(*x));
    }
    let total: f32 = widths.iter().sum();
    let (rect, resp) = ui.allocate_exact_size(vec2(total, h), Sense::click());
    let mut xs = Vec::with_capacity(widths.len());
    let mut x = rect.left();
    for w in &widths {
        xs.push(x);
        x += w;
    }
    let seg_at = |px: f32| xs.iter().zip(&widths).position(|(x, w)| px >= *x && px < *x + *w);
    let mut changed = false;
    if resp.clicked() {
        if let Some(i) = resp.interact_pointer_pos().and_then(|p| seg_at(p.x)) {
            if *value != options[i].0 {
                *value = options[i].0;
                changed = true;
            }
        }
    }
    let hovered = resp.hover_pos().and_then(|p| seg_at(p.x));
    let p = ui.painter();
    for (i, g) in galleys.into_iter().enumerate() {
        let sel = options[i].0 == *value;
        let r = Rect::from_min_size(pos2(xs[i], rect.top()), vec2(widths[i], h));
        let fill = if sel { TEXT } else if hovered == Some(i) { SURFACE3 } else { SURFACE2 };
        p.rect_filled(r, 0.0, fill);
        let col = if sel { INK } else { SOFT };
        p.galley(r.center() - g.size() / 2.0, g, col);
    }
    changed
}

pub fn pill(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let g = ui.painter().layout_job(spaced(&text.to_uppercase(), display(14.0), Color32::PLACEHOLDER, 1.0));
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 30.0, 28.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, SURFACE2);
    chip(p, pos2(rect.left() + 12.0, rect.center().y), color);
    p.galley(pos2(rect.left() + 22.0, rect.center().y - g.size().y / 2.0), g, SOFT);
    resp
}

pub fn tag(ui: &mut Ui, text: &str, fill: Color32, ink: Color32) -> Response {
    let g = ui.painter().layout_job(spaced(&text.to_uppercase(), display(14.0), Color32::PLACEHOLDER, 1.0));
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 20.0, 28.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, fill);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, ink);
    resp
}

pub fn meter(p: &Painter, rect: Rect, frac: f32, segments: usize, color: Color32) {
    let gap = 3.0;
    let w = (rect.width() - gap * (segments as f32 - 1.0)) / segments as f32;
    let lit = (frac.clamp(0.0, 1.0) * segments as f32).ceil() as usize;
    for i in 0..segments {
        let r = Rect::from_min_size(pos2(rect.left() + i as f32 * (w + gap), rect.top()), vec2(w, rect.height()));
        p.rect_filled(r, 0.0, if i < lit { color } else { SURFACE3 });
    }
}

pub fn swatch(ui: &mut Ui, c: Rgb, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
    let r = Rect::from_center_size(rect.center(), rect.size() * (1.0 + 0.08 * hov));
    let p = ui.painter();
    p.rect_filled(r, 0.0, c32(c));
    p.rect_stroke(r.shrink(0.5), 0.0, Stroke::new(1.0_f32, Color32::from_white_alpha(30)));
    resp
}

pub fn palette_bar(ui: &mut Ui, colors: &[Rgb], height: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    paint_gradient(ui.painter(), rect, colors, 0.0);
    lit_edge(ui.painter(), rect);
    resp
}

pub fn paint_gradient(p: &Painter, rect: Rect, colors: &[Rgb], _rounding: f32) {
    if colors.is_empty() {
        p.rect_filled(rect, 0.0, SURFACE2);
        return;
    }
    let steps = (colors.len() * 10).clamp(12, 96);
    let mut m = Mesh::default();
    for i in 0..=steps {
        let u = i as f32 / steps as f32;
        let c = c32(lumen_engine::sample_linear(colors, u));
        let x = rect.left() + rect.width() * u;
        m.colored_vertex(pos2(x, rect.top()), c);
        m.colored_vertex(pos2(x, rect.bottom()), c);
        if i > 0 {
            let b = (i as u32 - 1) * 2;
            m.add_triangle(b, b + 1, b + 2);
            m.add_triangle(b + 1, b + 3, b + 2);
        }
    }
    p.add(Shape::mesh(m));
}

pub fn paint_gradient_v(p: &Painter, rect: Rect, colors: &[Rgb]) {
    if colors.is_empty() {
        return;
    }
    let steps = (colors.len() * 8).clamp(8, 48);
    let mut m = Mesh::default();
    for i in 0..=steps {
        let u = i as f32 / steps as f32;
        let c = c32(lumen_engine::sample_linear(colors, u));
        let y = rect.top() + rect.height() * u;
        m.colored_vertex(pos2(rect.left(), y), c);
        m.colored_vertex(pos2(rect.right(), y), c);
        if i > 0 {
            let b = (i as u32 - 1) * 2;
            m.add_triangle(b, b + 1, b + 2);
            m.add_triangle(b + 1, b + 3, b + 2);
        }
    }
    p.add(Shape::mesh(m));
}

pub fn bri_slider_at(ui: &mut Ui, rect: Rect, id: egui::Id, value: f32, enabled: bool) -> (Response, Option<f32>) {
    let rect = Rect::from_center_size(rect.center(), vec2(rect.width(), HIT));
    let resp = ui.interact(rect, id, if enabled { Sense::click_and_drag() } else { Sense::hover() });
    let resp = resp.on_hover_cursor(if enabled { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Default });
    let mut out = None;
    if enabled && (resp.dragged() || resp.clicked()) {
        if let Some(p) = resp.interact_pointer_pos() {
            out = Some(((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
        }
    }
    let v = out.unwrap_or(value).clamp(0.0, 1.0);
    let hov = ease_bool(ui, id.with("kh"), enabled && (resp.hovered() || resp.dragged()), 0.15);
    let p = ui.painter();
    let track = Rect::from_min_size(pos2(rect.left(), rect.top() + 17.0), vec2(rect.width(), 6.0));
    p.rect_filled(track, 0.0, SURFACE3);
    for f in [0.25, 0.5, 0.75] {
        let x = rect.left() + rect.width() * f;
        p.rect_filled(Rect::from_min_size(pos2(x, rect.top() + 26.0), vec2(1.0, 5.0)), 0.0, EDGE);
    }
    if enabled {
        p.rect_filled(Rect::from_min_size(track.min, vec2(track.width() * v, 6.0)), 0.0, TEXT);
        let kx = rect.left() + rect.width() * v;
        let kh = 22.0 * (1.0 + 0.25 * hov);
        let knob = Rect::from_center_size(pos2(kx, rect.top() + 20.0), vec2(6.0, kh));
        p.rect_filled(knob.expand(2.0), 0.0, OPENED);
        p.rect_filled(knob, 0.0, TEXT);
    }
    if resp.has_focus() {
        focus_ring(p, rect);
    }
    (resp, out)
}

pub fn light_tile_sized(ui: &mut Ui, name: &str, colors: &[Rgb], on: bool, bri: f32, selected: bool, w: f32) -> Response {
    let size = Vec2::new(w, 98.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let hov = ease_bool(ui, resp.id.with("h"), resp.hovered(), 0.15);
    let rect = if resp.is_pointer_button_down_on() { rect.translate(vec2(0.0, 1.0)) } else { rect };
    let p = ui.painter();
    p.rect_filled(rect, 0.0, mix(SURFACE, SURFACE2, hov));
    let band = Rect::from_min_size(rect.min, vec2(rect.width(), 52.0));
    let lit = on && colors.iter().any(|c| c.max_channel() > 0.02);
    if lit {
        if colors.len() > 1 {
            paint_gradient(p, band, colors, 0.0);
        } else {
            p.rect_filled(band, 0.0, c32(colors[0]));
        }
        lit_edge(p, band);
    } else {
        hatch(p, band, Color32::from_rgb(26, 26, 30));
    }
    let nm = truncate(name.trim(), ((rect.width() - 20.0) / 7.2) as usize);
    p.text(pos2(rect.left() + 10.0, rect.top() + 60.0), Align2::LEFT_TOP, nm, semibold(13.5), TEXT);
    if lit {
        num_unit(p, pos2(rect.left() + 10.0, rect.top() + 79.0), Align2::LEFT_TOP, &format!("{:.0}", bri * 100.0), "%", 15.0, SOFT);
    } else {
        text_at(p, pos2(rect.left() + 10.0, rect.top() + 79.0), Align2::LEFT_TOP, "OFF", display(15.0), SOFT, 0.0);
    }
    if selected {
        brackets(p, rect, ACCENT, 0.0, 2.0);
    }
    resp.on_hover_text(name.trim())
}

pub fn row_cursor(ui: &Ui, id: egui::Id, row: Rect, hovered: bool) {
    let t = ease_bool(ui, id.with("cur"), hovered, 0.2);
    if t <= 0.0 {
        return;
    }
    let full = row.shrink2(vec2(0.0, 10.0));
    let h = full.height() * t;
    let r = Rect::from_center_size(pos2(row.left() - 12.5, full.center().y), vec2(3.0, h));
    ui.painter().rect_filled(r, 0.0, ACCENT);
}

pub fn text_input<'t>(text: &'t mut dyn egui::TextBuffer, hint: &str) -> egui::TextEdit<'t> {
    egui::TextEdit::singleline(text)
        .hint_text(RichText::new(hint).color(MUTED))
        .font(body(14.5))
        .frame(true)
        .margin(vec2(12.0, 12.0))
        .text_color(TEXT)
}

pub fn labelled_switch(ui: &mut Ui, label: &str, on: bool, height: f32) -> Response {
    let font = display_bold(16.0);
    let g = ui.painter().layout_job(spaced(&label.to_uppercase(), font.clone(), Color32::PLACEHOLDER, 1.0));
    let inset = 4.0;
    let sw = vec2(64.0, height - inset * 2.0);
    let size = vec2(20.0 + g.size().x + 16.0 + sw.x + inset, height);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let t = ease_bool(ui, resp.id.with("wipe"), on, 0.22);
    let p = ui.painter();
    p.rect_filled(rect, 0.0, if resp.hovered() { SURFACE3 } else { SURFACE2 });
    text_at(p, pos2(rect.left() + 20.0, rect.center().y), Align2::LEFT_CENTER, &label.to_uppercase(), font, TEXT, 1.0);
    let s = Rect::from_min_size(pos2(rect.right() - inset - sw.x, rect.top() + inset), sw);
    let s = if resp.is_pointer_button_down_on() { Rect::from_center_size(s.center(), s.size() * 0.96) } else { s };
    p.rect_filled(s, 0.0, INK);
    if t > 0.0 {
        p.rect_filled(Rect::from_min_size(s.min, vec2(s.width() * t, s.height())), 0.0, GOOD);
    }
    text_at(p, s.center(), Align2::CENTER_CENTER, if on { "ON" } else { "OFF" }, display_bold(16.0), if t > 0.5 { INK } else { SOFT }, 1.0);
    if resp.has_focus() {
        focus_ring(p, rect);
    }
    resp
}
