use autopara_core::models::{
    Lesson, STATUS_MANUAL, STATUS_MISSED, STATUS_OPENED, STATUS_SKIPPED,
};
use autopara_core::normalize::{add_minutes, minutes_between, subject_color, week_start};
use chrono::{Datelike, Duration, Local, NaiveDate, Timelike};
use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Pos2, Rect, RichText, Sense, Stroke, StrokeKind,
    Ui, pos2, vec2,
};
use std::collections::HashMap;

use crate::anim::MorphState;
use crate::i18n::{Lang, ProviderKind, Tr, days_full, detect_display_provider};
use crate::modals::{primary_button, secondary_button};
use crate::palette::{Palette, blend, parse_hex};

const HOUR_HEIGHT: f32 = 82.0;
const GUTTER_WIDTH: f32 = 64.0;
const MIN_COL_WIDTH: f32 = 170.0;

fn layout_clamped(
    painter: &egui::Painter,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
    max_rows: usize,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_string(),
        egui::TextFormat::simple(font, color),
    );
    job.wrap = egui::text::TextWrapping {
        max_width,
        max_rows: max_rows.max(1),
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    painter.layout_job(job)
}

fn semibold_font(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name("semibold".into()))
}

#[derive(Debug, Clone)]
pub struct DragState {
    pub lesson_id: i64,
    pub subject: String,
    pub from_day: NaiveDate,
    pub duration_mins: i64,
}

#[derive(Debug, Clone)]
pub enum ContextPopup {
    Lesson {
        pos: Pos2,
        lesson_id: i64,
        day: NaiveDate,
        has_url: bool,
        has_status: bool,
    },
    EmptySlot {
        pos: Pos2,
        day: NaiveDate,
        start_time: String,
    },
}

pub enum CalendarAction {
    None,
    OpenLesson { lesson_id: i64, day: NaiveDate },
    EditLesson { lesson_id: i64, day: NaiveDate },
    CreateAt { day: NaiveDate, start_time: Option<String> },
    MarkLesson { lesson_id: i64, day: NaiveDate, status: String },
    ClearMark { lesson_id: i64, day: NaiveDate },
    RequestDelete { lesson_id: i64 },
    DropLesson {
        lesson_id: i64,
        from_day: NaiveDate,
        to_day: NaiveDate,
        start_time: String,
        end_time: String,
    },
    OpenImport,
}

fn hhmm_minutes(s: &str) -> i64 {
    let mut parts = s.split(':');
    let h = parts.next().and_then(|v| v.parse::<i64>().ok()).unwrap_or(8);
    let m = parts.next().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    h * 60 + m
}

fn chip_colors(kind: ProviderKind, p: Palette) -> (Color32, Color32) {
    match kind {
        ProviderKind::Zoom => (p.zoom_bg, p.zoom_fg),
        ProviderKind::Meet => (p.meet_bg, p.meet_fg),
        ProviderKind::Teams => (p.teams_bg, p.teams_fg),
        ProviderKind::Discord => (p.discord_bg, p.discord_fg),
        ProviderKind::Link => (p.chip_bg, p.text_muted),
    }
}

fn draw_chip(
    painter: &egui::Painter,
    x: &mut f32,
    y: f32,
    max_x: f32,
    text: &str,
    bg: Color32,
    fg: Color32,
) {
    let galley = painter.layout_no_wrap(text.to_string(), semibold_font(10.5), fg);
    let w = galley.size().x + 14.0;
    if *x + w > max_x {
        return;
    }
    let rect = Rect::from_min_size(pos2(*x, y), vec2(w, 19.0));
    painter.rect_filled(rect, CornerRadius::same(6), bg);
    painter.galley(
        pos2(rect.left() + 7.0, rect.center().y - galley.size().y * 0.5),
        galley,
        fg,
    );
    *x += w + 6.0;
}

