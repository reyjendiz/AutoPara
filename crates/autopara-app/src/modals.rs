use autopara_core::models::{
    CATCHUP_MISSED, CATCHUP_NOTIFY, CATCHUP_OPEN, Course, Group, LANG_EN, LANG_UK, Lesson,
    PERSONAL_COURSE_ORDINAL, Settings, UPDATE_ASK, UPDATE_AUTO, UPDATE_OFF, VIEW_LIST,
};
use autopara_core::normalize::{add_minutes, minutes_between, parse_time};
use autopara_core::schedule_parser::ParsedCourse;
use autopara_core::updater::CURRENT_VERSION;
use chrono::{Datelike, Duration, Local, NaiveDate};
use egui::{Align, Button, Color32, ComboBox, CornerRadius, Frame, Layout, RichText, Stroke, Ui, vec2};

use crate::gcal_sync::GcalStatus;
use crate::i18n::{Lang, Tr, days_full, detect_display_provider, format_short_date};
use crate::icons;
use crate::palette::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatKind {
    Once,
    Until,
    Weekly,
}

#[derive(Debug, Clone)]
pub struct EditModal {
    pub lesson_id: Option<i64>,
    pub is_imported: bool,
    pub subject: String,
    pub teacher: String,
    pub url: String,
    pub repeat: RepeatKind,
    pub on_date_str: String,
    pub until_str: String,
    pub day_index: i64,
    pub start_time: String,
    pub end_time: String,
    pub auto_end: bool,
    pub default_duration: i64,
    pub error: Option<String>,
}

impl EditModal {
    pub fn new_call(day: NaiveDate, start_time: Option<&str>, default_duration: i64) -> Self {
        let start = start_time
            .map(String::from)
            .unwrap_or_else(|| {
                let h = (chrono::Timelike::hour(&Local::now()) + 1).clamp(8, 22);
                format!("{h:02}:00")
            });
        let end = add_minutes(&start, default_duration);
        let wd = i64::from(day.weekday().num_days_from_monday());
        Self {
            lesson_id: None,
            is_imported: false,
            subject: String::new(),
            teacher: String::new(),
            url: String::new(),
            repeat: RepeatKind::Once,
            on_date_str: day.format("%Y-%m-%d").to_string(),
            until_str: (day + Duration::days(28)).format("%Y-%m-%d").to_string(),
            day_index: wd,
            start_time: start,
            end_time: end,
            auto_end: true,
            default_duration,
            error: None,
        }
    }

    pub fn from_lesson(lesson: &Lesson, fallback_day: NaiveDate, default_duration: i64) -> Self {
        let on_date = lesson.on_date.unwrap_or(fallback_day);
        let until = lesson
            .repeat_until
            .unwrap_or_else(|| on_date + Duration::days(28));
        let repeat = if !lesson.is_manual || lesson.on_date.is_none() {
            RepeatKind::Weekly
        } else if lesson.repeat_until.is_some() {
            RepeatKind::Until
        } else {
            RepeatKind::Once
        };
        Self {
            lesson_id: Some(lesson.id),
            is_imported: !lesson.is_manual,
            subject: lesson.subject.clone(),
            teacher: lesson.teacher.clone(),
            url: lesson.url.clone().unwrap_or_default(),
            repeat,
            on_date_str: on_date.format("%Y-%m-%d").to_string(),
            until_str: until.format("%Y-%m-%d").to_string(),
            day_index: lesson.day_index,
            start_time: lesson.start_time.clone(),
            end_time: lesson.end_time.clone(),
            auto_end: false,
            default_duration,
            error: None,
        }
    }
}

pub struct EditResult {
    pub lesson_id: Option<i64>,
    pub subject: String,
    pub teacher: String,
    pub url: Option<String>,
    pub start_time: String,
    pub end_time: String,
    pub on_date: Option<NaiveDate>,
    pub repeat_until: Option<NaiveDate>,
    pub day_index: i64,
}

#[derive(Debug, Clone)]
pub struct SettingsModal {
    pub lead_minutes: i64,
    pub class_duration_minutes: i64,
    pub language: String,
    pub notifications_enabled: bool,
    pub notify_minutes: i64,
    pub catchup_mode: String,
    pub autostart_enabled: bool,
    pub update_mode: String,
    pub gcal_auto_open: bool,
    /// Show upcoming classes and calls as a list instead of the week grid
    pub list_view: bool,
    /// "Delete timetable" was pressed once and waits for "yes"
    pub confirm_delete_timetable: bool,
}

