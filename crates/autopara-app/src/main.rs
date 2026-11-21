#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod anim;
mod calendar;
mod gcal_sync;
mod i18n;
mod icons;
mod modals;
mod palette;
mod platform;

use autopara_core::autostart;
use autopara_core::filters::{self, LessonFilter};
use autopara_core::launcher;
use autopara_core::models::{
    Course, Group, LANG_EN, LANG_UK, Lesson, PERSONAL_COURSE_ORDINAL, STATUS_MISSED, Settings, UPDATE_AUTO, UPDATE_OFF,
};
use autopara_core::nextup;
use autopara_core::normalize::{detect_provider, pair_slot, week_start};
use autopara_core::schedule_parser;
use autopara_core::scheduler::{SchedulerEngine, SchedulerEvent, TICK_SECONDS};
use autopara_core::storage::{Storage, default_db_path};
use autopara_core::updater::{self, CURRENT_VERSION, Release, TargetPlatform};
use calendar::{CalendarAction, ContextPopup, DragState};
use chrono::{Datelike, Duration, Local, NaiveDate};
use egui::{
    Align, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame,
    Layout, RichText, Sense, Stroke, StrokeKind, TextureHandle, Ui, pos2, vec2,
};
use i18n::{Lang, Tr, format_week_title};
use modals::{
    DeleteConfirmModal, EditModal, EditResult, GroupModal, ImportModal, ModalOutcome,
    MoveScopeModal, SettingsModal, center_pad, plain_button, primary_button, secondary_button,
};
use palette::Palette;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use tray_icon::TrayIconEvent;
use tray_icon::menu::MenuEvent;

const LOGO_PNG: &[u8] = include_bytes!("../../../assets/logo.png");
const INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");
const INTER_BOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-Bold.ttf");

fn bold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

fn semibold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

#[derive(Debug, Clone)]
struct CatchupItem {
    lesson_id: i64,
    subject: String,
    start_time: String,
}

struct SharedBackend {
    storage: Storage,
    scheduler: SchedulerEngine,
    pending_catchups: Vec<CatchupItem>,
    dirty: bool,
}