#[allow(clippy::too_many_arguments)]
pub fn show_calendar(
    ui: &mut Ui,
    anchor: NaiveDate,
    lessons: &[Lesson],
    total_workspace_lessons: usize,
    occurrences: &HashMap<(i64, String), String>,
    next_up_id: Option<i64>,
    drag: &mut Option<DragState>,
    popup: &mut Option<ContextPopup>,
    morph: &mut MorphState,
    p: Palette,
    lang: Lang,
) -> CalendarAction {
    let mut action = CalendarAction::None;
    let (cal_t, cal_anim) = morph.calendar_progress();
    if cal_anim {
        ui.ctx().request_repaint();
    }

    let frame_rect = Frame::NONE
        .fill(p.surface)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(14))
        // 1px keeps the grid off the border line; corners are masked below
        .inner_margin(egui::Margin::same(1))
        .show(ui, |ui| {
            let avail = ui.available_size();
            ui.set_min_size(avail);

            action = show_week_view(
                ui,
                anchor,
                lessons,
                total_workspace_lessons,
                occurrences,
                next_up_id,
                drag,
                popup,
                morph,
                cal_t,
                p,
                lang,
            );
        })
        .response
        .rect;
    mask_round_corners(ui.painter(), frame_rect, 14.0, p);

    // Floating ghost while dragging a card
    if let Some(d) = drag.as_ref()
        && let Some(ptr) = ui.ctx().pointer_latest_pos()
    {
        let ghost_rect = Rect::from_min_size(ptr + vec2(10.0, 10.0), vec2(170.0, 32.0));
        let layer = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("drag_ghost"),
        ));
        layer.rect(
            ghost_rect,
            CornerRadius::same(8),
            p.accent_fill,
            Stroke::NONE,
            StrokeKind::Inside,
        );
        layer.text(
            pos2(ghost_rect.left() + 10.0, ghost_rect.center().y),
            Align2::LEFT_CENTER,
            &d.subject,
            FontId::proportional(12.0),
            p.accent_text,
        );
    }

    // Release drag outside targets cleanly
    if drag.is_some() && !ui.ctx().input(|i| i.pointer.primary_down()) {
        *drag = None;
    }

    // Context popup menu (lesson actions or create-at-slot)
    if let Some(ctx_popup) = popup.clone() {
        let (pop_t, pop_anim) = morph.popup_progress();
        if pop_anim {
            ui.ctx().request_repaint();
        }
        let pop_action = show_context_popup(ui.ctx(), &ctx_popup, popup, morph, pop_t, p, lang);
        if !matches!(pop_action, CalendarAction::None) {
            action = pop_action;
        }
    }

    action
}

/// egui clips to rectangles, so the grid (e.g. today's column) can fill the
/// rounded corners of its frame. Paint the canvas back over them and redraw the border.
fn mask_round_corners(painter: &egui::Painter, r: Rect, rad: f32, p: Palette) {
    for (c, sx, sy) in [
        (r.left_top(), 1.0, 1.0),
        (r.right_top(), -1.0, 1.0),
        (r.left_bottom(), 1.0, -1.0),
        (r.right_bottom(), -1.0, -1.0),
    ] {
        let center = c + vec2(sx * rad, sy * rad);
        // fan from the square corner to the arc: exactly the area outside the rounding
        let mut pts = vec![c];
        pts.extend((0..=8).map(|i| {
            let a = i as f32 / 8.0 * std::f32::consts::FRAC_PI_2;
            center + vec2(-sx * rad * a.cos(), -sy * rad * a.sin())
        }));
        painter.add(egui::Shape::convex_polygon(pts, p.canvas, Stroke::NONE));
    }
    painter.rect_stroke(
        r,
        CornerRadius::same(rad as u8),
        Stroke::new(1.0_f32, p.border),
        StrokeKind::Inside,
    );
}

