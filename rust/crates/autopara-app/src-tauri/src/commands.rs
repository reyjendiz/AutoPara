//! The commands the interface calls (plan step 4.1). Every type crossing the boundary derives
//! `TS`; `cargo test` writes the TypeScript side into `ui/src/bindings`.
//!
//! Errors reach the interface as a short code (`invalid_url`, `not_found`, ...) or, for the rare
//! unexpected failure, the error's text; the interface turns codes into Ukrainian sentences.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use autopara_core::clock::Clock;
use autopara_core::db::Storage;
use autopara_core::engine::{EngineHandle, Shared};
use autopara_core::i18n::t;
use autopara_core::import::{
    import_timetable as store_timetable, read_timetable, select_group as choose,
};
use autopara_core::model::{
    CatchupMode, Course, Group, Lesson, Occurrence, OccurrenceStatus, Recurrence, Settings, Theme,
    UpdateMode, ViewMode, Weekday,
};
use autopara_core::scheduler::{Scheduler, is_openable};
use autopara_core::text::detect_provider;
use autopara_core::time::{TimeOfDay, pair_slot};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::autostart;
use crate::opener::SharedOpener;

pub type CommandResult<T> = Result<T, String>;

/// Everything the commands share.
pub struct AppState {
    pub shared: Arc<Shared>,
    pub clock: Arc<dyn Clock>,
    pub opener: SharedOpener,
    pub data_root: PathBuf,
    pub engine: Mutex<Option<EngineHandle>>,
}

impl AppState {
    pub(crate) fn storage(&self) -> MutexGuard<'_, Storage> {
        self.shared
            .storage
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Database first, then scheduler: the engine takes them in the same order.
    fn both(&self) -> (MutexGuard<'_, Storage>, MutexGuard<'_, Scheduler>) {
        let storage = self.storage();
        let scheduler = self
            .shared
            .scheduler
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        (storage, scheduler)
    }

    /// Something the scheduler depends on changed: look again now.
    fn wake(&self) {
        if let Some(engine) = self
            .engine
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            engine.wake();
        }
    }
}

pub(crate) fn failed(error: impl std::fmt::Display) -> String {
    tracing::warn!(%error, "command failed");
    error.to_string()
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub debug: bool,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: state.data_root.display().to_string(),
        debug: cfg!(debug_assertions),
    }
}

// ------------------------------------------------------------------ settings

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CommandResult<Settings> {
    state.storage().settings().map_err(failed)
}

/// What the settings screen saves.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct SettingsInput {
    pub lead_minutes: u32,
    pub class_duration_minutes: u32,
    pub catchup_mode: CatchupMode,
    pub notifications_enabled: bool,
    pub notify_minutes: u32,
    pub theme: Theme,
    pub update_mode: UpdateMode,
    pub autostart_enabled: bool,
    pub auto_join: bool,
    pub join_name: String,
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, input: SettingsInput) -> CommandResult<Settings> {
    if input.lead_minutes > 120
        || !(10..=240).contains(&input.class_duration_minutes)
        || !(1..=120).contains(&input.notify_minutes)
        || input.join_name.chars().count() > 60
        || input.join_name.chars().any(char::is_control)
    {
        return Err("out_of_range".into());
    }
    let autostart = autostart::sync(input.autostart_enabled).map_err(failed)?;
    let storage = state.storage();
    let flag = |on: bool| if on { "1" } else { "0" };
    for (key, value) in [
        ("lead_minutes", input.lead_minutes.to_string()),
        (
            "class_duration_minutes",
            input.class_duration_minutes.to_string(),
        ),
        ("catchup_mode", input.catchup_mode.as_str().to_owned()),
        (
            "notifications_enabled",
            flag(input.notifications_enabled).to_owned(),
        ),
        ("notify_minutes", input.notify_minutes.to_string()),
        ("theme", input.theme.as_str().to_owned()),
        ("update_mode", input.update_mode.as_str().to_owned()),
        ("autostart_enabled", flag(autostart).to_owned()),
        ("auto_join", flag(input.auto_join).to_owned()),
        ("join_name", input.join_name.trim().to_owned()),
    ] {
        storage.set_setting(key, &value).map_err(failed)?;
    }
    let settings = storage.settings().map_err(failed)?;
    drop(storage);
    state.wake();
    Ok(settings)
}