enum ActiveModal {
    Edit(EditModal),
    Settings(SettingsModal),
    Group(GroupModal),
    Import(ImportModal),
    MoveScope(MoveScopeModal),
    DeleteConfirm(DeleteConfirmModal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterDropdown {
    Subject,
    Group,
}

struct CachedView {
    settings: Settings,
    courses: Vec<Course>,
    course_groups: Vec<Group>,
    all_groups: Vec<(Group, usize)>,
    selected_course: Option<Course>,
    selected_group: Option<Group>,
    shown_group: Option<Group>,
    lessons: Vec<Lesson>,
    occurrences: HashMap<(i64, String), String>,
    next_up_id: Option<i64>,
    next_up_label: Option<String>,
    pending_catchups: Vec<CatchupItem>,
}

#[derive(Debug, Clone)]
enum UpdateBannerState {
    Hidden,
    Available(Release),
    Downloading { release: Release, percent: u8 },
    Ready { release: Release, path: PathBuf },
}

enum UpdateMsg {
    Checked {
        result: Result<Option<Release>, String>,
        manual: bool,
    },
    Progress {
        release: Release,
        percent: u8,
    },
    Downloaded {
        release: Release,
        path: PathBuf,
    },
    DownloadFailed(String),
}

struct AutoParaApp {
    backend: Arc<Mutex<SharedBackend>>,
    cache: CachedView,
    anchor: NaiveDate,
    view_group_id: Option<i64>,
    search_text: String,
    subject_filter: String,
    open_dropdown: Option<(FilterDropdown, egui::Rect)>,
    active_modal: Option<ActiveModal>,
    drag: Option<DragState>,
    popup: Option<ContextPopup>,
    morph: anim::MorphState,
    logo_tex: Option<TextureHandle>,
    visuals_applied: bool,
    should_quit: Arc<AtomicBool>,
    start_hidden: bool,
    applied_initial_hide: bool,
    _tray: Option<tray_icon::TrayIcon>,
    updates_dir: PathBuf,
    update_tx: Sender<UpdateMsg>,
    update_rx: Receiver<UpdateMsg>,
    update_banner: UpdateBannerState,
    update_checking: bool,
    update_downloading: bool,
    update_status: Option<String>,
    gcal: Option<gcal_sync::Gcal>,
}

fn spawn_update_check(tx: Sender<UpdateMsg>, ctx: egui::Context, manual: bool) {
    std::thread::spawn(move || {
        let res = updater::fetch_latest(TargetPlatform::current()).map_err(|e| e.to_string());
        let _ = tx.send(UpdateMsg::Checked {
            result: res,
            manual,
        });
        ctx.request_repaint();
    });
}

impl AutoParaApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        backend: Arc<Mutex<SharedBackend>>,
        start_hidden: bool,
    ) -> Self {
        install_fonts(&cc.egui_ctx);
        let logo_tex = load_logo_texture(&cc.egui_ctx);

        let should_quit = Arc::new(AtomicBool::new(false));
        let tray_bundle = platform::build_tray_icon(LOGO_PNG);
        let (tray_handle, open_id, quit_id) = match tray_bundle {
            Some((t, o, q)) => (Some(t), Some(o), Some(q)),
            None => (None, None, None),
        };

        let updates_dir = {
            let db_path = backend.lock().ok().map(|g| g.storage.path.clone());
            updater::updates_dir(db_path.as_deref())
        };
        updater::tidy(&updates_dir, CURRENT_VERSION);

        let (update_tx, update_rx) = mpsc::channel::<UpdateMsg>();

        // Background thread: 15s scheduler + instant tray polling + periodic GitHub update checks
        let bg_backend = Arc::clone(&backend);
        let bg_ctx = cc.egui_ctx.clone();
        let bg_quit = Arc::clone(&should_quit);
        let bg_upd_tx = update_tx.clone();
        std::thread::spawn(move || {
            let mut ticks_until_sched: u32 = 0;
            let mut ticks_until_update: u32 = 150; // 15s after launch, matching original FIRST_CHECK_MS
            loop {
                if bg_quit.load(Ordering::Relaxed) {
                    break;
                }

                // Tray left-click
                while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                    if let TrayIconEvent::Click {
                        button: tray_icon::MouseButton::Left,
                        button_state: tray_icon::MouseButtonState::Up,
                        ..
                    } = ev
                    {
                        platform::show_main_window(&bg_ctx);
                    }
                }

                // Tray menu clicks
                while let Ok(ev) = MenuEvent::receiver().try_recv() {
                    if open_id.as_ref() == Some(ev.id()) {
                        platform::show_main_window(&bg_ctx);
                    } else if quit_id.as_ref() == Some(ev.id()) {
                        bg_quit.store(true, Ordering::Relaxed);
                        bg_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        bg_ctx.request_repaint();
                        return;
                    }
                }

                // Scheduler tick every TICK_SECONDS (15s)
                if ticks_until_sched == 0 {
                    ticks_until_sched = (TICK_SECONDS * 10) as u32;
                    let mut should_surface = false;
                    let mut toasts: Vec<(String, String)> = Vec::new();
                    if let Ok(mut guard) = bg_backend.lock() {
                        let now = Local::now().naive_local();
                        let lang = guard
                            .storage
                            .settings()
                            .map(|s| Lang::from_code(&s.language))
                            .unwrap_or(Lang::Uk);
                        let SharedBackend {
                            storage,
                            scheduler,
                            pending_catchups,
                            dirty,
                        } = &mut *guard;
                        if let Ok((_decisions, events)) = scheduler.tick(storage, now) {
                            for ev in events {
                                *dirty = true;
                                match ev {
                                    SchedulerEvent::CatchupAvailable {
                                        lesson_id,
                                        subject,
                                        start_time,
                                        ..
                                    } => {
                                        if !pending_catchups
                                            .iter()
                                            .any(|c| c.lesson_id == lesson_id)
                                        {
                                            pending_catchups.push(CatchupItem {
                                                lesson_id,
                                                subject,
                                                start_time,
                                            });
                                        }
                                        should_surface = true;
                                    }
                                    SchedulerEvent::ReminderDue {
                                        subject,
                                        start_time,
                                        minutes,
                                        ..
                                    } => {
                                        let title = if lang.is_en() {
                                            format!("AutoPara · starts in {minutes} min")
                                        } else {
                                            format!("AutoPara · початок через {minutes} хв")
                                        };
                                        toasts.push((title, format!("{subject} ({start_time})")));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    for (t, b) in toasts {
                        platform::send_reminder_toast(&t, &b);
                    }
                    if should_surface {
                        platform::show_main_window(&bg_ctx);
                    } else {
                        bg_ctx.request_repaint();
                    }
                } else {
                    ticks_until_sched -= 1;
                }

                // Periodic GitHub release check (15s after start, then every 6 hours)
                if ticks_until_update == 0 {
                    ticks_until_update = 6 * 3600 * 10;
                    let enabled = bg_backend
                        .lock()
                        .ok()
                        .and_then(|g| g.storage.settings().ok())
                        .is_none_or(|s| s.update_mode != UPDATE_OFF);
                    if enabled {
                        spawn_update_check(bg_upd_tx.clone(), bg_ctx.clone(), false);
                    }
                } else {
                    ticks_until_update -= 1;
                }

                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });

        let gcal = gcal_sync::Gcal::start(
            Arc::clone(&backend),
            Arc::clone(&should_quit),
            cc.egui_ctx.clone(),
        );

        let anchor = Local::now().date_naive();
        let cache = load_cache(&backend, anchor, None);

        Self {
            backend,
            cache,
            anchor,
            view_group_id: None,
            search_text: String::new(),
            subject_filter: String::new(),
            open_dropdown: None,
            active_modal: None,
            drag: None,
            popup: None,
            morph: anim::MorphState::new(),
            logo_tex,
            visuals_applied: false,
            should_quit,
            start_hidden,
            applied_initial_hide: false,
            _tray: tray_handle,
            updates_dir,
            update_tx,
            update_rx,
            update_banner: UpdateBannerState::Hidden,
            update_checking: false,
            update_downloading: false,
            update_status: None,
            gcal,
        }
    }

    fn reload(&mut self) {
        self.cache = load_cache(&self.backend, self.anchor, self.view_group_id);
    }

    fn lang(&self) -> Lang {
        Lang::from_code(&self.cache.settings.language)
    }

    fn set_modal(&mut self, modal: ActiveModal) {
        self.morph.begin_modal();
        self.active_modal = Some(modal);
    }

    fn open_new_call(&mut self, day: NaiveDate, start_time: Option<&str>) {
        let dur = self.cache.settings.class_duration_minutes;
        self.set_modal(ActiveModal::Edit(EditModal::new_call(day, start_time, dur)));
    }

    fn start_from_scratch(&mut self) {
        if let Ok(guard) = self.backend.lock() {
            let _ = guard.storage.ensure_personal_workspace();
        }
        self.view_group_id = None;
        self.reload();
    }

    fn open_import_with_path(&mut self, path: Option<String>) {
        let mut modal = ImportModal {
            path: None,
            courses: Vec::new(),
            selected_ordinal: self.cache.selected_course.as_ref().map(|c| c.ordinal),
            selected_group: self.cache.selected_group.as_ref().map(|g| g.name.clone()),
            error: None,
        };
        if let Some(p) = path {
            self.populate_import_modal(&mut modal, p);
        }
        self.set_modal(ActiveModal::Import(modal));
    }

    fn populate_import_modal(&self, modal: &mut ImportModal, path: String) {
        let dur = self.cache.settings.class_duration_minutes;
        match schedule_parser::parse_file(&path, Some(dur)) {
            Ok(courses) if !courses.is_empty() => {
                let prev_ord = modal.selected_ordinal;
                let chosen_ord = prev_ord
                    .filter(|o| courses.iter().any(|c| c.ordinal == *o))
                    .unwrap_or(courses[0].ordinal);
                let prev_grp = modal.selected_group.clone();
                let chosen_grp = courses
                    .iter()
                    .find(|c| c.ordinal == chosen_ord)
                    .and_then(|c| {
                        prev_grp
                            .filter(|n| c.groups.iter().any(|g| g.name == *n))
                            .or_else(|| c.groups.first().map(|g| g.name.clone()))
                    });
                modal.path = Some(path);
                modal.courses = courses;
                modal.selected_ordinal = Some(chosen_ord);
                modal.selected_group = chosen_grp;
                modal.error = None;
            }
            Ok(_) => {
                modal.error = Some(
                    Tr::new(self.lang())
                        .pick("У файлі не знайдено таблиць розкладу.", "No timetable tables found in file.")
                        .to_string(),
                );
            }
            Err(e) => {
                modal.error = Some(e.to_string());
            }
        }
    }

    fn save_edit(&mut self, res: EditResult) {
        if let Ok(guard) = self.backend.lock() {
            let settings = guard.storage.settings().unwrap_or_default();
            let (course_id, group_id) = match (settings.selected_course_id, settings.selected_group_id) {
                (Some(cid), Some(gid))
                    if guard.storage.group(gid).ok().flatten().is_some() =>
                {
                    (cid, gid)
                }
                _ => match guard.storage.ensure_personal_workspace() {
                    Ok((c, g)) => (c.id, g.id),
                    Err(_) => return,
                },
            };

            let provider = detect_provider(res.url.as_deref());
            let pair = pair_slot(&res.start_time);
            let lesson = Lesson {
                id: res.lesson_id.unwrap_or(0),
                course_id,
                day_index: res.day_index,
                pair,
                start_time: res.start_time,
                end_time: res.end_time,
                subject: res.subject,
                teacher: res.teacher,
                needs_link: res.url.is_none(),
                url: res.url,
                provider,
                is_manual: true,
                source_key: String::new(),
                group_names: Vec::new(),
                on_date: res.on_date,
                repeat_until: res.repeat_until,
                skip_dates: BTreeSet::new(),
            };
            if res.lesson_id.is_some() {
                let _ = guard.storage.update_lesson(&lesson, None);
            } else {
                let _ = guard.storage.add_lesson(&lesson, &[group_id]);
            }
        }
        self.reload();
    }

    fn save_settings(&mut self, s: SettingsModal) {
        if let Ok(guard) = self.backend.lock() {
            let _ = guard
                .storage
                .set_setting("lead_minutes", &s.lead_minutes.clamp(0, 60).to_string());
            let _ = guard.storage.set_setting(
                "class_duration_minutes",
                &s.class_duration_minutes.clamp(15, 300).to_string(),
            );
            let lang = if s.language == LANG_EN { LANG_EN } else { LANG_UK };
            let _ = guard.storage.set_setting("language", lang);
            let _ = guard.storage.set_setting(
                "notifications_enabled",
                if s.notifications_enabled { "1" } else { "0" },
            );
            let _ = guard
                .storage
                .set_setting("notify_minutes", &s.notify_minutes.clamp(1, 120).to_string());
            let _ = guard.storage.set_setting("catchup_mode", &s.catchup_mode);
            let _ = guard.storage.set_setting(
                "autostart_enabled",
                if s.autostart_enabled { "1" } else { "0" },
            );
            let _ = guard.storage.set_setting("update_mode", &s.update_mode);
            let _ = guard
                .storage
                .set_setting("gcal_auto_open", if s.gcal_auto_open { "1" } else { "0" });
            autostart::sync(s.autostart_enabled);
        }
        self.reload();
    }

    fn request_manual_update_check(&mut self, ctx: &egui::Context) {
        if self.update_checking || self.update_downloading {
            return;
        }
        self.update_checking = true;
        self.update_status = None;
        spawn_update_check(self.update_tx.clone(), ctx.clone(), true);
    }

    fn start_update_download(&mut self, release: Release, ctx: &egui::Context) {
        if self.update_downloading {
            return;
        }
        let Some(asset) = release.asset.clone() else {
            return;
        };
        self.update_downloading = true;
        self.update_banner = UpdateBannerState::Downloading {
            release: release.clone(),
            percent: 0,
        };
        let dir = self.updates_dir.clone();
        let tx = self.update_tx.clone();
        let ctx_bg = ctx.clone();
        std::thread::spawn(move || {
            let mut last_pct: i32 = -1;
            let res = updater::download(&asset, &dir, |done, total| {
                let pct = done
                    .saturating_mul(100)
                    .checked_div(total)
                    .unwrap_or(0)
                    .min(100) as i32;
                if pct != last_pct {
                    last_pct = pct;
                    let _ = tx.send(UpdateMsg::Progress {
                        release: release.clone(),
                        percent: pct as u8,
                    });
                    ctx_bg.request_repaint();
                }
            });
            match res {
                Ok(path) => {
                    let _ = tx.send(UpdateMsg::Downloaded { release, path });
                }
                Err(err) => {
                    let _ = tx.send(UpdateMsg::DownloadFailed(err.to_string()));
                }
            }
            ctx_bg.request_repaint();
        });
    }

    fn poll_update_messages(&mut self, ctx: &egui::Context) {
        let tr = Tr::new(self.lang());
        while let Ok(msg) = self.update_rx.try_recv() {
            match msg {
                UpdateMsg::Checked { result, manual } => {
                    self.update_checking = false;
                    match result {
                        Ok(release_opt) => {
                            if let Ok(guard) = self.backend.lock() {
                                let ts = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
                                let _ = guard.storage.set_setting("last_update_check", &ts);
                            }
                            let Some(rel) = release_opt.filter(|r| {
                                updater::is_newer(&r.version, CURRENT_VERSION)
                            }) else {
                                if manual {
                                    self.update_status = Some(
                                        tr.pick(
                                            "Встановлено останню версію",
                                            "You are up to date",
                                        )
                                        .to_string(),
                                    );
                                }
                                continue;
                            };
                            if !manual && rel.version == self.cache.settings.skipped_update_version
                            {
                                continue;
                            }
                            if manual {
                                self.update_status = Some(if self.lang().is_en() {
                                    format!("Version {} available", rel.version)
                                } else {
                                    format!("Доступна версія {}", rel.version)
                                });
                            }
                            if self.cache.settings.update_mode == UPDATE_AUTO
                                && TargetPlatform::current().installable()
                                && rel.asset.is_some()
                            {
                                self.start_update_download(rel, ctx);
                            } else {
                                self.update_banner = UpdateBannerState::Available(rel);
                            }
                        }
                        Err(err) => {
                            if manual {
                                self.update_status = Some(err);
                            }
                        }
                    }
                }
                UpdateMsg::Progress { release, percent } => {
                    self.update_banner = UpdateBannerState::Downloading { release, percent };
                }
                UpdateMsg::Downloaded { release, path } => {
                    self.update_downloading = false;
                    self.update_banner = UpdateBannerState::Ready { release, path };
                }
                UpdateMsg::DownloadFailed(err) => {
                    self.update_downloading = false;
                    self.update_banner = UpdateBannerState::Hidden;
                    self.update_status = Some(err);
                }
            }
        }
    }

    fn step_week(&mut self, dir: i64) {
        self.morph.begin_calendar_morph(if dir > 0 { 1 } else { -1 });
        self.anchor += Duration::days(dir * 7);
        self.reload();
    }

    fn handle_calendar_action(&mut self, action: CalendarAction) {
        match action {
            CalendarAction::None => {}
            CalendarAction::OpenLesson { lesson_id, day } => {
                let claim = day <= Local::now().date_naive();
                if let Ok(mut guard) = self.backend.lock() {
                    let SharedBackend {
                        storage,
                        scheduler,
                        pending_catchups,
                        ..
                    } = &mut *guard;
                    if scheduler.open_now(storage, lesson_id, day, claim).unwrap_or(false) {
                        pending_catchups.retain(|c| c.lesson_id != lesson_id);
                    }
                }
                self.reload();
            }
            CalendarAction::EditLesson { lesson_id, day } => {
                if let Some(lesson) = self.cache.lessons.iter().find(|l| l.id == lesson_id).cloned()
                {
                    let dur = self.cache.settings.class_duration_minutes;
                    self.set_modal(ActiveModal::Edit(EditModal::from_lesson(&lesson, day, dur)));
                }
            }
            CalendarAction::CreateAt { day, start_time } => {
                self.open_new_call(day, start_time.as_deref());
            }
            CalendarAction::MarkLesson {
                lesson_id,
                day,
                status,
            } => {
                if let Ok(mut guard) = self.backend.lock() {
                    let SharedBackend {
                        storage,
                        scheduler,
                        pending_catchups,
                        ..
                    } = &mut *guard;
                    let _ = scheduler.mark(storage, lesson_id, &status, day);
                    pending_catchups.retain(|c| c.lesson_id != lesson_id);
                }
                self.reload();
            }
            CalendarAction::ClearMark { lesson_id, day } => {
                if let Ok(mut guard) = self.backend.lock() {
                    let SharedBackend {
                        storage, scheduler, ..
                    } = &mut *guard;
                    let _ = scheduler.clear_mark(storage, lesson_id, day);
                }
                self.reload();
            }
            CalendarAction::RequestDelete { lesson_id } => {
                if let Some(lesson) = self.cache.lessons.iter().find(|l| l.id == lesson_id) {
                    self.set_modal(ActiveModal::DeleteConfirm(DeleteConfirmModal {
                        lesson_id,
                        subject: lesson.subject.clone(),
                        is_series: lesson.repeat_until.is_some(),
                    }));
                }
            }
            CalendarAction::DropLesson {
                lesson_id,
                from_day,
                to_day,
                start_time,
                end_time,
            } => {
                if let Some(lesson) = self.cache.lessons.iter().find(|l| l.id == lesson_id) {
                    if lesson.repeats() {
                        self.set_modal(ActiveModal::MoveScope(MoveScopeModal {
                            lesson_id,
                            subject: lesson.subject.clone(),
                            from_day,
                            to_day,
                            start_time,
                            end_time,
                        }));
                    } else {
                        if let Ok(guard) = self.backend.lock() {
                            let wd = i64::from(to_day.weekday().num_days_from_monday());
                            let _ = guard.storage.move_lesson(
                                lesson_id,
                                wd,
                                pair_slot(&start_time),
                                &start_time,
                                &end_time,
                                Some(to_day),
                            );
                        }
                        self.reload();
                    }
                }
            }
            CalendarAction::OpenImport => {
                self.open_import_with_path(None);
            }
        }
    }

    /// Single unified top bar: [Week Title + < > + Clock + Action Capsule] ... [drag strip] ... [– □ ×].
    fn show_header_bar(&mut self, ctx: &egui::Context, p: Palette) {
        let lang = self.lang();
        let has_schedule = self.cache.selected_group.is_some();

        egui::TopBottomPanel::top("autopara_titlebar")
            .exact_height(64.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::NONE.fill(p.canvas))
            .show(ctx, |ui| {
                let bar_rect = ui.max_rect();
                let edge_pad = 16.0_f32;
                let cy = bar_rect.top() + 34.0; // Exact 16px top & 16px right for 36px window buttons

                // Right-aligned circular window buttons (– □ ×), 36px matching nav arrows.
                let btn_d = 36.0_f32;
                let win_gap = 8.0_f32;
                let cl_rect = egui::Rect::from_center_size(
                    pos2(bar_rect.right() - edge_pad - btn_d * 0.5, cy),
                    vec2(btn_d, btn_d),
                );
                let mx_rect = egui::Rect::from_center_size(
                    pos2(cl_rect.center().x - (btn_d + win_gap), cy),
                    vec2(btn_d, btn_d),
                );
                let mn_rect = egui::Rect::from_center_size(
                    pos2(mx_rect.center().x - (btn_d + win_gap), cy),
                    vec2(btn_d, btn_d),
                );

                // Left-to-right header order: [Action Capsule]  [Week Title + < >]  [Clock]  ...  [– □ ×]
                let mut x = bar_rect.left() + edge_pad;
                x = self.draw_top_action_menu_from_left(ui, x, cy, p);
                x += 20.0;

                if has_schedule {
                    let mon = week_start(self.anchor);
                    let (title_main, title_year) =
                        format_week_title(mon, mon + Duration::days(6), lang);

                    let main_g = ui
                        .painter()
                        .layout_no_wrap(title_main, bold_font(24.0), p.text);
                    ui.painter().galley(
                        pos2(x, cy - main_g.size().y * 0.52),
                        main_g.clone(),
                        p.text,
                    );
                    x += main_g.size().x + 8.0;

                    let year_g = ui
                        .painter()
                        .layout_no_wrap(title_year, FontId::proportional(24.0), p.text_faint);
                    ui.painter().galley(
                        pos2(x, cy - year_g.size().y * 0.52),
                        year_g.clone(),
                        p.text_faint,
                    );
                    x += year_g.size().x + 16.0;

                    // Prev '<' week circle (36x36)
                    let prev_rect =
                        egui::Rect::from_center_size(pos2(x + 18.0, cy), vec2(36.0, 36.0));
                    let prev_resp =
                        ui.interact(prev_rect, ui.id().with("nav_prev_week"), Sense::click());
                    let prev_hov = self.morph.smooth_hover(ctx, 3002, prev_resp.hovered());
                    let prev_prs = self
                        .morph
                        .smooth_press(ctx, 3002, prev_resp.is_pointer_button_down_on());
                    ui.painter().circle(
                        prev_rect.center(),
                        17.5 + prev_hov * 1.0 - prev_prs * 1.5,
                        palette::blend(p.surface, p.accent_soft, prev_hov),
                        Stroke::new(1.0_f32, palette::blend(p.border, p.accent, prev_hov * 0.45)),
                    );
                    icons::draw_chevron(
                        ui.painter(),
                        prev_rect.center() + vec2(-prev_hov * 1.0, 0.0),
                        6.2,
                        false,
                        palette::blend(p.text_muted, p.text, prev_hov * 0.6 + 0.4),
                    );
                    if prev_resp.clicked() {
                        self.step_week(-1);
                    }
                    x += 36.0 + 8.0;

                    // Next '>' week circle (36x36)
                    let next_rect =
                        egui::Rect::from_center_size(pos2(x + 18.0, cy), vec2(36.0, 36.0));
                    let next_resp =
                        ui.interact(next_rect, ui.id().with("nav_next_week"), Sense::click());
                    let next_hov = self.morph.smooth_hover(ctx, 3003, next_resp.hovered());
                    let next_prs = self
                        .morph
                        .smooth_press(ctx, 3003, next_resp.is_pointer_button_down_on());
                    ui.painter().circle(
                        next_rect.center(),
                        17.5 + next_hov * 1.0 - next_prs * 1.5,
                        palette::blend(p.surface, p.accent_soft, next_hov),
                        Stroke::new(1.0_f32, palette::blend(p.border, p.accent, next_hov * 0.45)),
                    );
                    icons::draw_chevron(
                        ui.painter(),
                        next_rect.center() + vec2(next_hov * 1.0, 0.0),
                        6.2,
                        true,
                        palette::blend(p.text_muted, p.text, next_hov * 0.6 + 0.4),
                    );
                    if next_resp.clicked() {
                        self.step_week(1);
                    }
                    x += 36.0 + 16.0;
                }

                // Live HH:MM:SS monospace clock to the right of the date & arrows (zero jitter)
                let clock = Local::now().format("%H:%M:%S").to_string();
                ui.painter().text(
                    pos2(x, cy),
                    Align2::LEFT_CENTER,
                    clock,
                    FontId::monospace(12.5),
                    p.text_faint,
                );
                x += 68.0;

                // Optional next-up status pill when enough breathing room remains for the drag strip
                if has_schedule && let Some(next_lbl) = &self.cache.next_up_label {
                    let avail_for_pill = (mn_rect.left() - 72.0 - (x + 14.0)).max(0.0);
                    if avail_for_pill >= 150.0 {
                        let ng = ui
                            .painter()
                            .layout_no_wrap(next_lbl.clone(), semibold_font(12.5), p.text);
                        let pw = (ng.size().x + 26.0).min(avail_for_pill);
                        let pill_rect =
                            egui::Rect::from_min_size(pos2(x + 14.0, cy - 17.0), vec2(pw, 34.0));
                        ui.painter().rect(
                            pill_rect,
                            CornerRadius::same(17),
                            p.surface,
                            Stroke::new(1.0_f32, p.border),
                            StrokeKind::Inside,
                        );
                        let np = ui
                            .painter()
                            .with_clip_rect(pill_rect.shrink2(vec2(12.0, 0.0)));
                        np.galley(
                            pos2(pill_rect.left() + 13.0, cy - ng.size().y * 0.5),
                            ng,
                            p.text,
                        );
                        x = pill_rect.right();
                    }
                }

                // Clean frameless drag strip between the action capsule and window controls (plus top edge)
                let drag_rect = egui::Rect::from_min_max(
                    pos2((x + 10.0).min(mn_rect.left()), bar_rect.top()),
                    pos2((mn_rect.left() - 10.0).max(x + 10.0), bar_rect.bottom()),
                );
                let drag_resp = ui.interact(
                    drag_rect,
                    ui.id().with("titlebar_drag"),
                    Sense::click_and_drag(),
                );
                if drag_resp.drag_started_by(egui::PointerButton::Primary) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag_resp.double_clicked() {
                    let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                }

                // Minimize
                let mn_resp = ui.interact(mn_rect, ui.id().with("win_min"), Sense::click());
                let mn_hov = self.morph.smooth_hover(ctx, 1003, mn_resp.hovered());
                let mn_prs = self.morph.smooth_press(ctx, 1003, mn_resp.is_pointer_button_down_on());
                ui.painter().circle(
                    mn_rect.center(),
                    btn_d * 0.5 + mn_hov * 0.9 - mn_prs * 1.4,
                    palette::blend(p.sunken, p.accent_soft, mn_hov),
                    Stroke::new(1.0_f32, palette::blend(p.border, p.accent, mn_hov * 0.45)),
                );
                icons::draw_win_minimize(
                    ui.painter(),
                    mn_rect.center(),
                    palette::blend(p.text_muted, p.text, mn_hov),
                );
                if mn_resp.clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }

                // Maximize / Restore
                let mx_resp = ui.interact(mx_rect, ui.id().with("win_max"), Sense::click());
                let mx_hov = self.morph.smooth_hover(ctx, 1002, mx_resp.hovered());
                let mx_prs = self.morph.smooth_press(ctx, 1002, mx_resp.is_pointer_button_down_on());
                ui.painter().circle(
                    mx_rect.center(),
                    btn_d * 0.5 + mx_hov * 0.9 - mx_prs * 1.4,
                    palette::blend(p.sunken, p.accent_soft, mx_hov),
                    Stroke::new(1.0_f32, palette::blend(p.border, p.accent, mx_hov * 0.45)),
                );
                icons::draw_win_maximize(
                    ui.painter(),
                    mx_rect.center(),
                    palette::blend(p.text_muted, p.text, mx_hov),
                );
                if mx_resp.clicked() {
                    let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                }

                // Close (hides to tray)
                let cl_resp = ui.interact(cl_rect, ui.id().with("win_close"), Sense::click());
                let cl_hov = self.morph.smooth_hover(ctx, 1001, cl_resp.hovered());
                let cl_prs = self.morph.smooth_press(ctx, 1001, cl_resp.is_pointer_button_down_on());
                ui.painter().circle(
                    cl_rect.center(),
                    btn_d * 0.5 + cl_hov * 0.9 - cl_prs * 1.4,
                    palette::blend(p.sunken, p.danger, cl_hov),
                    Stroke::new(1.0_f32, palette::blend(p.border, p.danger, cl_hov)),
                );
                icons::draw_win_close(
                    ui.painter(),
                    cl_rect.center(),
                    palette::blend(p.text_muted, Color32::WHITE, cl_hov),
                );
                if cl_resp.clicked() {
                    platform::hide_main_window(ctx);
                }
            });
    }

    /// Horizontal floating action capsule (+, Import, Groups, Settings) + fixed-width clock on the right.
    fn draw_top_action_menu_from_left(
        &mut self,
        ui: &mut Ui,
        left_x: f32,
        cy: f32,
        p: Palette,
    ) -> f32 {
        let ctx = ui.ctx().clone();
        let tr = Tr::new(self.lang());

        // Pill dimensions: 4 circular buttons (34px each, 42px center step -> 174 x 42)
        let pill_w = 174.0_f32;
        let pill_h = 42.0_f32;
        let pill_rect = egui::Rect::from_min_size(
            pos2(left_x, cy - pill_h * 0.5),
            vec2(pill_w, pill_h),
        );
        ui.painter().rect(
            pill_rect,
            CornerRadius::same(21),
            p.rail_bg,
            Stroke::new(1.0_f32, p.border_soft),
            StrokeKind::Inside,
        );

        let cx0 = pill_rect.left() + 24.0;
        let step = 42.0_f32;

        // 0. Primary '+' add button
        let add_rect = egui::Rect::from_center_size(pos2(cx0, cy), vec2(34.0, 34.0));
        let add_resp = ui.interact(add_rect, ui.id().with("top_add"), Sense::click());
        let add_hov = self.morph.smooth_hover(&ctx, 2001, add_resp.hovered());
        let add_prs = self.morph.smooth_press(&ctx, 2001, add_resp.is_pointer_button_down_on());
        ui.painter().circle_filled(
            add_rect.center(),
            16.5 + add_hov * 1.2 - add_prs * 1.5,
            p.rail_primary_bg,
        );
        icons::draw_plus(ui.painter(), add_rect.center(), 6.8, p.rail_primary_fg, 2.2);
        if add_resp
            .on_hover_text(tr.pick("Додати созвон / пару", "Add call / class"))
            .clicked()
        {
            self.open_new_call(self.anchor, None);
        }

        // 1. Import .docx
        let imp_rect = egui::Rect::from_center_size(pos2(cx0 + step, cy), vec2(34.0, 34.0));
        let imp_resp = ui.interact(imp_rect, ui.id().with("top_import"), Sense::click());
        let imp_hov = self.morph.smooth_hover(&ctx, 2002, imp_resp.hovered());
        let imp_prs = self.morph.smooth_press(&ctx, 2002, imp_resp.is_pointer_button_down_on());
        if imp_hov > 0.01 {
            ui.painter().circle_filled(
                imp_rect.center(),
                16.5 + imp_hov * 0.6 - imp_prs * 1.4,
                palette::blend(p.rail_bg, p.rail_hover, imp_hov),
            );
        }
        icons::draw_import(ui.painter(), imp_rect.center(), 7.2, p.rail_icon);
        if imp_resp
            .on_hover_text(tr.pick("Імпортувати розклад .docx…", "Import .docx timetable…"))
            .clicked()
        {
            self.open_import_with_path(None);
        }

        // 2. Switch workspace / course & group
        let has_courses = !self.cache.courses.is_empty();
        let grp_rect = egui::Rect::from_center_size(pos2(cx0 + step * 2.0, cy), vec2(34.0, 34.0));
        let grp_resp = ui.interact(
            grp_rect,
            ui.id().with("top_groups"),
            if has_courses { Sense::click() } else { Sense::hover() },
        );
        let grp_hov = self.morph.smooth_hover(&ctx, 2003, has_courses && grp_resp.hovered());
        let grp_prs = self.morph.smooth_press(
            &ctx,
            2003,
            has_courses && grp_resp.is_pointer_button_down_on(),
        );
        if grp_hov > 0.01 {
            ui.painter().circle_filled(
                grp_rect.center(),
                16.5 + grp_hov * 0.6 - grp_prs * 1.4,
                palette::blend(p.rail_bg, p.rail_hover, grp_hov),
            );
        }
        let grp_col = if has_courses {
            p.rail_icon
        } else {
            p.rail_icon.linear_multiply(0.35)
        };
        icons::draw_groups(ui.painter(), grp_rect.center(), 7.6, grp_col);
        if grp_resp
            .on_hover_text(tr.pick("Обрати розклад або групу…", "Switch workspace or group…"))
            .clicked()
            && has_courses
        {
            let cid = self
                .cache
                .selected_course
                .as_ref()
                .map(|c| c.id)
                .unwrap_or(self.cache.courses[0].id);
            let gid = self
                .cache
                .selected_group
                .as_ref()
                .map(|g| g.id)
                .or_else(|| {
                    self.cache
                        .all_groups
                        .iter()
                        .find(|(g, _)| g.course_id == cid)
                        .map(|(g, _)| g.id)
                })
                .unwrap_or(0);
            self.set_modal(ActiveModal::Group(GroupModal {
                selected_course_id: cid,
                selected_group_id: gid,
            }));
        }

        // 3. Settings (includes Language switch, NO AI, NO Theme)
        let set_rect = egui::Rect::from_center_size(pos2(cx0 + step * 3.0, cy), vec2(34.0, 34.0));
        let set_resp = ui.interact(set_rect, ui.id().with("top_settings"), Sense::click());
        let set_hov = self.morph.smooth_hover(&ctx, 2004, set_resp.hovered());
        let set_prs = self.morph.smooth_press(&ctx, 2004, set_resp.is_pointer_button_down_on());
        if set_hov > 0.01 {
            ui.painter().circle_filled(
                set_rect.center(),
                16.5 + set_hov * 0.6 - set_prs * 1.4,
                palette::blend(p.rail_bg, p.rail_hover, set_hov),
            );
        }
        icons::draw_gear(ui.painter(), set_rect.center(), 7.6, p.rail_icon);
        if set_resp
            .on_hover_text(tr.pick("Налаштування", "Settings"))
            .clicked()
        {
            self.set_modal(ActiveModal::Settings(
                SettingsModal::from_settings(&self.cache.settings),
            ));
        }

        pill_rect.right()
    }

    fn show_landing(&mut self, ui: &mut Ui, p: Palette) {
        let lang = self.lang();
        let tr = Tr::new(lang);

        Frame::NONE
            .fill(p.surface)
            .stroke(Stroke::new(1.0_f32, p.border))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(egui::Margin::same(24))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());

                let top_pad = ((ui.available_height() - 400.0) * 0.44).max(16.0);
                ui.add_space(top_pad);

                ui.vertical_centered(|ui| {
                    let (logo_rect, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
                    if let Some(tex) = &self.logo_tex {
                        ui.painter().image(
                            tex.id(),
                            logo_rect,
                            egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(tr.pick("Автозапуск созвонів і пар", "Auto-join calls & classes"))
                            .size(24.0)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.allocate_ui(vec2(460.0, 44.0), |ui| {
                        ui.label(
                            RichText::new(tr.pick(
                                "Додавайте посилання на созвони з нуля (Zoom, Meet, Teams, Discord) або імпортуйте університетський розклад .docx — AutoPara відкриє кожну зустріч вчасно.",
                                "Add meeting links from scratch (Zoom, Meet, Teams, Discord) or import a university .docx timetable — AutoPara opens each call on time.",
                            ))
                            .size(13.5)
                            .color(p.text_muted),
                        );
                    });

                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.add_space(center_pad(
                            ui,
                            &[
                                tr.pick("Додати перший созвон", "Add first call"),
                                tr.pick("Почати з нуля", "Start from scratch"),
                            ],
                            8.0,
                        ));
                        if ui
                            .add(primary_button(
                                tr.pick("Додати перший созвон", "Add first call"),
                                p,
                            ))
                            .clicked()
                        {
                            self.start_from_scratch();
                            self.open_new_call(Local::now().date_naive(), None);
                        }
                        ui.add_space(8.0);
                        if ui
                            .add(secondary_button(
                                tr.pick("Почати з нуля", "Start from scratch"),
                                p,
                            ))
                            .clicked()
                        {
                            self.start_from_scratch();
                        }
                    });

                    ui.add_space(18.0);
                    // Divider drawn as lines: Inter has no box-drawing glyphs.
                    let (div_rect, _) = ui.allocate_exact_size(vec2(460.0, 16.0), Sense::hover());
                    let div_text = ui.painter().layout_no_wrap(
                        tr.pick("або імпортуйте файл розкладу", "or import a timetable file").to_owned(),
                        FontId::proportional(12.0),
                        p.text_faint,
                    );
                    let c = div_rect.center();
                    let gap = div_text.size().x / 2.0 + 12.0;
                    let line = Stroke::new(1.0_f32, p.drop_border);
                    ui.painter().hline(div_rect.left()..=c.x - gap, c.y, line);
                    ui.painter().hline(c.x + gap..=div_rect.right(), c.y, line);
                    ui.painter().galley(c - div_text.size() / 2.0, div_text, p.text_faint);
                    ui.add_space(12.0);

                    // .docx Drop Well
                    let (well_rect, well_resp) =
                        ui.allocate_exact_size(vec2(460.0, 122.0), Sense::click());
                    let well_bg = if well_resp.hovered() { p.drop_hover_bg } else { p.drop_bg };
                    let well_bd = if well_resp.hovered() { p.accent } else { p.drop_border };
                    ui.painter().rect(
                        well_rect,
                        CornerRadius::same(14),
                        well_bg,
                        Stroke::new(1.5_f32, well_bd),
                        StrokeKind::Inside,
                    );
                    let doc_rect = egui::Rect::from_center_size(
                        pos2(well_rect.center().x, well_rect.top() + 42.0),
                        vec2(24.0, 30.0),
                    );
                    icons::draw_document(
                        ui.painter(),
                        doc_rect,
                        if well_resp.hovered() { p.accent } else { p.text_faint },
                    );
                    ui.painter().text(
                        pos2(well_rect.center().x, well_rect.bottom() - 32.0),
                        Align2::CENTER_CENTER,
                        tr.pick(
                            "Перетягніть сюди файл .docx або натисніть, щоб обрати",
                            "Drop a .docx file here or click to browse",
                        ),
                        FontId::proportional(13.0),
                        p.text_muted,
                    );

                    ui.add_space(12.0);
                    let choose_clicked = ui
                        .add(secondary_button(
                            tr.pick("Обрати файл .docx…", "Choose .docx file…"),
                            p,
                        ))
                        .clicked();

                    if (well_resp.clicked() || choose_clicked)
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Word Timetable", &["docx"])
                            .pick_file()
                    {
                        self.open_import_with_path(Some(path.display().to_string()));
                    }
                });
            });
    }

    fn show_filter_bar(&mut self, ui: &mut Ui, shown_count: usize, p: Palette) {
        let lang = self.lang();
        let tr = Tr::new(lang);
        let search_id = egui::Id::new("autopara_search_input");

        if ui.ctx().input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
            ui.ctx().memory_mut(|m| m.request_focus(search_id));
        }

        let has_groups = self.cache.course_groups.len() > 1;
        let total_w = ui.available_width();
        let gap = 12.0_f32;
        let grp_w = if has_groups { 196.0 } else { 0.0 };
        let subj_w = (total_w * 0.24).clamp(190.0, 265.0);
        let search_w =
            (total_w - subj_w - grp_w - if has_groups { gap * 2.0 } else { gap }).max(180.0);

        // Exact mathematical rects guarantee pixel-identical 12px gaps between all pills.
        let (row_rect, _) = ui.allocate_exact_size(vec2(total_w, 38.0), Sense::hover());
        let s_rect = egui::Rect::from_min_size(row_rect.min, vec2(search_w, 38.0));
        let sub_rect = egui::Rect::from_min_size(
            pos2(s_rect.right() + gap, row_rect.top()),
            vec2(subj_w, 38.0),
        );
        let grp_rect = egui::Rect::from_min_size(
            pos2(sub_rect.right() + gap, row_rect.top()),
            vec2(grp_w, 38.0),
        );

        // 1. Search pill (38px, radius 19, smooth hover & focus border morph)
        let s_hov_resp = ui.interact(s_rect, ui.id().with("search_pill_bg"), Sense::hover());
        let focused = ui.ctx().memory(|m| m.has_focus(search_id));
        let s_focus_t = self
            .morph
            .smooth_hover(ui.ctx(), 3100, focused || s_hov_resp.hovered());
        ui.painter().rect(
            s_rect,
            CornerRadius::same(19),
            palette::blend(p.surface, p.accent_soft, s_focus_t * 0.35),
            Stroke::new(
                1.0_f32,
                palette::blend(p.border, p.accent, if focused { 0.9 } else { s_focus_t * 0.35 }),
            ),
            StrokeKind::Inside,
        );
        icons::draw_search(
            ui.painter(),
            pos2(s_rect.left() + 20.0, s_rect.center().y),
            6.2,
            palette::blend(p.text_faint, p.text_muted, s_focus_t),
        );
        let active_filter = !self.search_text.trim().is_empty() || !self.subject_filter.is_empty();
        let count_w = if active_filter { 92.0 } else { 16.0 };
        let input_rect = egui::Rect::from_min_max(
            pos2(s_rect.left() + 38.0, s_rect.top() + 1.0),
            pos2(s_rect.right() - count_w, s_rect.bottom() - 1.0),
        );
        // `new_child` keeps the parent cursor at `row_rect.bottom()` instead of pulling it back up.
        let mut child_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(input_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        let resp = child_ui.add_sized(
            vec2(input_rect.width(), 22.0),
            egui::TextEdit::singleline(&mut self.search_text)
                .id(search_id)
                .frame(false)
                .vertical_align(Align::Center)
                .hint_text(tr.pick(
                    "Пошук: предмет, назва або організатор",
                    "Search: subject, title, or host",
                )),
        );
        if resp.lost_focus() && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)) {
            let dir = if ui.ctx().input(|i| i.modifiers.shift) { -1 } else { 1 };
            let flt = LessonFilter {
                text: self.search_text.clone(),
                subject: self.subject_filter.clone(),
            };
            if let Some(found) = filters::find_match(&self.cache.lessons, &flt, self.anchor, dir) {
                self.morph.begin_calendar_morph(if dir > 0 { 1 } else { -1 });
                self.anchor = found;
                self.reload();
            }
            ui.ctx().memory_mut(|m| m.request_focus(search_id));
        }
        if active_filter {
            ui.painter().text(
                pos2(s_rect.right() - 16.0, s_rect.center().y),
                Align2::RIGHT_CENTER,
                tr.found_count(shown_count),
                FontId::proportional(11.5),
                p.text_muted,
            );
        }

        // 2. Subject pill dropdown (exact 12px after Search, smooth hover/press + rotating chevron)
        let sel_subj = if self.subject_filter.is_empty() {
            tr.pick("Усі предмети", "All subjects").to_string()
        } else {
            self.subject_filter.clone()
        };
        let subj_open = matches!(self.open_dropdown, Some((FilterDropdown::Subject, _)));
        let sub_resp = ui.interact(sub_rect, ui.id().with("filter_subj_pill"), Sense::click());
        let sub_hov = self
            .morph
            .smooth_hover(ui.ctx(), 3101, sub_resp.hovered() || subj_open);
        let sub_prs = self
            .morph
            .smooth_press(ui.ctx(), 3101, sub_resp.is_pointer_button_down_on());
        let sub_open_t = self
            .morph
            .smooth_val(ui.ctx(), 3111, if subj_open { 1.0 } else { 0.0 }, 18.0);
        let draw_sub = sub_rect.shrink(sub_prs * 1.1);
        ui.painter().rect(
            draw_sub,
            CornerRadius::same(19),
            palette::blend(p.surface, p.accent_soft, sub_hov * 0.65),
            Stroke::new(
                1.0_f32,
                palette::blend(p.border, p.accent, if subj_open { 0.9 } else { sub_hov * 0.38 }),
            ),
            StrokeKind::Inside,
        );
        let sp = ui.painter().with_clip_rect(egui::Rect::from_min_max(
            pos2(draw_sub.left() + 16.0, draw_sub.top()),
            pos2(draw_sub.right() - 32.0, draw_sub.bottom()),
        ));
        sp.text(
            pos2(draw_sub.left() + 16.0, draw_sub.center().y),
            Align2::LEFT_CENTER,
            sel_subj,
            FontId::proportional(13.0),
            p.text,
        );
        icons::draw_chevron_down(
            ui.painter(),
            pos2(draw_sub.right() - 18.0, draw_sub.center().y),
            5.2,
            sub_open_t,
            palette::blend(p.text_muted, p.text, sub_hov * 0.6),
        );
        if sub_resp.clicked() {
            if subj_open {
                self.open_dropdown = None;
            } else {
                self.morph.begin_popup();
                self.open_dropdown = Some((FilterDropdown::Subject, sub_rect));
            }
        }

        // 3. Group pill dropdown (exact 12px after Subjects, smooth hover/press + rotating chevron)
        if has_groups {
            let active_name = self
                .cache
                .shown_group
                .as_ref()
                .map(|g| g.name.as_str())
                .unwrap_or("");
            let grp_open = matches!(self.open_dropdown, Some((FilterDropdown::Group, _)));
            let grp_resp = ui.interact(grp_rect, ui.id().with("filter_grp_pill"), Sense::click());
            let grp_hov = self
                .morph
                .smooth_hover(ui.ctx(), 3102, grp_resp.hovered() || grp_open);
            let grp_prs = self
                .morph
                .smooth_press(ui.ctx(), 3102, grp_resp.is_pointer_button_down_on());
            let grp_open_t = self
                .morph
                .smooth_val(ui.ctx(), 3112, if grp_open { 1.0 } else { 0.0 }, 18.0);
            let draw_grp = grp_rect.shrink(grp_prs * 1.1);
            ui.painter().rect(
                draw_grp,
                CornerRadius::same(19),
                palette::blend(p.surface, p.accent_soft, grp_hov * 0.65),
                Stroke::new(
                    1.0_f32,
                    palette::blend(p.border, p.accent, if grp_open { 0.9 } else { grp_hov * 0.38 }),
                ),
                StrokeKind::Inside,
            );
            let gp = ui.painter().with_clip_rect(egui::Rect::from_min_max(
                pos2(draw_grp.left() + 16.0, draw_grp.top()),
                pos2(draw_grp.right() - 32.0, draw_grp.bottom()),
            ));
            gp.text(
                pos2(draw_grp.left() + 16.0, draw_grp.center().y),
                Align2::LEFT_CENTER,
                active_name,
                FontId::proportional(13.0),
                p.text,
            );
            icons::draw_chevron_down(
                ui.painter(),
                pos2(draw_grp.right() - 18.0, draw_grp.center().y),
                5.2,
                grp_open_t,
                palette::blend(p.text_muted, p.text, grp_hov * 0.6),
            );
            if grp_resp.clicked() {
                if grp_open {
                    self.open_dropdown = None;
                } else {
                    self.morph.begin_popup();
                    self.open_dropdown = Some((FilterDropdown::Group, grp_rect));
                }
            }
        }
    }