impl SettingsModal {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            lead_minutes: s.lead_minutes,
            class_duration_minutes: s.class_duration_minutes,
            language: s.language.clone(),
            notifications_enabled: s.notifications_enabled,
            notify_minutes: s.notify_minutes,
            catchup_mode: s.catchup_mode.clone(),
            autostart_enabled: s.autostart_enabled,
            update_mode: s.update_mode.clone(),
            gcal_auto_open: s.gcal_auto_open,
            list_view: s.view_mode == VIEW_LIST,
            confirm_delete_timetable: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GroupModal {
    pub selected_course_id: i64,
    pub selected_group_id: i64,
}

#[derive(Debug, Clone)]
pub struct ImportModal {
    pub path: Option<String>,
    pub courses: Vec<ParsedCourse>,
    pub selected_ordinal: Option<i64>,
    pub selected_group: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MoveScopeModal {
    pub lesson_id: i64,
    pub subject: String,
    pub from_day: NaiveDate,
    pub to_day: NaiveDate,
    pub start_time: String,
    pub end_time: String,
}

#[derive(Debug, Clone)]
pub struct DeleteConfirmModal {
    pub lesson_id: i64,
    pub subject: String,
    pub is_series: bool,
}

pub enum ModalOutcome {
    None,
    Close,
    SaveEdit(EditResult),
    SaveSettings(SettingsModal),
    CheckUpdatesNow,
    GcalSignIn,
    DeleteTimetable,
    GcalSignOut,
    SelectGroup { course_id: i64, group_id: i64 },
    PickDocxForImport,
    CommitImport { path: String, ordinal: i64, group_name: String },
    ConfirmMove { modal: MoveScopeModal, only_one: bool },
    ConfirmDelete(i64),
}

pub fn primary_button(text: &str, p: Palette) -> Button<'_> {
    Button::new(RichText::new(text).color(p.accent_text).strong().size(13.5))
        .fill(p.accent_fill)
        .stroke(Stroke::new(1.0_f32, p.accent_fill))
        .corner_radius(CornerRadius::same(18))
        .min_size(vec2(0.0, 36.0))
}

/// Left padding that centres a row of our 13.5pt buttons separated by `gap`.
pub fn center_pad(ui: &egui::Ui, labels: &[&str], gap: f32) -> f32 {
    let pad_x = ui.spacing().button_padding.x;
    let row: f32 = labels
        .iter()
        .map(|s| {
            let g = ui.painter().layout_no_wrap(
                (*s).to_owned(),
                egui::FontId::proportional(13.5),
                Color32::WHITE,
            );
            g.size().x + 2.0 * pad_x
        })
        .sum::<f32>()
        + (gap + ui.spacing().item_spacing.x) * (labels.len().saturating_sub(1)) as f32;
    ((ui.available_width() - row) * 0.5).max(0.0)
}

pub fn secondary_button(text: &str, p: Palette) -> Button<'_> {
    Button::new(RichText::new(text).color(p.text).strong().size(13.5))
        .fill(p.surface)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(18))
        .min_size(vec2(0.0, 36.0))
}

pub fn plain_button(text: &str, p: Palette) -> Button<'_> {
    Button::new(RichText::new(text).color(p.text_muted).strong().size(13.5))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(18))
        .min_size(vec2(0.0, 36.0))
}

pub fn danger_button(text: &str, p: Palette) -> Button<'_> {
    Button::new(RichText::new(text).color(Color32::WHITE).strong().size(13.5))
        .fill(p.danger)
        .stroke(Stroke::new(1.0_f32, p.danger))
        .corner_radius(CornerRadius::same(18))
        .min_size(vec2(0.0, 36.0))
}

fn modal_dismissed_id() -> egui::Id {
    egui::Id::new("modal_dismissed_by_backdrop")
}

/// True once per click on the dimmed area around a dialog. Read by the app after the
/// dialog is drawn and treated as "Cancel".
pub fn take_backdrop_click(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.remove_temp::<bool>(modal_dismissed_id()))
        .unwrap_or(false)
}

fn draw_backdrop(ctx: &egui::Context, t: f32) {
    let screen = ctx.screen_rect();
    let alpha = (165.0_f32 * t).round() as u8;
    egui::Area::new(egui::Id::new("modal_backdrop"))
        .fixed_pos(screen.min)
        .order(egui::Order::Middle)
        .interactable(true)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(alpha));
            // the dialog and its dropdowns sit on higher layers, so only a click on the
            // dimmed area itself lands here
            if resp.clicked() {
                ctx.data_mut(|d| d.insert_temp(modal_dismissed_id(), true));
            }
        });
}

fn show_modal_frame(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    p: Palette,
    t: f32,
    add_contents: impl FnOnce(&mut Ui),
) {
    draw_backdrop(ctx, t);
    let y_shift = (1.0 - t) * 14.0;
    // fit the window: 16px gap above/below + 24px inner margin on each side
    let max_h = (ctx.screen_rect().height() - 2.0 * (16.0 + 24.0)).max(120.0);
    // in a narrow window (the list view) leave ~48px of window on each side
    let width = width.min(ctx.screen_rect().width() - 2.0 * (48.0 + 24.0)).max(300.0);
    egui::Area::new(egui::Id::new(id))
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, y_shift))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.multiply_opacity(t);
            Frame::NONE
                .fill(p.surface)
                .stroke(Stroke::new(1.0_f32, p.border))
                .corner_radius(CornerRadius::same(16))
                .inner_margin(egui::Margin::same(24))
                .show(ui, |ui| {
                    ui.set_width(width);
                    // solid bar takes its own strip instead of covering the right-aligned buttons
                    ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                    egui::ScrollArea::vertical()
                        .id_salt(id)
                        .max_height(max_h)
                        // the Area only offers last frame's size; without this it stays stuck small
                        .min_scrolled_height(max_h)
                        .show(ui, add_contents);
                });
        });
}

fn modal_footer(ui: &mut Ui, add_buttons: impl FnOnce(&mut Ui)) {
    ui.add_space(6.0);
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 38.0),
        Layout::right_to_left(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            add_buttons(ui);
        },
    );
}

