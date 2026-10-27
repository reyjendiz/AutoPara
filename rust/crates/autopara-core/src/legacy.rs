//! One-time import of the Python app's data (plan step 2.7).
//!
//! The Python app keeps `autopara.db` and `schedules/` in the same folder the Rust app uses. On the
//! first start with an empty database, everything is copied across with its ids unchanged (so the
//! chosen course and group, and every verdict, keep pointing at the same rows). The old database is
//! opened read-only and the old document is copied, never moved: nothing of the Python app's is
//! modified, so going back to it stays possible.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use rusqlite::{Connection, OpenFlags, params};

use crate::db::{DbError, Storage};
use crate::import::keep_copy;

pub const LEGACY_DATABASE_FILE: &str = "autopara.db";
pub const LEGACY_SCHEDULES_DIR: &str = "schedules";
/// Set once the import has run (or found nothing worth importing), so it never runs twice.
pub const MARKER: &str = "legacy_imported_at";

#[derive(Debug, thiserror::Error)]
pub enum LegacyError {
    #[error(transparent)]
    Database(#[from] DbError),
    #[error("could not copy the old timetable: {0}")]
    Copy(#[from] std::io::Error),
}

/// What was brought across.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LegacyReport {
    pub courses: usize,
    pub groups: usize,
    pub lessons: usize,
    pub occurrences: usize,
    pub document: Option<PathBuf>,
}

/// Import the Python app's data from `data_root` into `storage`, once. `Ok(None)` when there was
/// nothing to do: already imported, nothing to import, or the new database already has a timetable.
pub fn import_legacy(
    storage: &mut Storage,
    data_root: &Path,
    now: NaiveDateTime,
) -> Result<Option<LegacyReport>, LegacyError> {
    if storage
        .get_setting(MARKER)?
        .is_some_and(|value| !value.is_empty())
    {
        return Ok(None);
    }
    let stamp = now.format("%Y-%m-%dT%H:%M:%S").to_string();
    let legacy_path = data_root.join(LEGACY_DATABASE_FILE);
    if !legacy_path.is_file() || !storage.courses()?.is_empty() {
        storage.set_setting(MARKER, &stamp)?;
        return Ok(None);
    }

    let legacy = Connection::open_with_flags(
        &legacy_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut report = LegacyReport::default();
    let tx = storage.connection_mut().transaction()?;

    report.courses = copy_rows(&legacy, &tx, "courses", &["id", "ordinal", "name"])?;
    report.groups = copy_rows(
        &legacy,
        &tx,
        "groups",
        &["id", "course_id", "name", "specialty", "col_lo", "col_hi"],
    )?;
    let mut lesson_columns = vec![
        "id",
        "course_id",
        "day_index",
        "pair",
        "start_time",
        "end_time",
        "subject",
        "teacher",
        "url",
        "provider",
        "needs_link",
        "is_manual",
        "source_key",
    ];
    // Databases from before dated lessons existed have no on_date / repeat_until.
    let existing = columns(&legacy, "lessons")?;
    lesson_columns.extend(
        ["on_date", "repeat_until"]
            .into_iter()
            .filter(|c| existing.iter().any(|e| e == c)),
    );
    report.lessons = copy_rows(&legacy, &tx, "lessons", &lesson_columns)?;
    copy_rows(&legacy, &tx, "lesson_groups", &["lesson_id", "group_id"])?;
    report.occurrences = copy_rows(
        &legacy,
        &tx,
        "occurrences",
        &["id", "lesson_id", "occur_date", "status", "fired_at"],
    )?;
    if !columns(&legacy, "lesson_skips")?.is_empty() {
        copy_rows(&legacy, &tx, "lesson_skips", &["lesson_id", "skip_date"])?;
    }

    // Settings: the user's choices come across; the document path is replaced below.
    let settings: Vec<(String, Option<String>)> = legacy
        .prepare("SELECT key, value FROM settings")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (key, value) in settings {
        if key == "schedule_copy_path" {
            continue;
        }
        tx.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
    }
    tx.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![MARKER, stamp],
    )?;
    tx.commit()?;

    if let Some(document) = legacy_document(&legacy, data_root)? {
        let copy = keep_copy(data_root, &document)?;
        storage.set_setting("schedule_copy_path", &copy.to_string_lossy())?;
        report.document = Some(copy);
    }
    Ok(Some(report))
}

/// The table's column names (empty when the table does not exist).
fn columns(conn: &Connection, table: &str) -> Result<Vec<String>, DbError> {
    conn.prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |row| row.get(1))?
        .collect()
}

