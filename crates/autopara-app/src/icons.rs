use egui::{Color32, Painter, Pos2, Rect, Stroke, pos2, vec2};
use std::f32::consts::TAU;

pub fn draw_plus(p: &Painter, center: Pos2, r: f32, color: Color32, width: f32) {
    let stroke = Stroke::new(width, color);
    p.line_segment([pos2(center.x - r, center.y), pos2(center.x + r, center.y)], stroke);
    p.line_segment([pos2(center.x, center.y - r), pos2(center.x, center.y + r)], stroke);
}

/// `square.and.arrow.up` — arrow pointing up, matching AutoPara's import glyph.
pub fn draw_import(p: &Painter, c: Pos2, s: f32, color: Color32) {
    let st = Stroke::new(1.8_f32, color);
    // Tray bottom U-shape
    let pts = [
        pos2(c.x - s * 0.85, c.y + s * 0.25),
        pos2(c.x - s * 0.85, c.y + s * 0.85),
        pos2(c.x + s * 0.85, c.y + s * 0.85),
        pos2(c.x + s * 0.85, c.y + s * 0.25),
    ];
    p.add(egui::Shape::line(pts.to_vec(), st));
    // Vertical shaft
    p.line_segment([pos2(c.x, c.y + s * 0.35), pos2(c.x, c.y - s * 0.85)], st);
    // Arrowhead pointing up
    p.add(egui::Shape::line(
        vec![
            pos2(c.x - s * 0.45, c.y - s * 0.38),
            pos2(c.x, c.y - s * 0.85),
            pos2(c.x + s * 0.45, c.y - s * 0.38),
        ],
        st,
    ));
}

pub fn draw_groups(p: &Painter, c: Pos2, s: f32, color: Color32) {
    let st = Stroke::new(1.7_f32, color);
    // Primary person head + shoulders
    p.circle_stroke(pos2(c.x - s * 0.22, c.y - s * 0.38), s * 0.32, st);
    let arc1 = [
        pos2(c.x - s * 0.88, c.y + s * 0.78),
        pos2(c.x - s * 0.72, c.y + s * 0.28),
        pos2(c.x - s * 0.22, c.y + s * 0.16),
        pos2(c.x + s * 0.28, c.y + s * 0.28),
        pos2(c.x + s * 0.42, c.y + s * 0.78),
    ];
    p.add(egui::Shape::line(arc1.to_vec(), st));
    // Secondary person hint on right
    p.circle_stroke(pos2(c.x + s * 0.48, c.y - s * 0.28), s * 0.22, st);
    p.add(egui::Shape::line(
        vec![
            pos2(c.x + s * 0.50, c.y + s * 0.22),
            pos2(c.x + s * 0.82, c.y + s * 0.36),
            pos2(c.x + s * 0.92, c.y + s * 0.76),
        ],
        st,
    ));
}

pub fn draw_gear(p: &Painter, c: Pos2, s: f32, color: Color32) {
    let st = Stroke::new(1.7_f32, color);
    p.circle_stroke(c, s * 0.34, st);
    p.circle_stroke(c, s * 0.68, st);
    for i in 0..8 {
        let angle = (i as f32) * (TAU / 8.0);
        let dir = vec2(angle.cos(), angle.sin());
        p.line_segment([c + dir * (s * 0.68), c + dir * (s * 0.96)], st);
    }
}

pub fn draw_chevron(p: &Painter, c: Pos2, s: f32, right: bool, color: Color32) {
    let st = Stroke::new(1.8_f32, color);
    let dx = if right { s * 0.35 } else { -s * 0.35 };
    p.add(egui::Shape::line(
        vec![
            pos2(c.x - dx, c.y - s * 0.62),
            pos2(c.x + dx, c.y),
            pos2(c.x - dx, c.y + s * 0.62),
        ],
        st,
    ));
}

pub fn draw_chevron_down(p: &Painter, c: Pos2, s: f32, open_t: f32, color: Color32) {
    let st = Stroke::new(1.6_f32, color);
    let flip = 1.0 - 2.0 * open_t;
    p.add(egui::Shape::line(
        vec![
            pos2(c.x - s * 0.68, c.y - s * 0.25 * flip),
            pos2(c.x, c.y + s * 0.38 * flip),
            pos2(c.x + s * 0.68, c.y - s * 0.25 * flip),
        ],
        st,
    ));
}

pub fn draw_search(p: &Painter, c: Pos2, s: f32, color: Color32) {
    let st = Stroke::new(1.7_f32, color);
    let lens = pos2(c.x - s * 0.15, c.y - s * 0.15);
    p.circle_stroke(lens, s * 0.56, st);
    p.line_segment(
        [
            lens + vec2(s * 0.40, s * 0.40),
            pos2(c.x + s * 0.78, c.y + s * 0.78),
        ],
        st,
    );
}

pub fn draw_document(p: &Painter, rect: Rect, color: Color32) {
    let st = Stroke::new(1.8_f32, color);
    let fold = rect.width() * 0.32;
    let pts = [
        pos2(rect.left(), rect.top()),
        pos2(rect.right() - fold, rect.top()),
        pos2(rect.right(), rect.top() + fold),
        pos2(rect.right(), rect.bottom()),
        pos2(rect.left(), rect.bottom()),
        pos2(rect.left(), rect.top()),
    ];
    p.add(egui::Shape::line(pts.to_vec(), st));
    p.add(egui::Shape::line(
        vec![
            pos2(rect.right() - fold, rect.top()),
            pos2(rect.right() - fold, rect.top() + fold),
            pos2(rect.right(), rect.top() + fold),
        ],
        st,
    ));
}

pub fn draw_win_minimize(p: &Painter, c: Pos2, color: Color32) {
    let st = Stroke::new(1.7_f32, color);
    p.line_segment([pos2(c.x - 5.5, c.y), pos2(c.x + 5.5, c.y)], st);
}

pub fn draw_win_maximize(p: &Painter, c: Pos2, color: Color32) {
    let r = Rect::from_center_size(c, vec2(10.0, 10.0));
    p.rect_stroke(
        r,
        egui::CornerRadius::same(2),
        Stroke::new(1.7_f32, color),
        egui::StrokeKind::Inside,
    );
}

pub fn draw_win_close(p: &Painter, c: Pos2, color: Color32) {
    let st = Stroke::new(1.8_f32, color);
    p.line_segment([pos2(c.x - 4.8, c.y - 4.8), pos2(c.x + 4.8, c.y + 4.8)], st);
    p.line_segment([pos2(c.x + 4.8, c.y - 4.8), pos2(c.x - 4.8, c.y + 4.8)], st);
}