/// Two-option pill switch (as for the interface language). Returns the clicked option.
fn segmented(ui: &mut Ui, id: &str, labels: [&str; 2], selected: usize, p: Palette) -> Option<usize> {
    let (seg_rect, _) = ui.allocate_exact_size(vec2(224.0, 34.0), egui::Sense::hover());
    ui.painter().rect(
        seg_rect,
        CornerRadius::same(17),
        p.sunken,
        Stroke::new(1.0_f32, p.border),
        egui::StrokeKind::Inside,
    );
    let half_w = (seg_rect.width() - 6.0) * 0.5;
    let mut clicked = None;
    for (i, title) in labels.iter().enumerate() {
        let r = egui::Rect::from_min_size(
            egui::pos2(seg_rect.left() + 3.0 + i as f32 * half_w, seg_rect.top() + 3.0),
            vec2(half_w, 28.0),
        );
        let resp = ui.interact(r, ui.id().with((id, i)), egui::Sense::click());
        let active = selected == i;
        let bg = if active {
            p.accent_fill
        } else if resp.hovered() {
            p.accent_soft
        } else {
            Color32::TRANSPARENT
        };
        let fg = if active { p.accent_text } else { p.text_muted };
        ui.painter().rect_filled(r, CornerRadius::same(14), bg);
        ui.painter().text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            *title,
            egui::FontId::new(12.5, egui::FontFamily::Name("semibold".into())),
            fg,
        );
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

fn form_row(ui: &mut Ui, label: &str, add_field: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 34.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.allocate_ui_with_layout(
                vec2(148.0, 34.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.label(RichText::new(label).size(13.0));
                },
            );
            add_field(ui);
        },
    );
}