#[allow(clippy::too_many_arguments)]
fn show_week_view(
    ui: &mut Ui,
    anchor: NaiveDate,
    lessons: &[Lesson],
    total_workspace_lessons: usize,
    occurrences: &HashMap<(i64, String), String>,
    next_up_id: Option<i64>,
    drag: &mut Option<DragState>,
    popup: &mut Option<ContextPopup>,
    morph: &mut MorphState,
    cal_t: f32,
    p: Palette,
    lang: Lang,
) -> CalendarAction {
    let tr = Tr::new(lang);
    let mut action = CalendarAction::None;
    let today = Local::now().date_naive();

    let mon = week_start(anchor);
    let sun = mon + Duration::days(6);
    let has_sun = lessons.iter().any(|l| l.occurs_on(sun));
    let count = if has_sun { 7 } else { 6 };
    let days: Vec<NaiveDate> = (0..count).map(|i| mon + Duration::days(i)).collect();

    // Visible hour range: always 08:00..23:00 (expands earlier if morning calls exist before 08:00)
    let mut min_hour: i64 = 8;
    let max_hour: i64 = 24;
    for l in lessons {
        let sh = hhmm_minutes(&l.start_time) / 60;
        if sh < min_hour {
            min_hour = sh.max(0);
        }
    }
    let hour_count = (max_hour - min_hour).max(1) as usize;

    let avail_w = ui.available_width();
    let min_grid_w = GUTTER_WIDTH + days.len() as f32 * MIN_COL_WIDTH;
    let total_w = avail_w.max(min_grid_w);
    let col_w = ((total_w - GUTTER_WIDTH) / days.len() as f32).max(MIN_COL_WIDTH);

    // Scrollable time grid (both axes when narrower than min_grid_w, so columns never crush cards)
    let pointer_pos = ui.ctx().pointer_latest_pos();
    let released = drag.is_some() && !ui.ctx().input(|i| i.pointer.primary_down());

    egui::ScrollArea::both()
        .id_salt("week_grid_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Zero vertical spacing so day header and time grid connect seamlessly with no gap.
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

            // Day header row aligned with scrollable columns (generous vertical centering)
            let header_height = 64.0;
            let (header_rect, _) = ui.allocate_exact_size(
                vec2(total_w, header_height),
                Sense::hover(),
            );
            let hp = ui.painter();
            hp.line_segment(
                [
                    pos2(header_rect.left(), header_rect.bottom()),
                    pos2(header_rect.right(), header_rect.bottom()),
                ],
                Stroke::new(1.0_f32, p.border_soft),
            );

            let wd_names = days_full(lang);
            for (idx, &day) in days.iter().enumerate() {
                let x0 = header_rect.left() + GUTTER_WIDTH + idx as f32 * col_w;
                let col_rect =
                    Rect::from_min_size(pos2(x0, header_rect.top()), vec2(col_w, header_height));
                let is_today = day == today;
                if is_today {
                    hp.rect_filled(col_rect, 0.0, p.today_bg);
                }
                let wd = day.weekday().num_days_from_monday() as usize;
                hp.text(
                    pos2(col_rect.center().x, col_rect.top() + 18.0),
                    Align2::CENTER_CENTER,
                    wd_names[wd],
                    semibold_font(12.0),
                    if is_today { p.text } else { p.text_muted },
                );
                let circle_center = pos2(col_rect.center().x, col_rect.top() + 43.0);
                if is_today {
                    hp.circle_filled(circle_center, 14.0, p.accent_fill);
                }
                hp.text(
                    circle_center,
                    Align2::CENTER_CENTER,
                    day.day().to_string(),
                    semibold_font(13.5),
                    if is_today { p.accent_text } else { p.text },
                );
            }

            let total_h = hour_count as f32 * HOUR_HEIGHT;
            let (grid_rect, _) = ui.allocate_exact_size(vec2(total_w, total_h), Sense::hover());

            // Background columns + hour lines + empty-slot clicks/drops
            for (c_idx, &day) in days.iter().enumerate() {
                let x0 = grid_rect.left() + GUTTER_WIDTH + c_idx as f32 * col_w;
                let col_rect = Rect::from_min_size(pos2(x0, grid_rect.top()), vec2(col_w, total_h));
                if day == today {
                    ui.painter().rect_filled(col_rect, 0.0, p.today_bg);
                }
                ui.painter().line_segment(
                    [pos2(x0, grid_rect.top()), pos2(x0, grid_rect.bottom())],
                    Stroke::new(1.0_f32, p.border_soft),
                );

                for h_idx in 0..hour_count {
                    let y0 = grid_rect.top() + h_idx as f32 * HOUR_HEIGHT;
                    let slot_rect = Rect::from_min_size(pos2(x0, y0), vec2(col_w, HOUR_HEIGHT));
                    let slot_resp = ui.interact(
                        slot_rect,
                        ui.id().with(("slot", c_idx, h_idx)),
                        Sense::click(),
                    );
                    let hovered_drag = drag.is_some()
                        && pointer_pos.is_some_and(|pt| slot_rect.contains(pt));
                    let slot_key = 0x5107_0000_u64 ^ ((c_idx as u64) << 8) ^ (h_idx as u64);
                    let slot_hov = morph.smooth_hover(
                        ui.ctx(),
                        slot_key,
                        slot_resp.hovered() && drag.is_none(),
                    );
                    if hovered_drag {
                        ui.painter().rect(
                            slot_rect.shrink(1.0),
                            CornerRadius::same(6),
                            p.accent_soft,
                            Stroke::new(1.2_f32, p.accent),
                            StrokeKind::Inside,
                        );
                        if released && let Some(d) = drag.take() {
                            let new_start = format!("{:02}:00", min_hour + h_idx as i64);
                            let new_end = add_minutes(&new_start, d.duration_mins);
                            action = CalendarAction::DropLesson {
                                lesson_id: d.lesson_id,
                                from_day: d.from_day,
                                to_day: day,
                                start_time: new_start,
                                end_time: new_end,
                            };
                        }
                    } else if slot_hov > 0.01 {
                        ui.painter().rect_filled(
                            slot_rect.shrink(2.0),
                            CornerRadius::same(8),
                            p.accent_soft.linear_multiply(slot_hov),
                        );
                        ui.painter().text(
                            slot_rect.center(),
                            Align2::CENTER_CENTER,
                            "+",
                            FontId::proportional(16.0),
                            p.text_faint.linear_multiply(slot_hov * 0.85),
                        );
                    }

                    if slot_resp.clicked() {
                        let pos = slot_resp.interact_pointer_pos().unwrap_or(slot_rect.center());
                        morph.begin_popup();
                        *popup = Some(ContextPopup::EmptySlot {
                            pos,
                            day,
                            start_time: format!("{:02}:00", min_hour + h_idx as i64),
                        });
                    }
                }
            }

            // Horizontal hour divider lines & left time gutter labels
            for h_idx in 0..hour_count {
                let y = grid_rect.top() + h_idx as f32 * HOUR_HEIGHT;
                let hour = min_hour + h_idx as i64;
                if h_idx > 0 {
                    ui.painter().line_segment(
                        [
                            pos2(grid_rect.left() + GUTTER_WIDTH, y),
                            pos2(grid_rect.right(), y),
                        ],
                        Stroke::new(1.0_f32, p.border_soft),
                    );
                }
                ui.painter().text(
                    pos2(grid_rect.left() + GUTTER_WIDTH - 10.0, y + 8.0),
                    Align2::RIGHT_CENTER,
                    format!("{hour:02}:00"),
                    FontId::proportional(11.0),
                    p.text_faint,
                );
            }

            // Draw lesson cards
            for (c_idx, &day) in days.iter().enumerate() {
                let x0 = grid_rect.left() + GUTTER_WIDTH + c_idx as f32 * col_w;
                let day_iso = day.format("%Y-%m-%d").to_string();
                let mut day_lessons: Vec<&Lesson> =
                    lessons.iter().filter(|l| l.occurs_on(day)).collect();
                day_lessons.sort_by(|a, b| a.start_time.cmp(&b.start_time));

                for lesson in day_lessons {
                    let start_m = hhmm_minutes(&lesson.start_time);
                    let end_m = hhmm_minutes(&lesson.end_time).max(start_m + 30);
                    let top_px = ((start_m - min_hour * 60) as f32 / 60.0 * HOUR_HEIGHT).max(0.0);
                    let height_px =
                        (((end_m - start_m) as f32 / 60.0) * HOUR_HEIGHT - 6.0).max(46.0);
                    let target_rect = Rect::from_min_size(
                        pos2(x0 + 6.0, grid_rect.top() + top_px + 3.0),
                        vec2(col_w - 12.0, height_px),
                    );
                    let (card_rect, card_alpha) = morph.morph_card_rect(lesson.id, target_rect, cal_t);

                    let status = occurrences
                        .get(&(lesson.id, day_iso.clone()))
                        .map(String::as_str);
                    let is_next = day == today && next_up_id == Some(lesson.id);
                    let card_resp = draw_lesson_card(
                        ui,
                        card_rect,
                        card_alpha,
                        lesson,
                        status,
                        is_next,
                        morph,
                        p,
                        lang,
                        ("card", lesson.id, c_idx),
                    );

                    if card_resp.drag_started() {
                        *drag = Some(DragState {
                            lesson_id: lesson.id,
                            subject: lesson.subject.clone(),
                            from_day: day,
                            duration_mins: minutes_between(&lesson.start_time, &lesson.end_time).max(15),
                        });
                    } else if card_resp.clicked() {
                        if lesson.url.as_ref().is_some_and(|u| !u.is_empty()) {
                            action = CalendarAction::OpenLesson {
                                lesson_id: lesson.id,
                                day,
                            };
                        } else {
                            let pos = card_resp.interact_pointer_pos().unwrap_or(card_rect.center());
                            morph.begin_popup();
                            *popup = Some(ContextPopup::Lesson {
                                pos,
                                lesson_id: lesson.id,
                                day,
                                has_url: false,
                                has_status: status.is_some(),
                            });
                        }
                    } else if card_resp.secondary_clicked() {
                        let pos = card_resp.interact_pointer_pos().unwrap_or(card_rect.center());
                        morph.begin_popup();
                        *popup = Some(ContextPopup::Lesson {
                            pos,
                            lesson_id: lesson.id,
                            day,
                            has_url: lesson.url.as_ref().is_some_and(|u| !u.is_empty()),
                            has_status: status.is_some(),
                        });
                    }
                }
            }

            // Live "now" horizontal line across today's column
            if let Some(today_idx) = days.iter().position(|&d| d == today) {
                let now = Local::now();
                let now_mins = i64::from(now.hour()) * 60 + i64::from(now.minute());
                let offset = now_mins - min_hour * 60;
                if offset >= 0 && offset <= (hour_count as i64) * 60 {
                    let y = grid_rect.top() + (offset as f32 / 60.0) * HOUR_HEIGHT;
                    let x0 = grid_rect.left() + GUTTER_WIDTH + today_idx as f32 * col_w;
                    ui.painter().line_segment(
                        [pos2(x0, y), pos2(x0 + col_w, y)],
                        Stroke::new(1.6_f32, p.accent),
                    );
                    let pill_rect = Rect::from_center_size(
                        pos2(grid_rect.left() + GUTTER_WIDTH * 0.5, y),
                        vec2(46.0, 20.0),
                    );
                    ui.painter()
                        .rect_filled(pill_rect, CornerRadius::same(10), p.accent_soft);
                    ui.painter().text(
                        pill_rect.center(),
                        Align2::CENTER_CENTER,
                        format!("{:02}:{:02}", now.hour(), now.minute()),
                        semibold_font(10.5),
                        p.text,
                    );
                }
            }

            // Unobtrusive empty-calendar hint when user starts from 0 with no calls yet
            if total_workspace_lessons == 0 {
                let hint_rect = Rect::from_center_size(
                    pos2(grid_rect.center().x, grid_rect.top() + 210.0),
                    vec2(370.0, 148.0),
                );
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(hint_rect), |ui| {
                    Frame::NONE
                        .fill(p.surface)
                        .stroke(Stroke::new(1.0_f32, p.border))
                        .corner_radius(CornerRadius::same(14))
                        .inner_margin(egui::Margin::same(18))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new(tr.pick(
                                        "Поки немає запланованих созвонів",
                                        "No calls scheduled yet",
                                    ))
                                    .size(15.0)
                                    .strong(),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(tr.pick(
                                        "Натисніть на будь-яку вільну годину або «+», щоб додати посилання на созвон.",
                                        "Click any empty hour slot or press “+” to add a meeting link.",
                                    ))
                                    .size(12.5)
                                    .color(p.text_muted),
                                );
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    ui.add_space(crate::modals::center_pad(
                                        ui,
                                        &[
                                            tr.pick("Додати созвон", "Add call"),
                                            tr.pick("Імпорт .docx", "Import .docx"),
                                        ],
                                        8.0,
                                    ));
                                    if ui
                                        .add(primary_button(
                                            tr.pick("Додати созвон", "Add call"),
                                            p,
                                        ))
                                        .clicked()
                                    {
                                        action = CalendarAction::CreateAt {
                                            day: anchor,
                                            start_time: None,
                                        };
                                    }
                                    ui.add_space(8.0);
                                    if ui
                                        .add(secondary_button(
                                            tr.pick("Імпорт .docx", "Import .docx"),
                                            p,
                                        ))
                                        .clicked()
                                    {
                                        action = CalendarAction::OpenImport;
                                    }
                                });
                            });
                        });
                });
            }
        });

    action
}