/// The calendar's view and theme toggle are saved as soon as they change.
#[tauri::command]
pub fn set_view(state: State<'_, AppState>, view: ViewMode) -> CommandResult<()> {
    state
        .storage()
        .set_setting("view_mode", view.as_str())
        .map_err(failed)
}

#[tauri::command]
pub fn set_theme(state: State<'_, AppState>, theme: Theme) -> CommandResult<()> {
    state
        .storage()
        .set_setting("theme", theme.as_str())
        .map_err(failed)
}

// --------------------------------------------------------------- the timetable

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct GroupSummary {
    pub group: Group,
    pub lessons: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CourseSummary {
    pub course: Course,
    pub groups: Vec<GroupSummary>,
}

/// Every imported course with its groups and how many classes each has.
#[tauri::command]
pub fn list_courses(state: State<'_, AppState>) -> CommandResult<Vec<CourseSummary>> {
    let storage = state.storage();
    storage
        .courses()
        .map_err(failed)?
        .into_iter()
        .map(|course| {
            let groups = storage
                .groups(course.id)
                .map_err(failed)?
                .into_iter()
                .map(|group| {
                    let lessons = storage.lessons_for_group(group.id).map_err(failed)?.len() as u32;
                    Ok(GroupSummary { group, lessons })
                })
                .collect::<CommandResult<_>>()?;
            Ok(CourseSummary { course, groups })
        })
        .collect()
}

#[tauri::command]
pub fn select_group(state: State<'_, AppState>, group_id: i64) -> CommandResult<()> {
    let storage = state.storage();
    let group = storage
        .group(group_id)
        .map_err(failed)?
        .ok_or("not_found")?;
    storage
        .set_setting("selected_course_id", &group.course_id.to_string())
        .map_err(failed)?;
    storage
        .set_setting("selected_group_id", &group.id.to_string())
        .map_err(failed)?;
    drop(storage);
    state.wake();
    Ok(())
}

#[tauri::command]
pub fn group_lessons(state: State<'_, AppState>, group_id: i64) -> CommandResult<Vec<Lesson>> {
    state.storage().lessons_for_group(group_id).map_err(failed)
}

#[tauri::command]
pub fn occurrences(
    state: State<'_, AppState>,
    from: NaiveDate,
    to: NaiveDate,
) -> CommandResult<Vec<Occurrence>> {
    state
        .storage()
        .occurrences_between(from, to)
        .map_err(failed)
}

// --------------------------------------------------------------------- import

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct GroupPreview {
    pub name: String,
    pub specialty: String,
    pub lessons: u32,
    pub shared: u32,
    pub without_link: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CoursePreview {
    pub ordinal: u32,
    pub name: String,
    pub lessons: u32,
    pub groups: Vec<GroupPreview>,
}

/// Read a document for the import screen, without storing anything.
#[tauri::command]
pub fn preview_timetable(
    state: State<'_, AppState>,
    path: String,
) -> CommandResult<Vec<CoursePreview>> {
    let minutes = state
        .storage()
        .settings()
        .map_err(failed)?
        .class_duration_minutes;
    let courses = read_timetable(Path::new(&path), minutes).map_err(failed)?;
    Ok(courses
        .into_iter()
        .map(|course| {
            let groups = course
                .groups
                .iter()
                .map(|group| {
                    let mine: Vec<_> = course
                        .lessons
                        .iter()
                        .filter(|l| l.group_names.contains(&group.name))
                        .collect();
                    GroupPreview {
                        name: group.name.clone(),
                        specialty: group.specialty.clone(),
                        lessons: mine.len() as u32,
                        shared: mine.iter().filter(|l| l.group_names.len() > 1).count() as u32,
                        without_link: mine.iter().filter(|l| l.url.is_none()).count() as u32,
                    }
                })
                .collect();
            CoursePreview {
                ordinal: course.ordinal,
                name: course.name,
                lessons: course.lessons.len() as u32,
                groups,
            }
        })
        .collect())
}

/// Import a document and choose the course (by ordinal) and group (by name) to show.
#[tauri::command]
pub fn import_timetable(
    state: State<'_, AppState>,
    path: String,
    ordinal: u32,
    group_name: String,
) -> CommandResult<()> {
    let source = PathBuf::from(&path);
    let mut storage = state.storage();
    let minutes = storage.settings().map_err(failed)?.class_duration_minutes;
    let courses = read_timetable(&source, minutes).map_err(failed)?;
    store_timetable(
        &mut storage,
        &state.data_root,
        &source,
        &courses,
        state.clock.now(),
    )
    .map_err(failed)?;
    if !choose(&storage, ordinal, &group_name).map_err(failed)? {
        return Err("not_found".into());
    }
    drop(storage);
    tracing::info!(ordinal, "timetable imported");
    state.wake();
    Ok(())
}

/// The system's file picker, filtered to Word documents.
#[tauri::command]
pub async fn pick_timetable(app: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let start = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|home| PathBuf::from(home).join("Desktop"));
    let mut dialog = app
        .dialog()
        .file()
        .set_title(t("dialog_pick_timetable"))
        .add_filter(t("dialog_word_documents"), &["docx"]);
    if let Some(start) = start.filter(|dir| dir.is_dir()) {
        dialog = dialog.set_directory(start);
    }
    dialog
        .blocking_pick_file()
        .and_then(|file| file.into_path().ok())
        .map(|path| path.display().to_string())
}

// -------------------------------------------------------------------- editing

/// A class as the editor saves it. The server works out the rest (slot, provider, course).
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct LessonInput {
    #[ts(type = "number | null")]
    pub id: Option<i64>,
    pub subject: String,
    pub teacher: String,
    pub url: Option<String>,
    pub start: TimeOfDay,
    pub end: TimeOfDay,
    pub weekday: Weekday,
    pub recurrence: Recurrence,
}

/// The class the editor's input describes, checked as the editor checks it (PORT_SPEC §E.12).
/// `existing` is the class being edited; `group` is the chosen group a new class joins.
fn build_lesson(
    input: LessonInput,
    existing: Option<&Lesson>,
    group: &Group,
) -> Result<Lesson, &'static str> {
    let subject = input.subject.trim().to_owned();
    if subject.is_empty() {
        return Err("subject_required");
    }
    let url = input
        .url
        .map(|url| url.trim().to_owned())
        .filter(|url| !url.is_empty());
    if url.is_some() && !is_openable(url.as_deref()) {
        return Err("invalid_url");
    }
    if input.end <= input.start {
        return Err("invalid_time");
    }
    if let Recurrence::WeeklyUntil { from, until } = input.recurrence
        && until < from
    {
        return Err("invalid_date");
    }
    let teacher = match input.teacher.trim() {
        "" => String::new(),
        name if name.starts_with('(') => name.to_owned(),
        name => format!("({name})"),
    };
    // An imported class is a row of the timetable and stays one.
    let recurrence = match existing {
        Some(lesson) if !lesson.is_manual => Recurrence::Weekly,
        _ => input.recurrence,
    };
    Ok(Lesson {
        id: existing.map_or(0, |lesson| lesson.id),
        course_id: existing.map_or(group.course_id, |lesson| lesson.course_id),
        weekday: recurrence.first_date().map_or(input.weekday, Weekday::of),
        pair: u32::from(pair_slot(input.start)),
        start: input.start,
        end: input.end,
        subject,
        teacher,
        provider: detect_provider(url.as_deref()),
        needs_link: url.is_none(),
        url,
        is_manual: existing.is_none_or(|lesson| lesson.is_manual),
        source_key: existing
            .map(|lesson| lesson.source_key.clone())
            .unwrap_or_default(),
        group_names: Vec::new(),
        recurrence,
        skip_dates: Default::default(),
    })
}