pub fn show_edit_modal(
    ctx: &egui::Context,
    modal: &mut EditModal,
    p: Palette,
    lang: Lang,
    t: f32,
) -> ModalOutcome {
    let tr = Tr::new(lang);
    let mut outcome = ModalOutcome::None;

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return ModalOutcome::Close;
    }

    show_modal_frame(ctx, "edit_modal", 480.0, p, t, |ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 12.0);
        let title = if modal.lesson_id.is_some() {
            tr.pick("Редагувати", "Edit event")
        } else {
            tr.pick("Додати созвон / пару", "Add call / class")
        };
        ui.label(RichText::new(title).size(20.0).strong());
        ui.add_space(2.0);

        form_row(ui, tr.pick("Назва / Предмет", "Title / Subject"), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut modal.subject)
                    .hint_text(tr.pick("Наприклад: Дейлі стендап", "e.g. Daily standup"))
                    .desired_width(280.0)
                    .margin(vec2(10.0, 8.0)),
            );
        });

        form_row(ui, tr.pick("Організатор / Викл.", "Host / Teacher"), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut modal.teacher)
                    .hint_text(tr.pick("Необов'язково", "Optional"))
                    .desired_width(280.0)
                    .margin(vec2(10.0, 8.0)),
            );
        });

        if !modal.is_imported {
            form_row(ui, tr.pick("Повторення", "Repeat"), |ui| {
                let current_lbl = match modal.repeat {
                    RepeatKind::Once => tr.pick("Один раз", "Once"),
                    RepeatKind::Until => tr.pick("Щотижня до дати", "Weekly until date"),
                    RepeatKind::Weekly => tr.pick("Щотижня, без кінця", "Weekly, forever"),
                };
                ComboBox::from_id_salt("edit_repeat")
                    .selected_text(current_lbl)
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut modal.repeat, RepeatKind::Once, tr.pick("Один раз", "Once"));
                        ui.selectable_value(
                            &mut modal.repeat,
                            RepeatKind::Until,
                            tr.pick("Щотижня до дати", "Weekly until date"),
                        );
                        ui.selectable_value(
                            &mut modal.repeat,
                            RepeatKind::Weekly,
                            tr.pick("Щотижня, без кінця", "Weekly, forever"),
                        );
                    });
            });
        }

        if modal.repeat == RepeatKind::Weekly {
            form_row(ui, tr.pick("День", "Weekday"), |ui| {
                let days = days_full(lang);
                let idx = modal.day_index.rem_euclid(7) as usize;
                ComboBox::from_id_salt("edit_weekday")
                    .selected_text(days[idx])
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        for (i, &name) in days.iter().enumerate() {
                            ui.selectable_value(&mut modal.day_index, i as i64, name);
                        }
                    });
            });
        } else {
            form_row(ui, tr.pick("Дата", "Date"), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut modal.on_date_str)
                        .hint_text("YYYY-MM-DD")
                        .desired_width(150.0)
                        .margin(vec2(10.0, 8.0)),
                );
                if ui.add(secondary_button("-1", p)).clicked()
                    && let Ok(d) = NaiveDate::parse_from_str(&modal.on_date_str, "%Y-%m-%d")
                {
                    modal.on_date_str = (d - Duration::days(1)).format("%Y-%m-%d").to_string();
                }
                if ui.add(secondary_button("+1", p)).clicked()
                    && let Ok(d) = NaiveDate::parse_from_str(&modal.on_date_str, "%Y-%m-%d")
                {
                    modal.on_date_str = (d + Duration::days(1)).format("%Y-%m-%d").to_string();
                }
            });
            if modal.repeat == RepeatKind::Until {
                form_row(ui, tr.pick("До дати", "Until"), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut modal.until_str)
                            .hint_text("YYYY-MM-DD")
                            .desired_width(150.0)
                            .margin(vec2(10.0, 8.0)),
                    );
                    if ui.add(secondary_button("+7", p)).clicked()
                        && let Ok(d) = NaiveDate::parse_from_str(&modal.until_str, "%Y-%m-%d")
                    {
                        modal.until_str = (d + Duration::days(7)).format("%Y-%m-%d").to_string();
                    }
                });
            }
        }

        form_row(ui, tr.pick("Час", "Time"), |ui| {
            let s_resp = ui.add(
                egui::TextEdit::singleline(&mut modal.start_time)
                    .hint_text("09:30")
                    .desired_width(120.0)
                    .margin(vec2(10.0, 8.0)),
            );
            if s_resp.changed()
                && modal.auto_end
                && let Some(norm) = parse_time(&modal.start_time)
            {
                modal.end_time = add_minutes(&norm, modal.default_duration);
            }
            ui.label("–");
            let e_resp = ui.add(
                egui::TextEdit::singleline(&mut modal.end_time)
                    .hint_text("10:50")
                    .desired_width(120.0)
                    .margin(vec2(10.0, 8.0)),
            );
            if e_resp.changed() {
                modal.auto_end = false;
            }
        });

        form_row(ui, tr.pick("Посилання", "Meeting link"), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut modal.url)
                    .hint_text("https://meet.google.com/... / https://zoom.us/...")
                    .desired_width(280.0)
                    .margin(vec2(10.0, 8.0)),
            );
        });

        // Live summary (repeat + duration + detected provider)
        let mut meta = Vec::new();
        match modal.repeat {
            RepeatKind::Once => {
                if let Ok(d) = NaiveDate::parse_from_str(&modal.on_date_str, "%Y-%m-%d") {
                    let s = format_short_date(d, lang);
                    meta.push(if lang.is_en() {
                        format!("One-off event on {s}.")
                    } else {
                        format!("Лише один раз: {s}.")
                    });
                }
            }
            RepeatKind::Weekly => {
                let wd = days_full(lang)[modal.day_index.rem_euclid(7) as usize];
                meta.push(if lang.is_en() {
                    format!("Every {wd} until deleted.")
                } else {
                    format!("Щотижня ({wd}), доки не видалено.")
                });
            }
            RepeatKind::Until => {
                if let (Ok(d1), Ok(d2)) = (
                    NaiveDate::parse_from_str(&modal.on_date_str, "%Y-%m-%d"),
                    NaiveDate::parse_from_str(&modal.until_str, "%Y-%m-%d"),
                ) {
                    let s1 = format_short_date(d1, lang);
                    let s2 = format_short_date(d2.max(d1), lang);
                    meta.push(if lang.is_en() {
                        format!("Weekly from {s1} to {s2}.")
                    } else {
                        format!("Щотижня з {s1} по {s2}.")
                    });
                }
            }
        }
        if let (Some(st), Some(et)) = (parse_time(&modal.start_time), parse_time(&modal.end_time)) {
            let mins = minutes_between(&st, &et);
            if mins > 0 {
                meta.push(tr.duration_label(mins));
            }
        }
        if !modal.url.trim().is_empty() {
            let (_, _, full) = detect_display_provider(modal.url.trim(), lang);
            meta.push(if lang.is_en() {
                format!("Detected: {full}")
            } else {
                format!("Розпізнано: {full}")
            });
        }
        ui.label(RichText::new(meta.join("\n")).size(12.0).color(p.text_muted));

        if let Some(err) = &modal.error {
            ui.label(RichText::new(err).size(12.5).color(p.danger_fg));
        }

        modal_footer(ui, |ui| {
            if ui.add(primary_button(tr.pick("Зберегти", "Save"), p)).clicked() {
                let subject = modal.subject.trim().to_string();
                let start_ok = parse_time(&modal.start_time);
                let end_ok = parse_time(&modal.end_time);
                if subject.is_empty() {
                    modal.error = Some(
                        tr.pick("Введіть назву зустрічі або предмету.", "Enter a title or subject.")
                            .to_string(),
                    );
                } else if start_ok.is_none() || end_ok.is_none() {
                    modal.error = Some(
                        tr.pick("Введіть час у форматі ГГ:ХХ (напр. 09:30).", "Enter time as HH:MM (e.g. 09:30).")
                            .to_string(),
                    );
                } else {
                    let st = start_ok.unwrap();
                    let et = end_ok.unwrap();
                    let et = if et > st {
                        et
                    } else {
                        add_minutes(&st, modal.default_duration)
                    };
                    let (on_date, repeat_until, day_index) = match modal.repeat {
                        RepeatKind::Weekly => (None, None, modal.day_index.rem_euclid(7)),
                        RepeatKind::Once => {
                            let d = NaiveDate::parse_from_str(&modal.on_date_str, "%Y-%m-%d")
                                .unwrap_or_else(|_| Local::now().date_naive());
                            (Some(d), None, i64::from(d.weekday().num_days_from_monday()))
                        }
                        RepeatKind::Until => {
                            let d1 = NaiveDate::parse_from_str(&modal.on_date_str, "%Y-%m-%d")
                                .unwrap_or_else(|_| Local::now().date_naive());
                            let d2 = NaiveDate::parse_from_str(&modal.until_str, "%Y-%m-%d")
                                .unwrap_or(d1)
                                .max(d1);
                            (Some(d1), Some(d2), i64::from(d1.weekday().num_days_from_monday()))
                        }
                    };
                    let clean_url = modal.url.trim().to_string();
                    outcome = ModalOutcome::SaveEdit(EditResult {
                        lesson_id: modal.lesson_id,
                        subject,
                        teacher: modal.teacher.trim().to_string(),
                        url: if clean_url.is_empty() { None } else { Some(clean_url) },
                        start_time: st,
                        end_time: et,
                        on_date,
                        repeat_until,
                        day_index,
                    });
                }
            }
            if ui.add(plain_button(tr.pick("Скасувати", "Cancel"), p)).clicked() {
                outcome = ModalOutcome::Close;
            }
        });
    });

    outcome
}