    fn show_filter_dropdown_popup(&mut self, ctx: &egui::Context, p: Palette) {
        let Some((kind, anchor_rect)) = self.open_dropdown else {
            return;
        };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.open_dropdown = None;
            return;
        }
        let (t, anim) = self.morph.popup_progress();
        if anim {
            ctx.request_repaint();
        }
        let tr = Tr::new(self.lang());
        let popup_pos = pos2(anchor_rect.left(), anchor_rect.bottom() + 6.0 + (1.0 - t) * 6.0);
        let width = anchor_rect.width().max(210.0);

        let mut picked_subject: Option<String> = None;
        let mut picked_group: Option<i64> = None;
        let subjects_list = if kind == FilterDropdown::Subject {
            filters::subjects(&self.cache.lessons)
        } else {
            Vec::new()
        };
        let active_gid = self
            .cache
            .shown_group
            .as_ref()
            .map(|g| g.id)
            .unwrap_or(0);
        let groups_list = if kind == FilterDropdown::Group {
            self.cache.course_groups.clone()
        } else {
            Vec::new()
        };
        let cur_subject = self.subject_filter.clone();

        let area_resp = egui::Area::new(egui::Id::new("filter_dropdown_popup"))
            .fixed_pos(popup_pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.multiply_opacity(t);
                Frame::NONE
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(width - 12.0);
                        egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                            let mut row_idx: u64 = 0;
                            let mut draw_opt = |ui: &mut Ui, label: &str, selected: bool| -> bool {
                                row_idx += 1;
                                let key = 0xD00D_0000_u64 + row_idx;
                                let (r, resp) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), 32.0),
                                    Sense::click(),
                                );
                                let hov = self.morph.smooth_hover(ctx, key, resp.hovered());
                                let prs = self
                                    .morph
                                    .smooth_press(ctx, key, resp.is_pointer_button_down_on());
                                let fill = if selected {
                                    p.accent_fill
                                } else if hov > 0.01 {
                                    p.accent_soft.linear_multiply(hov)
                                } else {
                                    Color32::TRANSPARENT
                                };
                                let fg = if selected { p.accent_text } else { p.text };
                                ui.painter().rect_filled(
                                    r.shrink(prs * 1.1),
                                    CornerRadius::same(8),
                                    fill,
                                );
                                let lp = ui.painter().with_clip_rect(r.shrink2(vec2(10.0, 0.0)));
                                lp.text(
                                    pos2(
                                        r.left() + 10.0 + if selected { 0.0 } else { hov * 2.5 },
                                        r.center().y,
                                    ),
                                    Align2::LEFT_CENTER,
                                    label,
                                    if selected {
                                        semibold_font(13.0)
                                    } else {
                                        FontId::proportional(13.0)
                                    },
                                    fg,
                                );
                                resp.clicked()
                            };

                            match kind {
                                FilterDropdown::Subject => {
                                    let all_lbl = tr.pick("Усі предмети", "All subjects");
                                    if draw_opt(ui, all_lbl, cur_subject.is_empty()) {
                                        picked_subject = Some(String::new());
                                    }
                                    for subj in &subjects_list {
                                        if draw_opt(ui, subj, cur_subject == *subj) {
                                            picked_subject = Some(subj.clone());
                                        }
                                    }
                                }
                                FilterDropdown::Group => {
                                    for g in &groups_list {
                                        if draw_opt(ui, &g.name, g.id == active_gid) {
                                            picked_group = Some(g.id);
                                        }
                                    }
                                }
                            }
                        });
                    });
            });

        if let Some(subj) = picked_subject {
            self.subject_filter = subj;
            self.open_dropdown = None;
        }
        if let Some(gid) = picked_group {
            self.view_group_id = Some(gid);
            self.open_dropdown = None;
            self.morph.begin_calendar_morph(0);
            self.reload();
        }

        if ctx.input(|i| i.pointer.any_pressed())
            && let Some(pt) = ctx.pointer_latest_pos()
            && !area_resp.response.rect.contains(pt)
            && !anchor_rect.contains(pt)
        {
            self.open_dropdown = None;
        }
    }

    fn show_update_banner(&mut self, ui: &mut Ui, p: Palette) {
        let state = self.update_banner.clone();
        if matches!(state, UpdateBannerState::Hidden) {
            return;
        }
        let ctx = ui.ctx().clone();
        let lang = self.lang();
        let tr = Tr::new(lang);
        let platform = TargetPlatform::current();

        Frame::NONE
            .fill(p.surface)
            .stroke(Stroke::new(1.0_f32, p.border))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(egui::Margin::symmetric(16, 10))
            .show(ui, |ui| match state {
                UpdateBannerState::Hidden => {}
                UpdateBannerState::Available(release) => {
                    ui.allocate_ui_with_layout(
                        vec2(ui.available_width(), 38.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                let title = if lang.is_en() {
                                    format!("New AutoPara {} available", release.version)
                                } else {
                                    format!("Доступна нова версія AutoPara {}", release.version)
                                };
                                ui.label(RichText::new(title).font(semibold_font(13.5)).color(p.text));
                                let note = updater::summary_line(&release.notes);
                                let detail = if !note.is_empty() {
                                    note
                                } else if lang.is_en() {
                                    format!("Current version: {CURRENT_VERSION}.")
                                } else {
                                    format!("Зараз у вас {CURRENT_VERSION}.")
                                };
                                ui.label(RichText::new(detail).size(12.0).color(p.text_muted));
                            });

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                if platform.installable() && release.asset.is_some() {
                                    if ui
                                        .add(primary_button(tr.pick("Завантажити", "Download"), p))
                                        .clicked()
                                    {
                                        self.start_update_download(release.clone(), &ctx);
                                    }
                                } else if !release.page_url.is_empty()
                                    && ui
                                        .add(primary_button(
                                            tr.pick("Відкрити сторінку", "Open release page"),
                                            p,
                                        ))
                                        .clicked()
                                {
                                    launcher::open_url(Some(&release.page_url));
                                }

                                if ui
                                    .add(plain_button(tr.pick("Пізніше", "Later"), p))
                                    .clicked()
                                {
                                    self.update_banner = UpdateBannerState::Hidden;
                                }
                                if ui
                                    .add(plain_button(
                                        tr.pick("Пропустити версію", "Skip version"),
                                        p,
                                    ))
                                    .clicked()
                                {
                                    if let Ok(guard) = self.backend.lock() {
                                        let _ = guard
                                            .storage
                                            .set_setting("skipped_update_version", &release.version);
                                    }
                                    self.update_banner = UpdateBannerState::Hidden;
                                    self.reload();
                                }
                                if !release.page_url.is_empty()
                                    && platform.installable()
                                    && release.asset.is_some()
                                    && ui
                                        .add(plain_button(tr.pick("Що нового", "What's new"), p))
                                        .clicked()
                                {
                                    launcher::open_url(Some(&release.page_url));
                                }
                            });
                        },
                    );
                }
                UpdateBannerState::Downloading { release, percent } => {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.horizontal(|ui| {
                            let title = if lang.is_en() {
                                format!("Downloading AutoPara {}…", release.version)
                            } else {
                                format!("Завантажую AutoPara {}…", release.version)
                            };
                            ui.label(RichText::new(title).font(semibold_font(13.5)).color(p.text));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("{percent}%"))
                                        .size(12.0)
                                        .color(p.text_muted),
                                );
                            });
                        });
                        let (bar_rect, _) =
                            ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
                        ui.painter()
                            .rect_filled(bar_rect, CornerRadius::same(3), p.sunken);
                        let fill_w = bar_rect.width() * (f32::from(percent) / 100.0);
                        if fill_w > 1.0 {
                            let fill_rect = egui::Rect::from_min_size(bar_rect.min, vec2(fill_w, 6.0));
                            ui.painter()
                                .rect_filled(fill_rect, CornerRadius::same(3), p.accent_fill);
                        }
                    });
                }
                UpdateBannerState::Ready { release, path } => {
                    ui.allocate_ui_with_layout(
                        vec2(ui.available_width(), 38.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            let restarts = platform.restarts_on_apply();
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                let title = if lang.is_en() {
                                    format!("Update {} downloaded", release.version)
                                } else {
                                    format!("Оновлення {} завантажено", release.version)
                                };
                                ui.label(RichText::new(title).font(semibold_font(13.5)).color(p.text));
                                let detail = if restarts {
                                    tr.pick(
                                        "Встановлення закриє AutoPara і запустить нову версію.",
                                        "Installing will close AutoPara and launch the new version.",
                                    )
                                } else {
                                    tr.pick(
                                        "Відкрийте образ і перетягніть AutoPara у «Програми», замінивши стару.",
                                        "Open the disk image and drag AutoPara into Applications to replace the old one.",
                                    )
                                };
                                ui.label(RichText::new(detail).size(12.0).color(p.text_muted));
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                let btn_lbl = if restarts {
                                    tr.pick("Встановити й перезапустити", "Install & restart")
                                } else {
                                    tr.pick("Відкрити образ", "Open disk image")
                                };
                                if ui.add(primary_button(btn_lbl, p)).clicked() {
                                    match updater::apply(&path, &self.updates_dir) {
                                        Ok(true) => {
                                            self.should_quit.store(true, Ordering::Relaxed);
                                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                        }
                                        Ok(false) => {
                                            self.update_banner = UpdateBannerState::Hidden;
                                        }
                                        Err(err) => {
                                            self.update_status = Some(err.to_string());
                                        }
                                    }
                                }
                                if ui
                                    .add(plain_button(tr.pick("Пізніше", "Later"), p))
                                    .clicked()
                                {
                                    self.update_banner = UpdateBannerState::Hidden;
                                }
                            });
                        },
                    );
                }
            });
    }

    fn show_catchup_banner(&mut self, ui: &mut Ui, p: Palette) {
        let Some(first) = self.cache.pending_catchups.first().cloned() else {
            return;
        };
        let queue_extra = self.cache.pending_catchups.len().saturating_sub(1);
        let tr = Tr::new(self.lang());

        Frame::NONE
            .fill(p.banner_bg)
            .stroke(Stroke::new(1.0_f32, p.banner_border))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(egui::Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let msg = format!(
                        "{} {} ({})",
                        tr.pick("Вже почалося:", "Already started:"),
                        first.subject,
                        first.start_time
                    );
                    ui.label(RichText::new(msg).color(p.banner_text).strong().size(13.5));
                    if queue_extra > 0 {
                        ui.label(
                            RichText::new(tr.queue_more(queue_extra))
                                .color(p.banner_text)
                                .size(12.0),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(plain_button(tr.pick("Закрити", "Dismiss"), p))
                            .clicked()
                        {
                            let today = Local::now().date_naive();
                            self.handle_calendar_action(CalendarAction::MarkLesson {
                                lesson_id: first.lesson_id,
                                day: today,
                                status: STATUS_MISSED.to_string(),
                            });
                        }
                        if ui
                            .add(primary_button(tr.pick("Підключитися зараз", "Join now"), p))
                            .clicked()
                        {
                            let today = Local::now().date_naive();
                            self.handle_calendar_action(CalendarAction::OpenLesson {
                                lesson_id: first.lesson_id,
                                day: today,
                            });
                        }
                    });
                });
            });
    }
}