/// Create or change a class; returns its id.
#[tauri::command]
pub fn save_lesson(state: State<'_, AppState>, input: LessonInput) -> CommandResult<i64> {
    let storage = state.storage();
    let group_id = storage
        .settings()
        .map_err(failed)?
        .selected_group_id
        .ok_or("no_group")?;
    let group = storage.group(group_id).map_err(failed)?.ok_or("no_group")?;
    let existing = match input.id {
        Some(id) => Some(storage.lesson(id).map_err(failed)?.ok_or("not_found")?),
        None => None,
    };
    let lesson = build_lesson(input, existing.as_ref(), &group)?;
    let id = if existing.is_some() {
        storage.update_lesson(&lesson, None).map_err(failed)?;
        lesson.id
    } else {
        storage.add_lesson(&lesson, &[group_id]).map_err(failed)?
    };
    drop(storage);
    state.wake();
    Ok(id)
}

/// For a class that repeats: move only the date it was dragged from, or the whole series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MoveScope {
    OnlyThisDate,
    WholeSeries,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct MoveRequest {
    #[ts(type = "number")]
    pub lesson_id: i64,
    /// The date the class was dragged from, when the view knows it.
    pub from: Option<NaiveDate>,
    pub to: NaiveDate,
    pub start: TimeOfDay,
    pub scope: Option<MoveScope>,
}