/// Settings modal — exact 1-to-1 non-AI AutoPara settings (ZERO "Помічник на парі" / AI options).
#[allow(clippy::too_many_arguments)]
pub fn show_settings_modal(
    ctx: &egui::Context,
    modal: &mut SettingsModal,
    update_status: Option<&str>,
    checking_updates: bool,
    gcal: Option<&GcalStatus>,
    has_timetable: bool,
    p: Palette,
    lang: Lang,
    t: f32,
) -> ModalOutcome {
    let tr = Tr::new(lang);
    let mut outcome = ModalOutcome::None;

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return ModalOutcome::Close;
    }

    show_modal_frame(ctx, "settings_modal", 470.0, p, t, |ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 11.0);
        ui.label(RichText::new(tr.pick("Налаштування", "Settings")).size(20.0).strong());
        ui.add_space(2.0);

        form_row(ui, tr.pick("Відкривати за", "Open ahead"), |ui| {
            ui.add(
                egui::DragValue::new(&mut modal.lead_minutes)
                    .range(0..=60)
                    .suffix(tr.pick(" хв до початку", " min before")),
            );
        });

        form_row(ui, tr.pick("Тривалість", "Default length"), |ui| {
            ui.add(
                egui::DragValue::new(&mut modal.class_duration_minutes)
                    .range(15..=300)
                    .speed(5.0)
                    .suffix(tr.pick(" хв", " min")),
            );
        });

        form_row(ui, tr.pick("Мова інтерфейсу", "Interface language"), |ui| {
            let current = usize::from(modal.language == LANG_EN);
            if let Some(i) = segmented(ui, "lang_seg", ["Українська", "English"], current, p) {
                modal.language = [LANG_UK, LANG_EN][i].to_string();
            }
        });

        form_row(ui, tr.pick("Вигляд", "View"), |ui| {
            let labels = [tr.pick("Календар", "Calendar"), tr.pick("Список", "List")];
            if let Some(i) = segmented(ui, "view_seg", labels, usize::from(modal.list_view), p) {
                modal.list_view = i == 1;
            }
        });

        ui.add_space(4.0);
        ui.label(
            RichText::new(tr.pick("Сповіщення", "Notifications"))
                .size(12.5)
                .strong()
                .color(p.text_muted),
        );
        ui.checkbox(
            &mut modal.notifications_enabled,
            tr.pick("Нагадувати перед початком", "Remind before start"),
        );
        if modal.notifications_enabled {
            form_row(ui, tr.pick("Час сповіщення", "Reminder lead"), |ui| {
                ui.add(
                    egui::DragValue::new(&mut modal.notify_minutes)
                        .range(1..=120)
                        .suffix(tr.pick(" хв до початку", " min before")),
                );
            });
        }

        ui.add_space(4.0);
        ui.label(
            RichText::new(tr.pick("Якщо початок пропущено", "If start was missed"))
                .size(12.5)
                .strong()
                .color(p.text_muted),
        );
        ui.radio_value(
            &mut modal.catchup_mode,
            CATCHUP_NOTIFY.to_string(),
            tr.pick(
                "Сповістити мене й відкрити після підтвердження",
                "Notify and open on confirmation",
            ),
        );
        ui.radio_value(
            &mut modal.catchup_mode,
            CATCHUP_OPEN.to_string(),
            tr.pick("Відкрити одразу", "Open immediately"),
        );
        ui.radio_value(
            &mut modal.catchup_mode,
            CATCHUP_MISSED.to_string(),
            tr.pick("Позначити як пропущену", "Mark as missed"),
        );
        ui.label(
            RichText::new(tr.pick(
                "Зустріч, яка вже тривала на момент запуску AutoPara, ніколи не відкривається сама — застосунок спершу спитає.",
                "A call already running when AutoPara launches is never opened unasked — the app always asks first.",
            ))
            .size(11.5)
            .color(p.text_muted),
        );

        ui.add_space(4.0);
        let autostart_label = if cfg!(target_os = "macos") {
            tr.pick(
                "Запускати AutoPara під час входу в систему",
                "Start AutoPara at login",
            )
        } else {
            tr.pick(
                "Запускати AutoPara разом із Windows",
                "Start AutoPara with Windows",
            )
        };
        ui.checkbox(&mut modal.autostart_enabled, autostart_label);

        if has_timetable {
            ui.add_space(6.0);
            ui.label(
                RichText::new(tr.pick("Розклад", "Timetable"))
                    .size(12.5)
                    .strong()
                    .color(p.text_muted),
            );
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 36.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    if modal.confirm_delete_timetable {
                        ui.label(
                            RichText::new(tr.pick("Точно видалити розклад?", "Delete the timetable?"))
                                .size(12.5),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            if ui.add(danger_button(tr.pick("Так, видалити", "Yes, delete"), p)).clicked() {
                                modal.confirm_delete_timetable = false;
                                outcome = ModalOutcome::DeleteTimetable;
                            }
                            if ui.add(secondary_button(tr.pick("Ні", "No"), p)).clicked() {
                                modal.confirm_delete_timetable = false;
                            }
                        });
                    } else if ui
                        .add(secondary_button(tr.pick("Видалити розклад", "Delete timetable"), p))
                        .clicked()
                    {
                        modal.confirm_delete_timetable = true;
                    }
                },
            );
            ui.label(
                RichText::new(tr.pick(
                    "Прибирає імпортований розклад пар. Ваші созвони та зустрічі з Google Календаря залишаться.",
                    "Removes the imported class timetable. Your own calls and Google Calendar meetings stay.",
                ))
                .size(11.5)
                .color(p.text_muted),
            );
        }

        // None = this build has no Google client configured: no section at all
        if let Some(g) = gcal {
            ui.add_space(6.0);
            ui.label(
                RichText::new(tr.pick("Google Календар", "Google Calendar"))
                    .size(12.5)
                    .strong()
                    .color(p.text_muted),
            );
            if g.connected {
                let who = g.account.as_deref().unwrap_or("Google");
                let synced = match (&g.last_sync, lang.is_en()) {
                    (Some(t), true) => format!(" · updated at {t}"),
                    (Some(t), false) => format!(" · оновлено о {t}"),
                    (None, _) => String::new(),
                };
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), 36.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.label(RichText::new(format!("{who}{synced}")).size(12.5));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add(secondary_button(tr.pick("Вийти", "Sign out"), p)).clicked() {
                                outcome = ModalOutcome::GcalSignOut;
                            }
                        });
                    },
                );
                ui.checkbox(
                    &mut modal.gcal_auto_open,
                    tr.pick(
                        "Відкривати зустрічі з календаря автоматично",
                        "Open calendar meetings automatically",
                    ),
                );
                ui.label(
                    RichText::new(tr.pick(
                        "Зустрічі з календаря підтягуються самі раз на хвилину. Пари з розкладу відкриваються автоматично завжди.",
                        "Calendar meetings are pulled in every minute. Timetable classes always open automatically.",
                    ))
                    .size(11.5)
                    .color(p.text_muted),
                );
            } else if g.signing_in {
                ui.label(
                    RichText::new(tr.pick(
                        "Увійдіть у Google у браузері, що відкрився…",
                        "Sign in to Google in the browser that just opened…",
                    ))
                    .size(12.5),
                );
            } else {
                if ui
                    .add(secondary_button(tr.pick("Увійти через Google", "Sign in with Google"), p))
                    .clicked()
                {
                    outcome = ModalOutcome::GcalSignIn;
                }
                ui.label(
                    RichText::new(tr.pick(
                        "Зустрічі з вашого Google Календаря з'являтимуться в AutoPara самі.",
                        "Meetings from your Google Calendar will appear in AutoPara by themselves.",
                    ))
                    .size(11.5)
                    .color(p.text_muted),
                );
            }
            if let Some(err) = &g.error {
                ui.label(RichText::new(err).size(11.5).color(p.danger));
            }
        }

        ui.add_space(6.0);
        ui.label(
            RichText::new(tr.pick("Оновлення", "Updates"))
                .size(12.5)
                .strong()
                .color(p.text_muted),
        );
        ui.radio_value(
            &mut modal.update_mode,
            UPDATE_ASK.to_string(),
            tr.pick("Повідомляти про нову версію", "Notify about new version"),
        );
        ui.radio_value(
            &mut modal.update_mode,
            UPDATE_AUTO.to_string(),
            tr.pick("Завантажувати автоматично", "Download automatically"),
        );
        ui.radio_value(
            &mut modal.update_mode,
            UPDATE_OFF.to_string(),
            tr.pick("Не перевіряти", "Don't check"),
        );
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), 36.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                let ver_text = if lang.is_en() {
                    format!("Version {CURRENT_VERSION}")
                } else {
                    format!("Версія {CURRENT_VERSION}")
                };
                ui.label(RichText::new(ver_text).size(12.0).color(p.text_muted));
                if let Some(st) = update_status {
                    ui.add_space(8.0);
                    ui.label(RichText::new(st).size(12.0).color(p.text_faint));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let btn_lbl = if checking_updates {
                        tr.pick("Перевіряю…", "Checking…")
                    } else {
                        tr.pick("Перевірити зараз", "Check now")
                    };
                    if ui
                        .add_enabled(!checking_updates, secondary_button(btn_lbl, p))
                        .clicked()
                    {
                        outcome = ModalOutcome::CheckUpdatesNow;
                    }
                });
            },
        );
        ui.label(
            RichText::new(tr.pick(
                "Нові версії беруться з GitHub. Навіть із автоматичним завантаженням установлення починається лише після вашого підтвердження.",
                "New versions come from GitHub Releases. Even with automatic download, installation starts only after your confirmation.",
            ))
            .size(11.5)
            .color(p.text_muted),
        );

        modal_footer(ui, |ui| {
            if ui.add(primary_button(tr.pick("Зберегти", "Save"), p)).clicked() {
                outcome = ModalOutcome::SaveSettings(modal.clone());
            }
            if ui.add(plain_button(tr.pick("Скасувати", "Cancel"), p)).clicked() {
                outcome = ModalOutcome::Close;
            }
        });
    });

    outcome
}