impl eframe::App for AutoParaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.morph.tick_frame();
        platform::capture_hwnd_if_needed();

        if self.start_hidden && !self.applied_initial_hide {
            self.applied_initial_hide = true;
            platform::hide_main_window(ctx);
        }

        // Intercept window X button -> hide to tray instead of quitting (unless tray Quit pressed)
        if ctx.input(|i| i.viewport().close_requested()) && !self.should_quit.load(Ordering::Relaxed)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            platform::hide_main_window(ctx);
        }

        // Refresh cache if background scheduler changed occurrences / catchups
        let was_dirty = self
            .backend
            .lock()
            .map(|mut g| {
                let d = g.dirty;
                g.dirty = false;
                d
            })
            .unwrap_or(false);
        if was_dirty {
            self.reload();
        }

        // Handle .docx drag-and-drop anywhere onto the window
        let dropped_docx = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.as_ref())
                .find(|p| {
                    p.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("docx"))
                })
                .map(|p| p.display().to_string())
        });
        if let Some(docx) = dropped_docx {
            self.open_import_with_path(Some(docx));
        }

        let p = palette::dark_palette();
        if !self.visuals_applied {
            palette::apply_visuals(ctx, p);
            self.visuals_applied = true;
        }

        let title = Tr::new(self.lang()).pick(
            "AutoPara — автозапуск созвонів і пар",
            "AutoPara — auto-join calls & classes",
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.to_string()));

        // Poll async GitHub update worker messages
        self.poll_update_messages(ctx);

        // Unified top bar (title + < > + clock + action capsule + window controls)
        self.show_header_bar(ctx, p);
        handle_frameless_resize(ctx);

        // Full-width main content area aligned flush with the 16px outer grid.
        egui::CentralPanel::default()
            .frame(
                Frame::NONE
                    .fill(p.canvas)
                    .inner_margin(egui::Margin {
                        left: 16,
                        right: 16,
                        top: 6,
                        bottom: 16,
                    }),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 14.0);

                if self.cache.selected_group.is_none() {
                    self.show_update_banner(ui, p);
                    self.show_catchup_banner(ui, p);
                    self.show_landing(ui, p);
                    return;
                }

                let flt = LessonFilter {
                    text: self.search_text.clone(),
                    subject: self.subject_filter.clone(),
                };
                let shown_lessons = filters::apply(&self.cache.lessons, &flt);

                self.show_filter_bar(ui, shown_lessons.len(), p);
                self.show_update_banner(ui, p);
                self.show_catchup_banner(ui, p);

                let cur_lang = self.lang();
                let action = calendar::show_calendar(
                    ui,
                    self.anchor,
                    &shown_lessons,
                    self.cache.lessons.len(),
                    &self.cache.occurrences,
                    self.cache.next_up_id,
                    &mut self.drag,
                    &mut self.popup,
                    &mut self.morph,
                    p,
                    cur_lang,
                );
                self.handle_calendar_action(action);
            });

        // Custom pill filter dropdown popup (Subject / Group)
        self.show_filter_dropdown_popup(ctx, p);

        // Active Modal (Edit / Settings / Group / Import / MoveScope / DeleteConfirm)
        let mut outcome = ModalOutcome::None;
        let lang = self.lang();
        if self.active_modal.is_some() {
            let (modal_t, modal_anim) = self.morph.modal_progress();
            if modal_anim {
                ctx.request_repaint();
            }
            let gcal_status = self.gcal.as_ref().map(|g| g.snapshot());
            if let Some(modal) = self.active_modal.as_mut() {
                outcome = match modal {
                    ActiveModal::Edit(m) => modals::show_edit_modal(ctx, m, p, lang, modal_t),
                    ActiveModal::Settings(m) => modals::show_settings_modal(
                        ctx,
                        m,
                        self.update_status.as_deref(),
                        self.update_checking,
                        gcal_status.as_ref(),
                        self.cache
                            .courses
                            .iter()
                            .any(|c| c.ordinal != PERSONAL_COURSE_ORDINAL),
                        p,
                        lang,
                        modal_t,
                    ),
                    ActiveModal::Group(m) => modals::show_group_modal(
                        ctx,
                        m,
                        &self.cache.courses,
                        &self.cache.all_groups,
                        p,
                        lang,
                        modal_t,
                    ),
                    ActiveModal::Import(m) => modals::show_import_modal(ctx, m, p, lang, modal_t),
                    ActiveModal::MoveScope(m) => {
                        modals::show_move_scope_modal(ctx, m, p, lang, modal_t)
                    }
                    ActiveModal::DeleteConfirm(m) => {
                        modals::show_delete_confirm_modal(ctx, m, p, lang, modal_t)
                    }
                };
            }
        }

        match outcome {
            ModalOutcome::None => {}
            ModalOutcome::Close => self.active_modal = None,
            ModalOutcome::SaveEdit(res) => {
                self.active_modal = None;
                self.save_edit(res);
            }
            ModalOutcome::SaveSettings(s) => {
                self.active_modal = None;
                self.save_settings(s);
            }
            ModalOutcome::CheckUpdatesNow => {
                self.request_manual_update_check(ctx);
            }
            ModalOutcome::GcalSignIn => {
                if let Some(g) = &self.gcal {
                    g.sign_in(ctx);
                }
            }
            ModalOutcome::DeleteTimetable => {
                if let Ok(mut g) = self.backend.lock() {
                    let _ = g.storage.delete_imported_timetables();
                    g.pending_catchups.clear();
                }
                self.view_group_id = None;
                self.reload();
            }
            ModalOutcome::GcalSignOut => {
                if let Some(g) = &self.gcal {
                    g.sign_out(&self.backend);
                }
            }
            ModalOutcome::SelectGroup {
                course_id,
                group_id,
            } => {
                self.active_modal = None;
                if let Ok(guard) = self.backend.lock() {
                    let _ = guard
                        .storage
                        .set_setting("selected_course_id", &course_id.to_string());
                    let _ = guard
                        .storage
                        .set_setting("selected_group_id", &group_id.to_string());
                }
                self.view_group_id = None;
                self.reload();
            }
            ModalOutcome::PickDocxForImport => {
                if let Some(file) = rfd::FileDialog::new()
                    .add_filter("Word Timetable", &["docx"])
                    .pick_file()
                    && let Some(ActiveModal::Import(mut imp)) = self.active_modal.take()
                {
                    self.populate_import_modal(&mut imp, file.display().to_string());
                    self.active_modal = Some(ActiveModal::Import(imp));
                }
            }
            ModalOutcome::CommitImport {
                path,
                ordinal,
                group_name,
            } => {
                self.active_modal = None;
                let dur = self.cache.settings.class_duration_minutes;
                if let Ok(parsed) = schedule_parser::parse_file(&path, Some(dur))
                    && let Ok(mut guard) = self.backend.lock()
                    && guard.storage.import_courses(&parsed, &path).is_ok()
                    && let Ok(courses) = guard.storage.courses()
                    && let Some(c) = courses.iter().find(|c| c.ordinal == ordinal)
                {
                    let _ = guard
                        .storage
                        .set_setting("selected_course_id", &c.id.to_string());
                    if let Ok(groups) = guard.storage.groups(c.id)
                        && let Some(g) = groups.iter().find(|g| g.name == group_name)
                    {
                        let _ = guard
                            .storage
                            .set_setting("selected_group_id", &g.id.to_string());
                    }
                    guard.pending_catchups.clear();
                }
                self.view_group_id = None;
                self.reload();
            }
            ModalOutcome::ConfirmMove { modal, only_one } => {
                self.active_modal = None;
                if let Ok(guard) = self.backend.lock() {
                    let pair = pair_slot(&modal.start_time);
                    if only_one {
                        let _ = guard.storage.move_occurrence(
                            modal.lesson_id,
                            modal.from_day,
                            modal.to_day,
                            pair,
                            &modal.start_time,
                            &modal.end_time,
                        );
                    } else {
                        let wd = i64::from(modal.to_day.weekday().num_days_from_monday());
                        let _ = guard.storage.move_lesson(
                            modal.lesson_id,
                            wd,
                            pair,
                            &modal.start_time,
                            &modal.end_time,
                            Some(modal.to_day),
                        );
                    }
                }
                self.reload();
            }
            ModalOutcome::ConfirmDelete(lesson_id) => {
                self.active_modal = None;
                if let Ok(mut guard) = self.backend.lock() {
                    let _ = guard.storage.delete_lesson(lesson_id);
                    guard.pending_catchups.retain(|c| c.lesson_id != lesson_id);
                }
                self.reload();
            }
        }

        // Keep clock & now-line smooth (1Hz repaint)
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }
}