/// What a drop does to the database.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MovePlan {
    Nothing,
    OneDate {
        from: NaiveDate,
        to: NaiveDate,
        pair: u32,
        start: TimeOfDay,
        end: TimeOfDay,
    },
    Whole {
        weekday: Weekday,
        pair: u32,
        start: TimeOfDay,
        end: TimeOfDay,
        on_date: Option<NaiveDate>,
    },
}

/// Drag and drop, with the Python app's rules (`MainWindow._lesson_dropped`): the class keeps its
/// length (or the class length, if it has none); a repeating class dragged from a date needs a
/// scope; dropping a class back where it was changes nothing.
fn plan_move(
    lesson: &Lesson,
    request: &MoveRequest,
    class_minutes: u32,
) -> Result<MovePlan, &'static str> {
    let minutes = match lesson.start.minutes_until(lesson.end) {
        0 => class_minutes,
        length => u32::from(length),
    };
    let (start, end) = (request.start, request.start.add_minutes(minutes));
    let pair = u32::from(pair_slot(start));
    if let Some(from) = request.from
        && lesson.repeats()
    {
        if from == request.to && lesson.start == start {
            return Ok(MovePlan::Nothing);
        }
        if request.scope.ok_or("scope_required")? == MoveScope::OnlyThisDate {
            return Ok(MovePlan::OneDate {
                from,
                to: request.to,
                pair,
                start,
                end,
            });
        }
    }
    let dated = lesson.recurrence.first_date().is_some();
    let unchanged = lesson.start == start
        && if dated {
            lesson.recurrence.first_date() == Some(request.to)
        } else {
            lesson.weekday == Weekday::of(request.to)
        };
    if unchanged {
        return Ok(MovePlan::Nothing);
    }
    Ok(MovePlan::Whole {
        weekday: Weekday::of(request.to),
        pair,
        start,
        end,
        on_date: dated.then_some(request.to),
    })
}

#[tauri::command]
pub fn move_lesson(state: State<'_, AppState>, request: MoveRequest) -> CommandResult<()> {
    let storage = state.storage();
    let lesson = storage
        .lesson(request.lesson_id)
        .map_err(failed)?
        .ok_or("not_found")?;
    let class_minutes = storage.settings().map_err(failed)?.class_duration_minutes;
    match plan_move(&lesson, &request, class_minutes)? {
        MovePlan::Nothing => return Ok(()),
        MovePlan::OneDate {
            from,
            to,
            pair,
            start,
            end,
        } => {
            storage
                .move_occurrence(lesson.id, from, to, pair, start, end)
                .map_err(failed)?;
        }
        MovePlan::Whole {
            weekday,
            pair,
            start,
            end,
            on_date,
        } => {
            storage
                .move_lesson(lesson.id, weekday, pair, start, end, on_date)
                .map_err(failed)?;
        }
    }
    drop(storage);
    state.wake();
    Ok(())
}

#[tauri::command]
pub fn delete_lesson(state: State<'_, AppState>, lesson_id: i64) -> CommandResult<()> {
    state.storage().delete_lesson(lesson_id).map_err(failed)?;
    state.wake();
    Ok(())
}

// --------------------------------------------------------------- joining, marks

