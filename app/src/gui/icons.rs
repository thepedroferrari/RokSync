//! Line icons painted with egui shapes, so they scale and take any colour.

use eframe::egui::{pos2, Color32, Painter, Pos2, Rect, Shape, Stroke};
use std::f32::consts::{PI, TAU};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Lights,
    Loadouts,
    Live,
    Games,
    Vibe,
    System,
    Play,
    Stop,
    Lock,
    Unlock,
    Close,
    Plus,
    Minus,
    Refresh,
    Trash,
    Copy,
    Left,
    Right,
    Bolt,
    Power,
    Screen,
    Swap,
    Save,
    Search,
    Check,
    Edit,
    Arrow,
    Shield,
    Dice,
    Undo,
}

fn arc(c: Pos2, r: f32, a0: f32, a1: f32, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height());
    let r = Rect::from_center_size(rect.center(), eframe::egui::vec2(s, s));
    let u = |x: f32, y: f32| pos2(r.left() + x * s, r.top() + y * s);
    let w = (s * 0.1).max(1.4);
    let st = Stroke::new(w, color);
    let line = |pts: Vec<Pos2>| p.add(Shape::line(pts, st));
    let closed = |pts: Vec<Pos2>| p.add(Shape::closed_line(pts, st));
    let seg = |a: Pos2, b: Pos2| p.line_segment([a, b], st);
    match icon {
        Icon::Lights => {
            p.circle_stroke(u(0.5, 0.4), s * 0.27, st);
            seg(u(0.38, 0.78), u(0.62, 0.78));
            seg(u(0.42, 0.92), u(0.58, 0.92));
            seg(u(0.5, 0.4), u(0.5, 0.52));
        }
        Icon::Loadouts => {
            closed(vec![u(0.1, 0.22), u(0.9, 0.22), u(0.9, 0.78), u(0.1, 0.78)]);
            seg(u(0.37, 0.22), u(0.37, 0.78));
            seg(u(0.63, 0.22), u(0.63, 0.78));
        }
        Icon::Live => {
            p.circle_filled(u(0.5, 0.5), s * 0.09, color);
            for (k, rad) in [(0.0, 0.22), (0.0, 0.4)] {
                let _ = k;
                line(arc(u(0.5, 0.5), s * rad, -0.75, 0.75, 10));
                line(arc(u(0.5, 0.5), s * rad, PI - 0.75, PI + 0.75, 10));
            }
        }
        Icon::Games => {
            closed(vec![u(0.18, 0.32), u(0.82, 0.32), u(0.96, 0.74), u(0.8, 0.8), u(0.66, 0.64), u(0.34, 0.64), u(0.2, 0.8), u(0.04, 0.74)]);
            seg(u(0.22, 0.48), u(0.38, 0.48));
            seg(u(0.3, 0.4), u(0.3, 0.56));
            p.circle_filled(u(0.66, 0.44), s * 0.05, color);
            p.circle_filled(u(0.76, 0.52), s * 0.05, color);
        }
        Icon::Vibe => {
            closed(vec![u(0.44, 0.1), u(0.53, 0.43), u(0.86, 0.52), u(0.53, 0.61), u(0.44, 0.94), u(0.35, 0.61), u(0.02, 0.52), u(0.35, 0.43)]);
            seg(u(0.82, 0.06), u(0.82, 0.3));
            seg(u(0.7, 0.18), u(0.94, 0.18));
        }
        Icon::System => {
            for (y, x) in [(0.25, 0.32), (0.5, 0.68), (0.75, 0.44)] {
                seg(u(0.1, y), u(0.9, y));
                p.circle_filled(u(x, y), s * 0.1, color);
            }
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(vec![u(0.26, 0.16), u(0.84, 0.5), u(0.26, 0.84)], color, Stroke::NONE));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_min_max(u(0.22, 0.22), u(0.78, 0.78)), 1.5, color);
        }
        Icon::Lock | Icon::Unlock => {
            p.rect_filled(Rect::from_min_max(u(0.2, 0.46), u(0.8, 0.9)), 2.0, color);
            let end = if icon == Icon::Lock { TAU } else { TAU - 0.9 };
            let mut pts = vec![u(0.32, 0.46)];
            pts.extend(arc(u(0.5, 0.32), s * 0.18, PI, end, 12));
            if icon == Icon::Lock {
                pts.push(u(0.68, 0.46));
            }
            line(pts);
        }
        Icon::Close => {
            seg(u(0.22, 0.22), u(0.78, 0.78));
            seg(u(0.78, 0.22), u(0.22, 0.78));
        }
        Icon::Plus => {
            seg(u(0.5, 0.16), u(0.5, 0.84));
            seg(u(0.16, 0.5), u(0.84, 0.5));
        }
        Icon::Minus => {
            seg(u(0.16, 0.5), u(0.84, 0.5));
        }
        Icon::Refresh | Icon::Undo => {
            let pts = arc(u(0.5, 0.52), s * 0.32, -PI * 0.35, PI * 1.4, 18);
            let tip = if icon == Icon::Refresh { *pts.first().unwrap() } else { *pts.last().unwrap() };
            line(pts);
            seg(tip, tip + eframe::egui::vec2(s * 0.2, -s * 0.02));
            seg(tip, tip + eframe::egui::vec2(s * 0.02, s * 0.2));
        }
        Icon::Trash => {
            seg(u(0.16, 0.26), u(0.84, 0.26));
            line(vec![u(0.4, 0.26), u(0.4, 0.14), u(0.6, 0.14), u(0.6, 0.26)]);
            closed(vec![u(0.26, 0.32), u(0.74, 0.32), u(0.68, 0.9), u(0.32, 0.9)]);
        }
        Icon::Copy => {
            closed(vec![u(0.32, 0.32), u(0.88, 0.32), u(0.88, 0.88), u(0.32, 0.88)]);
            line(vec![u(0.68, 0.14), u(0.12, 0.14), u(0.12, 0.68)]);
        }
        Icon::Left => {
            line(vec![u(0.64, 0.18), u(0.32, 0.5), u(0.64, 0.82)]);
        }
        Icon::Right => {
            line(vec![u(0.36, 0.18), u(0.68, 0.5), u(0.36, 0.82)]);
        }
        Icon::Arrow => {
            seg(u(0.12, 0.5), u(0.86, 0.5));
            line(vec![u(0.58, 0.22), u(0.86, 0.5), u(0.58, 0.78)]);
        }
        Icon::Bolt => {
            p.add(Shape::convex_polygon(vec![u(0.6, 0.04), u(0.2, 0.58), u(0.5, 0.58)], color, Stroke::NONE));
            p.add(Shape::convex_polygon(vec![u(0.5, 0.42), u(0.8, 0.42), u(0.4, 0.96)], color, Stroke::NONE));
            p.add(Shape::convex_polygon(vec![u(0.46, 0.42), u(0.56, 0.42), u(0.54, 0.58), u(0.44, 0.58)], color, Stroke::NONE));
        }
        Icon::Power => {
            line(arc(u(0.5, 0.55), s * 0.34, -PI * 0.3, PI * 1.3, 20));
            seg(u(0.5, 0.08), u(0.5, 0.5));
        }
        Icon::Screen => {
            closed(vec![u(0.08, 0.16), u(0.92, 0.16), u(0.92, 0.7), u(0.08, 0.7)]);
            seg(u(0.5, 0.7), u(0.5, 0.84));
            seg(u(0.3, 0.86), u(0.7, 0.86));
        }
        Icon::Swap => {
            seg(u(0.12, 0.34), u(0.86, 0.34));
            line(vec![u(0.66, 0.16), u(0.86, 0.34), u(0.66, 0.52)]);
            seg(u(0.88, 0.68), u(0.14, 0.68));
            line(vec![u(0.34, 0.5), u(0.14, 0.68), u(0.34, 0.86)]);
        }
        Icon::Save => {
            seg(u(0.5, 0.1), u(0.5, 0.62));
            line(vec![u(0.28, 0.42), u(0.5, 0.64), u(0.72, 0.42)]);
            line(vec![u(0.12, 0.66), u(0.12, 0.88), u(0.88, 0.88), u(0.88, 0.66)]);
        }
        Icon::Search => {
            p.circle_stroke(u(0.42, 0.42), s * 0.28, st);
            seg(u(0.63, 0.63), u(0.9, 0.9));
        }
        Icon::Check => {
            line(vec![u(0.14, 0.52), u(0.4, 0.78), u(0.88, 0.24)]);
        }
        Icon::Edit => {
            seg(u(0.2, 0.8), u(0.76, 0.24));
            seg(u(0.66, 0.14), u(0.86, 0.34));
            seg(u(0.14, 0.88), u(0.32, 0.84));
        }
        Icon::Shield => {
            closed(vec![u(0.5, 0.08), u(0.86, 0.2), u(0.82, 0.56), u(0.5, 0.92), u(0.18, 0.56), u(0.14, 0.2)]);
            line(vec![u(0.34, 0.5), u(0.46, 0.62), u(0.68, 0.38)]);
        }
        Icon::Dice => {
            closed(vec![u(0.12, 0.12), u(0.88, 0.12), u(0.88, 0.88), u(0.12, 0.88)]);
            for (x, y) in [(0.32, 0.32), (0.5, 0.5), (0.68, 0.68)] {
                p.circle_filled(u(x, y), s * 0.07, color);
            }
        }
    }
}