pub fn show_group_modal(
    ctx: &egui::Context,
    modal: &mut GroupModal,
    courses: &[Course],
    groups_with_counts: &[(Group, usize)],
    p: Palette,
    lang: Lang,
    t: f32,
) -> ModalOutcome {
    let tr = Tr::new(lang);
    let mut outcome = ModalOutcome::None;

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return ModalOutcome::Close;
    }

    show_modal_frame(ctx, "group_modal", 440.0, p, t, |ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 12.0);
        ui.label(
            RichText::new(tr.pick("Обрати розклад або групу", "Switch workspace or group"))
                .size(20.0)
                .strong(),
        );

        let course_label_for = |c: &Course| -> String {
            if c.ordinal == PERSONAL_COURSE_ORDINAL {
                tr.pick("Мої созвони", "My Calls").to_string()
            } else {
                c.name.clone()
            }
        };

        let selected_course = courses
            .iter()
            .find(|c| c.id == modal.selected_course_id)
            .or_else(|| courses.first());
        let selected_course_name = selected_course
            .map(course_label_for)
            .unwrap_or_default();

        form_row(ui, tr.pick("Розклад / Курс", "Workspace / Course"), |ui| {
            ComboBox::from_id_salt("grp_course_combo")
                .selected_text(selected_course_name)
                .width(240.0)
                .show_ui(ui, |ui| {
                    for c in courses {
                        let lbl = course_label_for(c);
                        if ui
                            .selectable_value(&mut modal.selected_course_id, c.id, lbl)
                            .clicked()
                            && let Some((first_g, _)) =
                                groups_with_counts.iter().find(|(g, _)| g.course_id == c.id)
                        {
                            modal.selected_group_id = first_g.id;
                        }
                    }
                });
        });

        let is_personal = selected_course.is_some_and(|c| c.ordinal == PERSONAL_COURSE_ORDINAL);
        Frame::NONE
            .fill(p.sunken)
            .stroke(Stroke::new(1.0_f32, p.border_soft))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(210.0).show(ui, |ui| {
                    for (g, count) in groups_with_counts
                        .iter()
                        .filter(|(g, _)| g.course_id == modal.selected_course_id)
                    {
                        let selected = g.id == modal.selected_group_id;
                        let name = if is_personal && g.name == "Особисті" {
                            tr.pick("Особисті", "Personal").to_string()
                        } else {
                            g.name.clone()
                        };
                        let bg = if selected { p.accent_fill } else { Color32::TRANSPARENT };
                        let fg = if selected { p.accent_text } else { p.text };
                        let (rect, resp) = ui.allocate_exact_size(
                            vec2(ui.available_width(), 36.0),
                            egui::Sense::click(),
                        );
                        let fill = if !selected && resp.hovered() { p.accent_soft } else { bg };
                        ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
                        ui.painter().text(
                            egui::pos2(rect.left() + 12.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            name,
                            egui::FontId::proportional(13.5),
                            fg,
                        );
                        ui.painter().text(
                            egui::pos2(rect.right() - 12.0, rect.center().y),
                            egui::Align2::RIGHT_CENTER,
                            tr.items_count(*count),
                            egui::FontId::proportional(12.0),
                            if selected { fg } else { p.text_muted },
                        );
                        if resp.clicked() {
                            modal.selected_group_id = g.id;
                        }
                    }
                });
            });

        modal_footer(ui, |ui| {
            if ui.add(primary_button(tr.pick("Обрати", "Select"), p)).clicked() {
                outcome = ModalOutcome::SelectGroup {
                    course_id: modal.selected_course_id,
                    group_id: modal.selected_group_id,
                };
            }
            if ui.add(plain_button(tr.pick("Скасувати", "Cancel"), p)).clicked() {
                outcome = ModalOutcome::Close;
            }
        });
    });

    outcome
}

