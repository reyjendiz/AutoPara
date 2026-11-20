use egui::{Rect, pos2, vec2};
use std::collections::HashMap;
use std::time::Instant;

pub const MORPH_SECS: f32 = 0.34;
pub const MODAL_SECS: f32 = 0.22;
pub const POPUP_SECS: f32 = 0.18;

/// Quintic ease-out: silky fast launch with an ultra-smooth, gentle landing.
pub fn ease_out_quint(t: f32) -> f32 {
    let clamped = t.clamp(0.0, 1.0);
    let inv = 1.0 - clamped;
    1.0 - inv * inv * inv * inv * inv
}

/// Exponential frame-rate-independent damper (never linear/robotic).
pub fn damp(current: f32, target: f32, speed: f32, dt: f32) -> (f32, bool) {
    let alpha = 1.0 - (-speed * dt.clamp(0.001, 0.05)).exp();
    let next = current + (target - current) * alpha;
    if (target - next).abs() < 0.003 {
        (target, false)
    } else {
        (next, true)
    }
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::from_min_max(
        pos2(lerp(a.min.x, b.min.x, t), lerp(a.min.y, b.min.y, t)),
        pos2(lerp(a.max.x, b.max.x, t), lerp(a.max.y, b.max.y, t)),
    )
}

pub fn scale_rect_center(rect: Rect, scale: f32) -> Rect {
    Rect::from_center_size(rect.center(), rect.size() * scale)
}

pub struct MorphState {
    pub prev_rects: HashMap<i64, Rect>,
    pub curr_rects: HashMap<i64, Rect>,
    pub transition_start: Option<Instant>,
    /// -1 = back in time, +1 = forward, 0 = view switch (Day/Week/Month)
    pub direction: i8,
    pub modal_opened: Option<Instant>,
    pub popup_opened: Option<Instant>,
    /// Smoothly damped hover values keyed by string/id hash.
    pub hovers: HashMap<u64, f32>,
    last_frame: Instant,
    pub dt: f32,
}

impl Default for MorphState {
    fn default() -> Self {
        Self::new()
    }
}

impl MorphState {
    pub fn new() -> Self {
        Self {
            prev_rects: HashMap::new(),
            curr_rects: HashMap::new(),
            transition_start: None,
            direction: 0,
            modal_opened: None,
            popup_opened: None,
            hovers: HashMap::new(),
            last_frame: Instant::now(),
            dt: 1.0 / 60.0,
        }
    }

    pub fn tick_frame(&mut self) {
        let now = Instant::now();
        self.dt = now
            .saturating_duration_since(self.last_frame)
            .as_secs_f32()
            .clamp(0.001, 0.05);
        self.last_frame = now;
    }

    pub fn smooth_val(&mut self, ctx: &egui::Context, key: u64, target: f32, speed: f32) -> f32 {
        let cur = self.hovers.get(&key).copied().unwrap_or(0.0);
        let (next, moving) = damp(cur, target, speed, self.dt);
        self.hovers.insert(key, next);
        if moving {
            ctx.request_repaint();
        }
        next
    }

    pub fn smooth_hover(&mut self, ctx: &egui::Context, key: u64, hovered: bool) -> f32 {
        self.smooth_val(ctx, key, if hovered { 1.0 } else { 0.0 }, 16.0)
    }

    pub fn smooth_press(&mut self, ctx: &egui::Context, key: u64, pressed: bool) -> f32 {
        // Disjoint key-space so press and hover never collide for the same widget ID.
        self.smooth_val(ctx, key ^ 0xA5A5_5A5A_F00D_1234, if pressed { 1.0 } else { 0.0 }, 26.0)
    }

    pub fn begin_calendar_morph(&mut self, direction: i8) {
        self.prev_rects = self.curr_rects.clone();
        self.curr_rects.clear();
        self.direction = direction;
        self.transition_start = Some(Instant::now());
    }

    pub fn begin_modal(&mut self) {
        self.modal_opened = Some(Instant::now());
    }

    pub fn begin_popup(&mut self) {
        self.popup_opened = Some(Instant::now());
    }

    pub fn calendar_progress(&mut self) -> (f32, bool) {
        let Some(started) = self.transition_start else {
            return (1.0, false);
        };
        let raw = started.elapsed().as_secs_f32() / MORPH_SECS;
        if raw >= 1.0 {
            self.transition_start = None;
            self.prev_rects.clear();
            (1.0, false)
        } else {
            (ease_out_quint(raw), true)
        }
    }

    pub fn modal_progress(&self) -> (f32, bool) {
        let Some(started) = self.modal_opened else {
            return (1.0, false);
        };
        let raw = started.elapsed().as_secs_f32() / MODAL_SECS;
        if raw >= 1.0 {
            (1.0, false)
        } else {
            (ease_out_quint(raw), true)
        }
    }

    pub fn popup_progress(&self) -> (f32, bool) {
        let Some(started) = self.popup_opened else {
            return (1.0, false);
        };
        let raw = started.elapsed().as_secs_f32() / POPUP_SECS;
        if raw >= 1.0 {
            (1.0, false)
        } else {
            (ease_out_quint(raw), true)
        }
    }

    pub fn morph_card_rect(&mut self, lesson_id: i64, target: Rect, t: f32) -> (Rect, f32) {
        self.curr_rects.insert(lesson_id, target);
        if t >= 1.0 {
            return (target, 1.0);
        }
        if let Some(&old) = self.prev_rects.get(&lesson_id) {
            (lerp_rect(old, target, t), 1.0)
        } else {
            let drift_x = (self.direction as f32) * 24.0 * (1.0 - t);
            let shifted = target.translate(vec2(drift_x, 0.0));
            let scaled = scale_rect_center(shifted, lerp(0.94, 1.0, t));
            (scaled, t)
        }
    }
}