fn load_cache(
    backend: &Arc<Mutex<SharedBackend>>,
    anchor: NaiveDate,
    view_group_id: Option<i64>,
) -> CachedView {
    let guard = backend.lock().expect("lock backend");
    let settings = guard.storage.settings().unwrap_or_default();
    let courses = guard.storage.courses().unwrap_or_default();

    let mut all_groups = Vec::new();
    for c in &courses {
        for g in guard.storage.groups(c.id).unwrap_or_default() {
            let count = guard.storage.lessons_for_group(g.id).map(|l| l.len()).unwrap_or(0);
            all_groups.push((g, count));
        }
    }

    let selected_group = settings
        .selected_group_id
        .and_then(|gid| guard.storage.group(gid).ok().flatten());
    let selected_course = selected_group
        .as_ref()
        .and_then(|g| guard.storage.course(g.course_id).ok().flatten());
    let course_groups = selected_course
        .as_ref()
        .map(|c| guard.storage.groups(c.id).unwrap_or_default())
        .unwrap_or_default();

    let shown_group = match (view_group_id, selected_group.as_ref()) {
        (Some(vgid), Some(sel)) if vgid != sel.id => {
            match guard.storage.group(vgid).ok().flatten() {
                Some(picked) if picked.course_id == sel.course_id => Some(picked),
                _ => selected_group.clone(),
            }
        }
        _ => selected_group.clone(),
    };

    let lessons = shown_group
        .as_ref()
        .map(|g| guard.storage.lessons_for_group(g.id).unwrap_or_default())
        .unwrap_or_default();

    let first = week_start(anchor - Duration::days(35));
    let last = anchor + Duration::days(42);
    let occurrences: HashMap<(i64, String), String> = guard
        .storage
        .occurrences_between(first, last)
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, v.status))
        .collect();

    let now = Local::now().naive_local();
    let today = now.date();
    let settled: HashSet<i64> = guard
        .storage
        .occurrences_on(today)
        .unwrap_or_default()
        .keys()
        .copied()
        .collect();
    let my_lessons = selected_group
        .as_ref()
        .map(|g| guard.storage.lessons_for_group(g.id).unwrap_or_default())
        .unwrap_or_default();
    let nu = nextup::find(&my_lessons, now, &settled);
    let next_up_id = nu.as_ref().map(|n| n.lesson_id);
    let next_up_label = nu
        .as_ref()
        .map(|n| nextup::label(n, &settings.language));

    CachedView {
        settings,
        courses,
        course_groups,
        all_groups,
        selected_course,
        selected_group,
        shown_group,
        lessons,
        occurrences,
        next_up_id,
        next_up_label,
        pending_catchups: guard.pending_catchups.clone(),
    }
}