pub fn show_import_modal(
    ctx: &egui::Context,
    modal: &mut ImportModal,
    p: Palette,
    lang: Lang,
    t: f32,
) -> ModalOutcome {
    let tr = Tr::new(lang);
    let mut outcome = ModalOutcome::None;

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return ModalOutcome::Close;
    }

    show_modal_frame(ctx, "import_modal", 460.0, p, t, |ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 12.0);
        ui.label(
            RichText::new(tr.pick("Імпорт розкладу (.docx)", "Import timetable (.docx)"))
                .size(20.0)
                .strong(),
        );

        // Drop well button
        let (well_rect, well_resp) =
            ui.allocate_exact_size(vec2(ui.available_width(), 106.0), egui::Sense::click());
        let well_fill = if well_resp.hovered() { p.drop_hover_bg } else { p.drop_bg };
        let well_border = if well_resp.hovered() { p.accent } else { p.drop_border };
        ui.painter().rect(
            well_rect,
            CornerRadius::same(12),
            well_fill,
            Stroke::new(1.5_f32, well_border),
            egui::StrokeKind::Inside,
        );
        let doc_rect = egui::Rect::from_center_size(
            egui::pos2(well_rect.center().x, well_rect.top() + 34.0),
            vec2(22.0, 26.0),
        );
        icons::draw_document(
            ui.painter(),
            doc_rect,
            if well_resp.hovered() { p.accent } else { p.text_faint },
        );
        let caption = modal
            .path
            .as_deref()
            .and_then(|s| std::path::Path::new(s).file_name())
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| {
                tr.pick(
                    "Перетягніть сюди .docx або натисніть, щоб обрати",
                    "Drop a .docx file here or click to browse",
                )
            });
        ui.painter().text(
            egui::pos2(well_rect.center().x, well_rect.bottom() - 28.0),
            egui::Align2::CENTER_CENTER,
            caption,
            egui::FontId::proportional(13.0),
            if modal.path.is_some() { p.text } else { p.text_muted },
        );
        if well_resp.clicked() {
            outcome = ModalOutcome::PickDocxForImport;
        }

        if let Some(err) = &modal.error {
            ui.label(RichText::new(err).color(p.danger_fg).size(12.5));
        }

        if !modal.courses.is_empty() {
            let sel_ord = modal
                .selected_ordinal
                .unwrap_or_else(|| modal.courses[0].ordinal);
            let current_name = modal
                .courses
                .iter()
                .find(|c| c.ordinal == sel_ord)
                .map(|c| c.name.as_str())
                .unwrap_or("");
            form_row(ui, tr.pick("Курс", "Course"), |ui| {
                ComboBox::from_id_salt("imp_course")
                    .selected_text(current_name)
                    .width(260.0)
                    .show_ui(ui, |ui| {
                        for c in &modal.courses {
                            if ui
                                .selectable_value(&mut modal.selected_ordinal, Some(c.ordinal), &c.name)
                                .clicked()
                            {
                                modal.selected_group = c.groups.first().map(|g| g.name.clone());
                            }
                        }
                    });
            });

            if let Some(course) = modal.courses.iter().find(|c| Some(c.ordinal) == modal.selected_ordinal) {
                Frame::NONE
                    .fill(p.sunken)
                    .stroke(Stroke::new(1.0_f32, p.border_soft))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                            for g in &course.groups {
                                let count = course
                                    .lessons
                                    .iter()
                                    .filter(|l| l.group_names.contains(&g.name))
                                    .count();
                                let selected = modal.selected_group.as_deref() == Some(g.name.as_str());
                                let (rect, resp) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), 34.0),
                                    egui::Sense::click(),
                                );
                                let fill = if selected {
                                    p.accent_fill
                                } else if resp.hovered() {
                                    p.accent_soft
                                } else {
                                    Color32::TRANSPARENT
                                };
                                let fg = if selected { p.accent_text } else { p.text };
                                ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
                                ui.painter().text(
                                    egui::pos2(rect.left() + 12.0, rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    &g.name,
                                    egui::FontId::proportional(13.0),
                                    fg,
                                );
                                ui.painter().text(
                                    egui::pos2(rect.right() - 12.0, rect.center().y),
                                    egui::Align2::RIGHT_CENTER,
                                    tr.items_count(count),
                                    egui::FontId::proportional(12.0),
                                    if selected { fg } else { p.text_muted },
                                );
                                if resp.clicked() {
                                    modal.selected_group = Some(g.name.clone());
                                }
                            }
                        });
                    });
            }
        }

        modal_footer(ui, |ui| {
            let can_commit = modal.path.is_some()
                && modal.selected_ordinal.is_some()
                && modal.selected_group.is_some();
            if ui
                .add_enabled(can_commit, primary_button(tr.pick("Імпортувати", "Import"), p))
                .clicked()
                && let (Some(path), Some(ord), Some(gname)) = (
                    modal.path.clone(),
                    modal.selected_ordinal,
                    modal.selected_group.clone(),
                )
            {
                outcome = ModalOutcome::CommitImport {
                    path,
                    ordinal: ord,
                    group_name: gname,
                };
            }
            if ui.add(plain_button(tr.pick("Скасувати", "Cancel"), p)).clicked() {
                outcome = ModalOutcome::Close;
            }
        });
    });

    outcome
}