#[allow(clippy::too_many_arguments)]
fn draw_lesson_card(
    ui: &mut Ui,
    rect: Rect,
    alpha: f32,
    lesson: &Lesson,
    status: Option<&str>,
    is_next: bool,
    morph: &mut MorphState,
    p: Palette,
    lang: Lang,
    id_salt: (&str, i64, usize),
) -> egui::Response {
    let tr = Tr::new(lang);
    let resp = ui.interact(rect, ui.id().with(id_salt), Sense::click_and_drag());
    let hover_key = ((lesson.id as u64) << 8) ^ (id_salt.2 as u64);
    let hover_t = morph.smooth_hover(ui.ctx(), hover_key, resp.hovered());
    let press_t = morph.smooth_press(ui.ctx(), hover_key, resp.is_pointer_button_down_on());
    let subj_col = parse_hex(subject_color(&lesson.subject));

    // Tactile hover lift + gentle press inset morph.
    let lift_y = -1.6 * hover_t + 1.8 * press_t;
    let shrink = -0.7 * hover_t + 1.6 * press_t;
    let rect = rect.translate(vec2(0.0, lift_y)).shrink(shrink);

    let (fill, base_border, border_w, bar_col) = match status {
        Some(STATUS_OPENED) | Some(STATUS_MANUAL) => (p.opened_bg, p.border, 1.0_f32, subj_col),
        Some(STATUS_MISSED) => (p.missed_bg, p.danger, 1.2_f32, p.danger),
        Some(STATUS_SKIPPED) => (p.skipped_bg, p.border, 1.0_f32, subj_col),
        _ if lesson.needs_link || lesson.url.as_ref().is_none_or(|u| u.is_empty()) => {
            (p.nolink_bg, p.border, 1.0_f32, subj_col)
        }
        _ => (
            blend(p.surface, subj_col, p.tint_alpha + hover_t * 0.06 - press_t * 0.03),
            if is_next { p.accent } else { p.border },
            if is_next { 1.5_f32 } else { 1.0_f32 },
            subj_col,
        ),
    };

    let border_col = blend(base_border, p.accent, hover_t * 0.85);
    let fill = fill.linear_multiply(alpha);
    let border_col = border_col.linear_multiply(alpha);
    let bar_col = bar_col.linear_multiply(alpha);
    let painter = ui.painter().with_clip_rect(rect);

    painter.rect(
        rect,
        CornerRadius::same(11),
        fill,
        Stroke::new(border_w, border_col),
        StrokeKind::Inside,
    );

    // Accent bar morphs subtly wider on hover (3.0 -> 4.2px)
    let bar_w = 3.0 + hover_t * 1.2;
    let bar_rect = Rect::from_min_max(
        pos2(rect.left() + 9.0, rect.top() + 10.0),
        pos2(rect.left() + 9.0 + bar_w, rect.bottom() - 10.0),
    );
    painter.rect_filled(bar_rect, CornerRadius::same(2), bar_col);

    // Chips row with airy margins
    let content_x = rect.left() + 20.0;
    let max_x = rect.right() - 10.0;
    let mut cx = content_x;
    let cy = rect.top() + 9.0;

    match status {
        Some(STATUS_OPENED) | Some(STATUS_MANUAL) => {
            draw_chip(&painter, &mut cx, cy, max_x, tr.pick("✓ відкрито", "✓ opened"), p.success_bg, p.success_fg);
        }
        Some(STATUS_MISSED) => {
            draw_chip(&painter, &mut cx, cy, max_x, tr.pick("не відкрито", "missed"), p.danger_bg, p.danger_fg);
        }
        Some(STATUS_SKIPPED) => {
            draw_chip(&painter, &mut cx, cy, max_x, tr.pick("пропущено", "skipped"), p.chip_bg, p.text_muted);
        }
        _ if lesson.needs_link || lesson.url.as_ref().is_none_or(|u| u.is_empty()) => {
            draw_chip(&painter, &mut cx, cy, max_x, tr.pick("без посилання", "no link"), p.warn_bg, p.warn_fg);
        }
        _ => {}
    }

    if let Some(url) = lesson.url.as_deref().filter(|u| !u.is_empty()) {
        let (kind, lbl, _) = detect_display_provider(url, lang);
        let (cbg, cfg) = chip_colors(kind, p);
        draw_chip(&painter, &mut cx, cy, max_x, lbl, cbg, cfg);
    }

    if lesson.group_names.len() > 1 {
        let badge = tr.groups_badge(lesson.group_names.len());
        draw_chip(&painter, &mut cx, cy, max_x, &badge, p.chip_bg, p.text_muted);
    }

    // Subject + teacher + time: airy spacing & strict whole-line clamping with ellipsis
    let text_w = (max_x - content_x).max(24.0);
    let subj_y = cy + 25.0;
    let bottom_limit = rect.bottom() - 9.0;
    let avail_h = (bottom_limit - subj_y).max(0.0);
    let has_teacher = !lesson.teacher.is_empty();

    let (subj_rows, show_teacher, show_time) = if avail_h >= 66.0 {
        let extra_rows = if avail_h >= 104.0 { 3 } else { 2 };
        (extra_rows, has_teacher, true)
    } else if avail_h >= 48.0 {
        (2, has_teacher, !has_teacher)
    } else if avail_h >= 34.0 {
        (2, false, false)
    } else {
        (1, false, false)
    };

    let subj_galley = layout_clamped(
        &painter,
        &lesson.subject,
        semibold_font(12.8),
        p.text.linear_multiply(alpha),
        text_w,
        subj_rows,
    );
    painter.galley(pos2(content_x, subj_y), subj_galley.clone(), p.text);

    let mut cursor_y = subj_y + subj_galley.size().y + 4.0;

    if show_teacher && cursor_y + 14.0 <= bottom_limit {
        let tg = layout_clamped(
            &painter,
            &lesson.teacher,
            FontId::proportional(11.5),
            p.text_muted.linear_multiply(alpha),
            text_w,
            1,
        );
        painter.galley(pos2(content_x, cursor_y), tg.clone(), p.text_muted);
        cursor_y += tg.size().y + 3.0;
    }

    if show_time && cursor_y + 13.0 <= bottom_limit {
        let time_y = (bottom_limit - 13.5).max(cursor_y);
        if time_y + 12.0 <= rect.bottom() - 6.0 {
            painter.text(
                pos2(content_x, time_y),
                Align2::LEFT_TOP,
                lesson.time_range(),
                FontId::proportional(11.0),
                p.text_faint.linear_multiply(alpha),
            );
        }
    }

    // Rich hover tooltip with full details
    resp.on_hover_ui(|ui| {
        ui.set_max_width(300.0);
        ui.label(RichText::new(&lesson.subject).strong().size(13.0));
        if !lesson.teacher.is_empty() {
            ui.label(RichText::new(&lesson.teacher).size(12.0).color(p.text_muted));
        }
        ui.label(RichText::new(lesson.time_range()).size(12.0));
        if lesson.group_names.len() > 1 {
            ui.label(
                RichText::new(lesson.group_names.join(", "))
                    .size(11.5)
                    .color(p.text_muted),
            );
        }
        if let Some(url) = lesson.url.as_deref().filter(|u| !u.is_empty()) {
            ui.label(RichText::new(url).size(11.0).color(p.text_faint));
        }
    })
}