fn handle_frameless_resize(ctx: &egui::Context) {
    let screen = ctx.screen_rect();
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }

    let edge = 6.0;
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    let left = pos.x <= screen.left() + edge;
    let right = pos.x >= screen.right() - edge;
    let top = pos.y <= screen.top() + edge;
    let bottom = pos.y >= screen.bottom() - edge;

    let (dir, cursor) = match (left, right, top, bottom) {
        (true, _, true, _) => (
            egui::ResizeDirection::NorthWest,
            egui::CursorIcon::ResizeNorthWest,
        ),
        (_, true, true, _) => (
            egui::ResizeDirection::NorthEast,
            egui::CursorIcon::ResizeNorthEast,
        ),
        (true, _, _, true) => (
            egui::ResizeDirection::SouthWest,
            egui::CursorIcon::ResizeSouthWest,
        ),
        (_, true, _, true) => (
            egui::ResizeDirection::SouthEast,
            egui::CursorIcon::ResizeSouthEast,
        ),
        (true, _, _, _) => (egui::ResizeDirection::West, egui::CursorIcon::ResizeHorizontal),
        (_, true, _, _) => (egui::ResizeDirection::East, egui::CursorIcon::ResizeHorizontal),
        (_, _, true, _) => (egui::ResizeDirection::North, egui::CursorIcon::ResizeVertical),
        (_, _, _, true) => (egui::ResizeDirection::South, egui::CursorIcon::ResizeVertical),
        _ => return,
    };

    ctx.set_cursor_icon(cursor);
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
    }
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".to_owned(),
        Arc::new(FontData::from_static(INTER_REGULAR)),
    );
    fonts.font_data.insert(
        "InterSemiBold".to_owned(),
        Arc::new(FontData::from_static(INTER_SEMIBOLD)),
    );
    fonts.font_data.insert(
        "InterBold".to_owned(),
        Arc::new(FontData::from_static(INTER_BOLD)),
    );
    if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
        list.insert(0, "Inter".to_owned());
    }
    fonts.families.insert(
        FontFamily::Name("semibold".into()),
        vec!["InterSemiBold".to_owned(), "Inter".to_owned()],
    );
    fonts.families.insert(
        FontFamily::Name("bold".into()),
        vec!["InterBold".to_owned(), "InterSemiBold".to_owned(), "Inter".to_owned()],
    );
    ctx.set_fonts(fonts);
}