/// Copy `table`'s `columns` row for row, values untouched. Returns the number of rows.
fn copy_rows(
    from: &Connection,
    to: &Connection,
    table: &str,
    columns: &[&str],
) -> Result<usize, DbError> {
    if self::columns(from, table)?.is_empty() {
        return Ok(0);
    }
    let list = columns.join(", ");
    let placeholders = (1..=columns.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut read = from.prepare(&format!("SELECT {list} FROM {table}"))?;
    let mut write = to.prepare(&format!(
        "INSERT INTO {table}({list}) VALUES ({placeholders})"
    ))?;
    let mut rows = read.query([])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        let values: Vec<rusqlite::types::Value> = (0..columns.len())
            .map(|i| row.get(i))
            .collect::<Result<_, _>>()?;
        write.execute(rusqlite::params_from_iter(values))?;
        count += 1;
    }
    Ok(count)
}

/// The Python app's archived timetable: the path its settings name, else the file in `schedules/`.
fn legacy_document(legacy: &Connection, data_root: &Path) -> Result<Option<PathBuf>, LegacyError> {
    let named: Option<String> = legacy
        .query_row(
            "SELECT value FROM settings WHERE key = 'schedule_copy_path'",
            [],
            |row| row.get(0),
        )
        .ok()
        .flatten();
    if let Some(path) = named.map(PathBuf::from).filter(|path| path.is_file()) {
        return Ok(Some(path));
    }
    let directory = data_root.join(LEGACY_SCHEDULES_DIR);
    if !directory.is_dir() {
        return Ok(None);
    }
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("docx"))
        {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::parse_datetime;
    use crate::import::TIMETABLES_DIR;
    use crate::model::{OccurrenceStatus, Recurrence, Weekday};

    /// The Python app's schema from before dated lessons: no on_date, no lesson_skips.
    const OLD_SCHEMA: &str = r"
        CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
        CREATE TABLE courses (id INTEGER PRIMARY KEY AUTOINCREMENT, ordinal INTEGER NOT NULL UNIQUE, name TEXT NOT NULL);
        CREATE TABLE groups (id INTEGER PRIMARY KEY AUTOINCREMENT, course_id INTEGER NOT NULL, name TEXT NOT NULL,
            specialty TEXT NOT NULL DEFAULT '', col_lo INTEGER NOT NULL DEFAULT 0, col_hi INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE lessons (id INTEGER PRIMARY KEY AUTOINCREMENT, course_id INTEGER NOT NULL, day_index INTEGER NOT NULL,
            pair INTEGER NOT NULL, start_time TEXT NOT NULL, end_time TEXT NOT NULL, subject TEXT NOT NULL,
            teacher TEXT NOT NULL DEFAULT '', url TEXT, provider TEXT NOT NULL DEFAULT 'unknown',
            needs_link INTEGER NOT NULL DEFAULT 0, is_manual INTEGER NOT NULL DEFAULT 0, source_key TEXT NOT NULL DEFAULT '');
        CREATE TABLE lesson_groups (lesson_id INTEGER NOT NULL, group_id INTEGER NOT NULL, PRIMARY KEY (lesson_id, group_id));
        CREATE TABLE occurrences (id INTEGER PRIMARY KEY AUTOINCREMENT, lesson_id INTEGER NOT NULL, occur_date TEXT NOT NULL,
            status TEXT NOT NULL, fired_at TEXT NOT NULL, UNIQUE(lesson_id, occur_date));
        INSERT INTO courses VALUES (7, 2, 'II курс');
        INSERT INTO groups VALUES (40, 7, '21 група', 'Дошкільна освіта', 3, 3);
        INSERT INTO lessons VALUES (300, 7, 1, 2, '09:30', '10:50', 'Психологія', '(Шевчук Т.О.)',
            'https://meet.google.com/a', 'google_meet', 0, 0, '2|1|2|психологія');
        INSERT INTO lesson_groups VALUES (300, 40);
        INSERT INTO occurrences VALUES (1, 300, '2026-10-06', 'opened', '2026-10-06T09:29:00');
        INSERT INTO settings VALUES ('selected_course_id', '7'), ('selected_group_id', '40'),
            ('theme', 'dark'), ('lead_minutes', '3');
    ";

    fn now() -> NaiveDateTime {
        parse_datetime("2026-10-27T10:00:00").unwrap()
    }

    fn legacy_folder() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        Connection::open(root.path().join(LEGACY_DATABASE_FILE))
            .unwrap()
            .execute_batch(OLD_SCHEMA)
            .unwrap();
        fs::create_dir(root.path().join(LEGACY_SCHEDULES_DIR)).unwrap();
        fs::write(
            root.path().join(LEGACY_SCHEDULES_DIR).join("розклад.docx"),
            b"docx bytes",
        )
        .unwrap();
        root
    }

    fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut files = Vec::new();
        for path in [
            root.join(LEGACY_DATABASE_FILE),
            root.join(LEGACY_SCHEDULES_DIR).join("розклад.docx"),
        ] {
            files.push((path.clone(), fs::read(&path).unwrap()));
        }
        files
    }

    #[test]
    fn everything_comes_across_with_its_ids_and_nothing_old_changes() {
        let root = legacy_folder();
        let before = snapshot(root.path());
        let mut storage = Storage::open(&root.path().join(crate::import::DATABASE_FILE)).unwrap();

        let report = import_legacy(&mut storage, root.path(), now())
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                report.courses,
                report.groups,
                report.lessons,
                report.occurrences
            ),
            (1, 1, 1, 1)
        );

        let settings = storage.settings().unwrap();
        assert_eq!(
            (settings.selected_group_id, settings.lead_minutes),
            (Some(40), 3)
        );
        let lessons = storage.lessons_for_group(40).unwrap();
        assert_eq!(lessons.len(), 1);
        assert_eq!((lessons[0].id, lessons[0].weekday), (300, Weekday::Tuesday));
        assert_eq!(lessons[0].recurrence, Recurrence::Weekly);
        let day = "2026-10-06".parse().unwrap();
        assert_eq!(
            storage.occurrence(300, day).unwrap().unwrap().status,
            OccurrenceStatus::Opened
        );

        let copy = report.document.unwrap();
        assert!(copy.starts_with(root.path().join(TIMETABLES_DIR)));
        assert_eq!(settings.schedule_copy_path, copy.to_string_lossy());
        assert_eq!(
            snapshot(root.path()),
            before,
            "the Python app's files are untouched"
        );
    }

    #[test]
    fn it_runs_once() {
        let root = legacy_folder();
        let mut storage = Storage::open_in_memory().unwrap();
        assert!(
            import_legacy(&mut storage, root.path(), now())
                .unwrap()
                .is_some()
        );
        assert!(
            import_legacy(&mut storage, root.path(), now())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn nothing_to_import_is_remembered_too() {
        let root = tempfile::tempdir().unwrap();
        let mut storage = Storage::open_in_memory().unwrap();
        assert!(
            import_legacy(&mut storage, root.path(), now())
                .unwrap()
                .is_none()
        );
        assert!(storage.get_setting(MARKER).unwrap().is_some());
    }
}