fn show_context_popup(
    ctx: &egui::Context,
    current: &ContextPopup,
    popup_slot: &mut Option<ContextPopup>,
    morph: &mut MorphState,
    pop_t: f32,
    p: Palette,
    lang: Lang,
) -> CalendarAction {
    let tr = Tr::new(lang);
    let mut action = CalendarAction::None;

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *popup_slot = None;
        return action;
    }

    let pos = match current {
        ContextPopup::Lesson { pos, .. } | ContextPopup::EmptySlot { pos, .. } => *pos,
    };
    let screen = ctx.screen_rect();
    let clamped = pos2(
        pos.x.min(screen.right() - 240.0).max(8.0),
        pos.y.min(screen.bottom() - 220.0).max(8.0) + (1.0 - pop_t) * 10.0,
    );

    let area_resp = egui::Area::new(egui::Id::new("autopara_ctx_popup"))
        .fixed_pos(clamped)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.multiply_opacity(pop_t);
            Frame::NONE
                .fill(p.surface)
                .stroke(Stroke::new(1.0_f32, p.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_min_width(210.0);
                    let mut item_idx: u64 = 0;
                    let mut menu_item = |ui: &mut Ui, label: &str, danger: bool| -> bool {
                        item_idx += 1;
                        let key = 0xC07E_0000_u64 + item_idx;
                        let (rect, resp) = ui.allocate_exact_size(
                            vec2(ui.available_width().max(200.0), 32.0),
                            Sense::click(),
                        );
                        let hov = morph.smooth_hover(ctx, key, resp.hovered());
                        let prs = morph.smooth_press(ctx, key, resp.is_pointer_button_down_on());
                        if hov > 0.01 {
                            let bg = if danger { p.danger_bg } else { p.accent_soft };
                            ui.painter().rect_filled(
                                rect.shrink(prs * 1.2),
                                CornerRadius::same(7),
                                bg.linear_multiply(hov),
                            );
                        }
                        ui.painter().text(
                            pos2(rect.left() + 10.0 + hov * 2.5, rect.center().y),
                            Align2::LEFT_CENTER,
                            label,
                            FontId::proportional(13.0),
                            if danger { p.danger_fg } else { p.text },
                        );
                        resp.clicked()
                    };

                    match current {
                        ContextPopup::EmptySlot { day, start_time, .. } => {
                            let lbl = tr.create_at(*day, start_time);
                            if menu_item(ui, &lbl, false) {
                                action = CalendarAction::CreateAt {
                                    day: *day,
                                    start_time: Some(start_time.clone()),
                                };
                                *popup_slot = None;
                            }
                        }
                        ContextPopup::Lesson {
                            lesson_id,
                            day,
                            has_url,
                            has_status,
                            ..
                        } => {
                            if *has_url {
                                if menu_item(ui, tr.pick("Відкрити посилання", "Open link"), false) {
                                    action = CalendarAction::OpenLesson {
                                        lesson_id: *lesson_id,
                                        day: *day,
                                    };
                                    *popup_slot = None;
                                }
                            } else if menu_item(ui, tr.pick("Додати посилання…", "Add link…"), false) {
                                action = CalendarAction::EditLesson {
                                    lesson_id: *lesson_id,
                                    day: *day,
                                };
                                *popup_slot = None;
                            }

                            if menu_item(ui, tr.pick("Редагувати…", "Edit…"), false) {
                                action = CalendarAction::EditLesson {
                                    lesson_id: *lesson_id,
                                    day: *day,
                                };
                                *popup_slot = None;
                            }

                            ui.separator();

                            if menu_item(ui, tr.pick("Позначити як відкриту", "Mark as opened"), false) {
                                action = CalendarAction::MarkLesson {
                                    lesson_id: *lesson_id,
                                    day: *day,
                                    status: STATUS_OPENED.to_string(),
                                };
                                *popup_slot = None;
                            }
                            if menu_item(ui, tr.pick("Позначити як пропущену", "Mark as skipped"), false) {
                                action = CalendarAction::MarkLesson {
                                    lesson_id: *lesson_id,
                                    day: *day,
                                    status: STATUS_SKIPPED.to_string(),
                                };
                                *popup_slot = None;
                            }
                            if *has_status
                                && menu_item(ui, tr.pick("Зняти позначку", "Clear mark"), false)
                            {
                                action = CalendarAction::ClearMark {
                                    lesson_id: *lesson_id,
                                    day: *day,
                                };
                                *popup_slot = None;
                            }

                            ui.separator();

                            if menu_item(ui, tr.pick("Видалити", "Delete"), true) {
                                action = CalendarAction::RequestDelete {
                                    lesson_id: *lesson_id,
                                };
                                *popup_slot = None;
                            }
                        }
                    }
                });
        });

    if ctx.input(|i| i.pointer.any_pressed())
        && let Some(pt) = ctx.pointer_latest_pos()
        && !area_resp.response.rect.contains(pt)
    {
        *popup_slot = None;
    }

    action
}