fn load_logo_texture(ctx: &egui::Context) -> Option<TextureHandle> {
    let img = image::load_from_memory(LOGO_PNG).ok()?.into_rgba8();
    let resized = image::imageops::resize(&img, 128, 128, image::imageops::FilterType::Lanczos3);
    let color_img = egui::ColorImage::from_rgba_unmultiplied([128, 128], resized.as_raw());
    Some(ctx.load_texture("autopara_logo", color_img, egui::TextureOptions::LINEAR))
}

fn load_window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(LOGO_PNG).ok()?.into_rgba8();
    let resized = image::imageops::resize(&img, 64, 64, image::imageops::FilterType::Lanczos3);
    Some(egui::IconData {
        rgba: resized.into_raw(),
        width: 64,
        height: 64,
    })
}

fn main() -> eframe::Result<()> {
    let _instance_guard = match platform::acquire_single_instance() {
        Some(g) => g,
        None => return Ok(()),
    };

    let start_hidden = std::env::args().any(|a| a == "--hidden");
    let storage = Storage::open(default_db_path()).expect("open autopara sqlite database");
    if let Ok(settings) = storage.settings() {
        autostart::sync(settings.autostart_enabled);
    }

    let backend = Arc::new(Mutex::new(SharedBackend {
        storage,
        scheduler: SchedulerEngine::new(),
        pending_catchups: Vec::new(),
        dirty: false,
    }));

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("AutoPara")
        .with_decorations(false)
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([1100.0, 660.0]);
    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "AutoPara",
        options,
        Box::new(move |cc| Ok(Box::new(AutoParaApp::new(cc, backend, start_hidden)))),
    )
}