/// Join a class now (a card click or "Підключитися зараз"). A date that has not come yet is not
/// claimed, so peeking at next week's class does not consume it.
#[tauri::command]
pub fn open_lesson(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    lesson_id: i64,
    date: NaiveDate,
) -> CommandResult<bool> {
    let now = state.clock.now();
    let mut opener = state.opener.clone();
    let opened = {
        let (storage, mut scheduler) = state.both();
        scheduler
            .open_now(
                &storage,
                lesson_id,
                date,
                date <= now.date(),
                now,
                &mut opener,
            )
            .map_err(failed)?
    };
    // Joining today's class by hand counts as AutoPara opening it.
    if opened && date == now.date() {
        crate::assist::listen_on_open(&app, lesson_id);
    }
    Ok(opened)
}

#[tauri::command]
pub fn mark_lesson(
    state: State<'_, AppState>,
    lesson_id: i64,
    date: NaiveDate,
    status: OccurrenceStatus,
) -> CommandResult<()> {
    let now = state.clock.now();
    let (storage, mut scheduler) = state.both();
    scheduler
        .mark(&storage, lesson_id, date, status, now)
        .map_err(failed)
}

#[tauri::command]
pub fn clear_mark(
    state: State<'_, AppState>,
    lesson_id: i64,
    date: NaiveDate,
) -> CommandResult<()> {
    let (storage, mut scheduler) = state.both();
    scheduler
        .clear_mark(&storage, lesson_id, date)
        .map_err(failed)?;
    drop((storage, scheduler));
    state.wake();
    Ok(())
}

// -------------------------------------------------------------------- updates

/// "Перевірити зараз" and the tray's "Перевірити оновлення": a manual check always answers.
#[tauri::command]
pub async fn check_updates(app: tauri::AppHandle) {
    crate::updates::check(app, true).await;
}

#[tauri::command]
pub async fn download_update(app: tauri::AppHandle) {
    crate::updates::download(app).await;
}

/// Installing is always the user's click; on success the app restarts and this never returns.
#[tauri::command]
pub fn install_update(app: tauri::AppHandle) -> CommandResult<()> {
    crate::updates::install(&app)
}