pub fn show_move_scope_modal(
    ctx: &egui::Context,
    modal: &MoveScopeModal,
    p: Palette,
    lang: Lang,
    t: f32,
) -> ModalOutcome {
    let tr = Tr::new(lang);
    let mut outcome = ModalOutcome::None;
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return ModalOutcome::Close;
    }
    show_modal_frame(ctx, "move_scope_modal", 420.0, p, t, |ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 12.0);
        ui.label(
            RichText::new(tr.pick("Перенести повторювану зустріч", "Move repeating event"))
                .size(18.0)
                .strong(),
        );
        let body = if lang.is_en() {
            format!("“{}” repeats weekly. Move only this date or the entire series?", modal.subject)
        } else {
            format!("«{}» повторюється щотижня. Перенести лише цю дату чи всю серію?", modal.subject)
        };
        ui.label(RichText::new(body).size(13.5));
        modal_footer(ui, |ui| {
            if ui
                .add(primary_button(tr.pick("Лише цю дату", "Only this date"), p))
                .clicked()
            {
                outcome = ModalOutcome::ConfirmMove {
                    modal: modal.clone(),
                    only_one: true,
                };
            }
            if ui
                .add(secondary_button(tr.pick("Всю серію", "Entire series"), p))
                .clicked()
            {
                outcome = ModalOutcome::ConfirmMove {
                    modal: modal.clone(),
                    only_one: false,
                };
            }
            if ui.add(plain_button(tr.pick("Скасувати", "Cancel"), p)).clicked() {
                outcome = ModalOutcome::Close;
            }
        });
    });
    outcome
}

pub fn show_delete_confirm_modal(
    ctx: &egui::Context,
    modal: &DeleteConfirmModal,
    p: Palette,
    lang: Lang,
    t: f32,
) -> ModalOutcome {
    let tr = Tr::new(lang);
    let mut outcome = ModalOutcome::None;
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return ModalOutcome::Close;
    }
    show_modal_frame(ctx, "delete_confirm_modal", 400.0, p, t, |ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 12.0);
        ui.label(
            RichText::new(tr.pick("Видалити з розкладу", "Delete event"))
                .size(18.0)
                .strong(),
        );
        let body = if modal.is_series {
            if lang.is_en() {
                format!("“{}” repeats weekly. Delete the entire series?", modal.subject)
            } else {
                format!("«{}» повторюється щотижня. Видалити всю серію?", modal.subject)
            }
        } else if lang.is_en() {
            format!("Delete “{}” from the calendar?", modal.subject)
        } else {
            format!("Видалити «{}» з розкладу?", modal.subject)
        };
        ui.label(RichText::new(body).size(13.5));
        modal_footer(ui, |ui| {
            if ui.add(danger_button(tr.pick("Видалити", "Delete"), p)).clicked() {
                outcome = ModalOutcome::ConfirmDelete(modal.lesson_id);
            }
            if ui.add(plain_button(tr.pick("Скасувати", "Cancel"), p)).clicked() {
                outcome = ModalOutcome::Close;
            }
        });
    });
    outcome
}