/// "Що нового" / "Відкрити сторінку": only this repository's release pages, nothing else.
#[tauri::command]
pub fn open_release_page(app: tauri::AppHandle, url: String) -> CommandResult<()> {
    use tauri_plugin_opener::OpenerExt;
    if !url.starts_with("https://github.com/reyjendiz/AutoPara/releases/") {
        return Err("invalid_url".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(failed)
}

/// "Пропустити версію": automatic checks stay quiet about exactly this version.
#[tauri::command]
pub fn skip_update(state: State<'_, AppState>, version: String) -> CommandResult<()> {
    state
        .storage()
        .set_setting("skipped_update_version", &version)
        .map_err(failed)
}

// ------------------------------------------------------------------ diagnostics

/// The text "copy diagnostics" puts on the clipboard.
#[tauri::command]
pub fn diagnostics(state: State<'_, AppState>) -> String {
    let storage = state.storage();
    let summary = match (storage.settings(), storage.courses()) {
        (Ok(settings), Ok(courses)) => format!(
            "Courses: {}; group: {:?}; lead: {} min; catch-up: {}; notifications: {}; theme: {}; updates: {}; autostart: {}",
            courses.len(),
            settings.selected_group_id,
            settings.lead_minutes,
            settings.catchup_mode,
            settings.notifications_enabled,
            settings.theme,
            settings.update_mode,
            autostart::is_enabled(),
        ),
        (Err(error), _) | (_, Err(error)) => format!("Database error: {error}"),
    };
    drop(storage);
    crate::logging::diagnostics(&state.data_root, &summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(text: &str) -> TimeOfDay {
        text.parse().unwrap()
    }

    fn date(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    fn group() -> Group {
        Group {
            id: 4,
            course_id: 2,
            name: "21".into(),
            specialty: String::new(),
            col_lo: 3,
            col_hi: 3,
        }
    }

    fn input(recurrence: Recurrence) -> LessonInput {
        LessonInput {
            id: None,
            subject: "  Логіка ".into(),
            teacher: "Гриценко В.М.".into(),
            url: Some(" https://zoom.us/j/9 ".into()),
            start: time("09:30"),
            end: time("10:50"),
            weekday: Weekday::Monday,
            recurrence,
        }
    }

    #[test]
    fn a_new_class_is_cleaned_up_and_derived() {
        let once = Recurrence::Once {
            date: date("2026-10-08"),
        };
        let lesson = build_lesson(input(once), None, &group()).unwrap();
        assert_eq!(lesson.subject, "Логіка");
        assert_eq!(lesson.teacher, "(Гриценко В.М.)");
        assert_eq!(lesson.url.as_deref(), Some("https://zoom.us/j/9"));
        assert_eq!(
            (lesson.pair, lesson.course_id, lesson.is_manual),
            (2, 2, true)
        );
        assert_eq!(
            lesson.weekday,
            Weekday::Thursday,
            "a dated class takes its date's weekday"
        );
    }

    #[test]
    fn the_editor_rules_are_enforced() {
        let mut bad = input(Recurrence::Weekly);
        bad.subject = " ".into();
        assert_eq!(build_lesson(bad, None, &group()), Err("subject_required"));
        let mut bad = input(Recurrence::Weekly);
        bad.url = Some("http://zoom.us/j/1".into());
        assert_eq!(build_lesson(bad, None, &group()), Err("invalid_url"));
        let mut bad = input(Recurrence::Weekly);
        bad.end = bad.start;
        assert_eq!(build_lesson(bad, None, &group()), Err("invalid_time"));
        let backwards = Recurrence::WeeklyUntil {
            from: date("2026-10-12"),
            until: date("2026-10-05"),
        };
        assert_eq!(
            build_lesson(input(backwards), None, &group()),
            Err("invalid_date")
        );
    }

    #[test]
    fn an_imported_class_stays_a_weekly_row() {
        let imported = Lesson {
            id: 7,
            is_manual: false,
            source_key: "2|0|2|логіка".into(),
            ..build_lesson(input(Recurrence::Weekly), None, &group()).unwrap()
        };
        let once = Recurrence::Once {
            date: date("2026-10-08"),
        };
        let edited = build_lesson(input(once), Some(&imported), &group()).unwrap();
        assert_eq!(
            (edited.id, edited.recurrence, edited.is_manual),
            (7, Recurrence::Weekly, false)
        );
        assert_eq!(edited.source_key, "2|0|2|логіка");
    }

    fn request(from: Option<&str>, to: &str, start: &str, scope: Option<MoveScope>) -> MoveRequest {
        MoveRequest {
            lesson_id: 1,
            from: from.map(date),
            to: date(to),
            start: time(start),
            scope,
        }
    }

    #[test]
    fn moving_keeps_the_length_and_asks_for_a_scope() {
        let weekly = build_lesson(input(Recurrence::Weekly), None, &group()).unwrap();
        let drop_on = |scope| request(Some("2026-10-05"), "2026-10-06", "11:20", scope);
        assert_eq!(
            plan_move(&weekly, &drop_on(None), 80),
            Err("scope_required")
        );
        assert_eq!(
            plan_move(&weekly, &drop_on(Some(MoveScope::OnlyThisDate)), 80),
            Ok(MovePlan::OneDate {
                from: date("2026-10-05"),
                to: date("2026-10-06"),
                pair: 3,
                start: time("11:20"),
                end: time("12:40"),
            })
        );
        assert_eq!(
            plan_move(&weekly, &drop_on(Some(MoveScope::WholeSeries)), 80),
            Ok(MovePlan::Whole {
                weekday: Weekday::Tuesday,
                pair: 3,
                start: time("11:20"),
                end: time("12:40"),
                on_date: None,
            })
        );
        let back = request(Some("2026-10-05"), "2026-10-05", "09:30", None);
        assert_eq!(plan_move(&weekly, &back, 80), Ok(MovePlan::Nothing));
    }

    #[test]
    fn a_one_off_moves_to_the_new_date_without_asking() {
        let once = build_lesson(
            input(Recurrence::Once {
                date: date("2026-10-08"),
            }),
            None,
            &group(),
        )
        .unwrap();
        assert_eq!(
            plan_move(
                &once,
                &request(Some("2026-10-08"), "2026-10-09", "13:00", None),
                80
            ),
            Ok(MovePlan::Whole {
                weekday: Weekday::Friday,
                pair: 4,
                start: time("13:00"),
                end: time("14:20"),
                on_date: Some(date("2026-10-09")),
            })
        );
    }
}
