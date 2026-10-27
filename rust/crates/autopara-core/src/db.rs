//! SQLite storage, the single source of truth (PORT_SPEC §B).
//!
//! Same logical schema as the Python app. The open log (`occurrences`) carries
//! `UNIQUE(lesson_id, occur_date)`, and a class is claimed by inserting its row *before* anything
//! is opened: "open each class exactly once" is a database constraint, not in-memory state.
//!
//! Nothing here reads the clock: every timestamp is passed in by the caller.

use std::collections::BTreeSet;
use std::path::Path;

use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Row, params};

use crate::model::{
    CatchupMode, Course, Group, Lesson, Occurrence, OccurrenceStatus, Provider, Recurrence,
    Settings, Theme, UpdateMode, ViewMode, Weekday,
};
use crate::parser::ParsedCourse;
use crate::time::TimeOfDay;

pub use rusqlite::Error as DbError;

/// Versioned migrations; `PRAGMA user_version` records how many have run.
const MIGRATIONS: &[&str] = &[
    // 1: the Python app's schema as of 1.10.0, with every later column and table folded in.
    r"
    CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
    CREATE TABLE courses (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        ordinal INTEGER NOT NULL UNIQUE,
        name TEXT NOT NULL
    );
    CREATE TABLE groups (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        specialty TEXT NOT NULL DEFAULT '',
        col_lo INTEGER NOT NULL DEFAULT 0,
        col_hi INTEGER NOT NULL DEFAULT 0,
        UNIQUE(course_id, name)
    );
    CREATE TABLE lessons (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
        day_index INTEGER NOT NULL,
        pair INTEGER NOT NULL,
        start_time TEXT NOT NULL,
        end_time TEXT NOT NULL,
        subject TEXT NOT NULL,
        teacher TEXT NOT NULL DEFAULT '',
        url TEXT,
        provider TEXT NOT NULL DEFAULT 'unknown',
        needs_link INTEGER NOT NULL DEFAULT 0,
        is_manual INTEGER NOT NULL DEFAULT 0,
        source_key TEXT NOT NULL DEFAULT '',
        on_date TEXT,
        repeat_until TEXT
    );
    CREATE TABLE lesson_groups (
        lesson_id INTEGER NOT NULL REFERENCES lessons(id) ON DELETE CASCADE,
        group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
        PRIMARY KEY (lesson_id, group_id)
    );
    CREATE TABLE occurrences (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        lesson_id INTEGER NOT NULL REFERENCES lessons(id) ON DELETE CASCADE,
        occur_date TEXT NOT NULL,
        status TEXT NOT NULL,
        fired_at TEXT NOT NULL,
        UNIQUE(lesson_id, occur_date)
    );
    CREATE TABLE lesson_skips (
        lesson_id INTEGER NOT NULL REFERENCES lessons(id) ON DELETE CASCADE,
        skip_date TEXT NOT NULL,
        PRIMARY KEY (lesson_id, skip_date)
    );
    CREATE INDEX idx_lessons_day ON lessons(day_index);
    CREATE INDEX idx_lessons_on_date ON lessons(on_date);
    CREATE INDEX idx_occurrences_date ON occurrences(occur_date);
    ",
];

/// Defaults written on first open (`INSERT OR IGNORE`), as the Python app seeds them.
const DEFAULTS: &[(&str, &str)] = &[
    ("lead_minutes", "1"),
    ("class_duration_minutes", "80"),
    ("autostart_enabled", "1"),
    ("catchup_mode", "notify"),
    ("last_import_path", ""),
    ("notifications_enabled", "1"),
    ("notify_minutes", "10"),
    ("schedule_active_from", ""),
    ("schedule_copy_path", ""),
    ("theme", "system"),
    ("view_mode", "week"),
    ("update_mode", "ask"),
    ("skipped_update_version", ""),
    ("last_update_check", ""),
];

const DATE: &str = "%Y-%m-%d";
const DATETIME: &str = "%Y-%m-%dT%H:%M:%S";

/// Parse an ISO date-time as the Python app writes it (seconds, or with fractions, or minutes).
pub fn parse_datetime(text: &str) -> Option<NaiveDateTime> {
    [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S%.f",
    ]
    .iter()
    .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format(DATETIME).to_string()
}

/// A stored date that does not parse reads as "no date", as in the Python app.
fn parse_date(text: Option<String>) -> Option<NaiveDate> {
    text.and_then(|text| NaiveDate::parse_from_str(&text, DATE).ok())
}

fn format_date(value: NaiveDate) -> String {
    value.format(DATE).to_string()
}

pub struct Storage {
    conn: Connection,
}

impl Storage {
    /// Open (creating if needed) the database at `path`, in WAL mode, migrated and seeded.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::prepare(conn)
    }

    /// A private in-memory database, for tests.
    pub fn open_in_memory() -> Result<Self, DbError> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(conn: Connection) -> Result<Self, DbError> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let mut storage = Self { conn };
        storage.migrate()?;
        storage.seed_defaults()?;
        Ok(storage)
    }

    fn migrate(&mut self) -> Result<(), DbError> {
        let done: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        for (version, sql) in (1_i64..)
            .zip(MIGRATIONS)
            .skip_while(|(version, _)| *version <= done)
        {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", version)?;
            tx.commit()?;
        }
        Ok(())
    }

    fn seed_defaults(&self) -> Result<(), DbError> {
        for (key, value) in DEFAULTS {
            self.conn.execute(
                "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, ?2)",
                params![key, value],
            )?;
        }
        Ok(())
    }

    /// The raw connection, for the legacy importer's row-for-row copy.
    pub(crate) fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    // ------------------------------------------------------------------ settings

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, DbError> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map(Option::flatten)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// The typed settings, read the way the Python app reads them (PORT_SPEC §D).
    pub fn settings(&self) -> Result<Settings, DbError> {
        // One query for the whole table: the scheduler reads the settings on every tick.
        let all: std::collections::HashMap<String, Option<String>> = self
            .conn
            .prepare_cached("SELECT key, value FROM settings")?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let raw = |key: &str| all.get(key).cloned().flatten();
        let get = |key: &str| -> Result<String, DbError> { Ok(raw(key).unwrap_or_default()) };
        let number = |key: &str, fallback: u32| -> Result<u32, DbError> {
            Ok(get(key)?.trim().parse().unwrap_or(fallback))
        };
        let flag =
            |key: &str| -> Result<bool, DbError> { Ok(raw(key).is_none_or(|value| value == "1")) };
        let id = |key: &str| -> Result<Option<i64>, DbError> { Ok(get(key)?.parse().ok()) };
        Ok(Settings {
            selected_course_id: id("selected_course_id")?,
            selected_group_id: id("selected_group_id")?,
            lead_minutes: number("lead_minutes", 1)?,
            class_duration_minutes: number("class_duration_minutes", 80)?,
            autostart_enabled: flag("autostart_enabled")?,
            catchup_mode: CatchupMode::parse(&get("catchup_mode")?).unwrap_or(CatchupMode::Notify),
            notifications_enabled: flag("notifications_enabled")?,
            notify_minutes: number("notify_minutes", 10)?,
            theme: Theme::parse(&get("theme")?).unwrap_or(Theme::System),
            view_mode: ViewMode::parse(&get("view_mode")?).unwrap_or(ViewMode::Week),
            update_mode: UpdateMode::parse(&get("update_mode")?).unwrap_or(UpdateMode::Ask),
            skipped_update_version: get("skipped_update_version")?,
            last_update_check: parse_datetime(&get("last_update_check")?),
            schedule_active_from: parse_datetime(&get("schedule_active_from")?),
            schedule_copy_path: get("schedule_copy_path")?,
            last_import_path: get("last_import_path")?,
        })
    }

    // ------------------------------------------------------------- courses, groups

    pub fn courses(&self) -> Result<Vec<Course>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, ordinal, name FROM courses ORDER BY ordinal")?;
        let rows = stmt.query_map([], |row| {
            Ok(Course {
                id: row.get(0)?,
                ordinal: row.get(1)?,
                name: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn course(&self, id: i64) -> Result<Option<Course>, DbError> {
        self.conn
            .query_row(
                "SELECT id, ordinal, name FROM courses WHERE id = ?1",
                [id],
                |row| {
                    Ok(Course {
                        id: row.get(0)?,
                        ordinal: row.get(1)?,
                        name: row.get(2)?,
                    })
                },
            )
            .optional()
    }

    fn group_from(row: &Row<'_>) -> rusqlite::Result<Group> {
        Ok(Group {
            id: row.get(0)?,
            course_id: row.get(1)?,
            name: row.get(2)?,
            specialty: row.get(3)?,
            col_lo: row.get(4)?,
            col_hi: row.get(5)?,
        })
    }

    pub fn groups(&self, course_id: i64) -> Result<Vec<Group>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, course_id, name, specialty, col_lo, col_hi FROM groups \
             WHERE course_id = ?1 ORDER BY col_lo",
        )?;
        let rows = stmt.query_map([course_id], Self::group_from)?;
        rows.collect()
    }

    pub fn group(&self, id: i64) -> Result<Option<Group>, DbError> {
        self.conn
            .query_row(
                "SELECT id, course_id, name, specialty, col_lo, col_hi FROM groups WHERE id = ?1",
                [id],
                Self::group_from,
            )
            .optional()
    }

    // -------------------------------------------------------------------- import

    /// Replace the imported timetable while keeping manual lessons and settings (PORT_SPEC §B.2).
    ///
    /// Courses are matched by ordinal and groups by name, so the chosen course and group keep
    /// their ids; imported lessons are matched by `source_key`. Afterwards every occurrence is
    /// forgotten and `schedule_active_from` is stamped with `now`, so the part of today that is
    /// already over is never marked missed.
    pub fn import_courses(
        &mut self,
        parsed: &[ParsedCourse],
        now: NaiveDateTime,
    ) -> Result<(), DbError> {
        let tx = self.conn.transaction()?;
        for course in parsed {
            tx.execute(
                "INSERT INTO courses(ordinal, name) VALUES (?1, ?2) \
                 ON CONFLICT(ordinal) DO UPDATE SET name = excluded.name",
                params![course.ordinal, course.name],
            )?;
            let course_id: i64 = tx.query_row(
                "SELECT id FROM courses WHERE ordinal = ?1",
                [course.ordinal],
                |row| row.get(0),
            )?;

            let mut group_ids = std::collections::HashMap::new();
            for group in &course.groups {
                tx.execute(
                    "INSERT INTO groups(course_id, name, specialty, col_lo, col_hi) \
                     VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(course_id, name) DO UPDATE SET \
                     specialty = excluded.specialty, col_lo = excluded.col_lo, col_hi = excluded.col_hi",
                    params![course_id, group.name, group.specialty, group.col_lo, group.col_hi],
                )?;
                let id: i64 = tx.query_row(
                    "SELECT id FROM groups WHERE course_id = ?1 AND name = ?2",
                    params![course_id, group.name],
                    |row| row.get(0),
                )?;
                group_ids.insert(group.name.as_str(), id);
            }

            let incoming: BTreeSet<String> = course
                .lessons
                .iter()
                .map(|lesson| lesson.source_key(course.ordinal))
                .collect();
            let existing: Vec<(i64, String)> = tx
                .prepare(
                    "SELECT id, source_key FROM lessons WHERE course_id = ?1 AND is_manual = 0",
                )?
                .query_map([course_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?;
            for (id, key) in existing {
                if !incoming.contains(&key) {
                    tx.execute("DELETE FROM lessons WHERE id = ?1", [id])?;
                }
            }

            for lesson in &course.lessons {
                let key = lesson.source_key(course.ordinal);
                let found: Option<i64> = tx
                    .query_row(
                        "SELECT id FROM lessons WHERE course_id = ?1 AND source_key = ?2 AND is_manual = 0",
                        params![course_id, key],
                        |row| row.get(0),
                    )
                    .optional()?;
                let values = params![
                    course_id,
                    lesson.weekday.index(),
                    lesson.pair,
                    lesson.start.to_string(),
                    lesson.end.to_string(),
                    lesson.subject,
                    lesson.teacher,
                    lesson.url,
                    lesson.provider.as_str(),
                    lesson.needs_link(),
                    key,
                ];
                let lesson_id = if let Some(id) = found {
                    tx.execute(
                        "UPDATE lessons SET course_id=?1, day_index=?2, pair=?3, start_time=?4, \
                         end_time=?5, subject=?6, teacher=?7, url=?8, provider=?9, needs_link=?10, \
                         source_key=?11 WHERE id=?12",
                        rusqlite::params_from_iter(
                            values.iter().copied().chain([&id as &dyn rusqlite::ToSql]),
                        ),
                    )?;
                    id
                } else {
                    tx.execute(
                        "INSERT INTO lessons(course_id, day_index, pair, start_time, end_time, \
                         subject, teacher, url, provider, needs_link, source_key) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                        values,
                    )?;
                    tx.last_insert_rowid()
                };
                tx.execute(
                    "DELETE FROM lesson_groups WHERE lesson_id = ?1",
                    [lesson_id],
                )?;
                for name in &lesson.group_names {
                    if let Some(group_id) = group_ids.get(name.as_str()) {
                        tx.execute(
                            "INSERT OR IGNORE INTO lesson_groups(lesson_id, group_id) VALUES (?1, ?2)",
                            params![lesson_id, group_id],
                        )?;
                    }
                }
            }
        }
        // A new document is a new week: yesterday's verdicts do not belong on today's grid.
        tx.execute("DELETE FROM occurrences", [])?;
        tx.execute(
            "INSERT INTO settings(key, value) VALUES ('schedule_active_from', ?1) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [format_datetime(now)],
        )?;
        tx.commit()
    }

    // ------------------------------------------------------------------- lessons

    const LESSON_COLUMNS: &'static str = "id, course_id, day_index, pair, start_time, end_time, \
        subject, teacher, url, provider, needs_link, is_manual, source_key, on_date, repeat_until";

    fn hydrate(&self, row: &Row<'_>) -> rusqlite::Result<Lesson> {
        let id: i64 = row.get(0)?;
        let bad = |column: usize, what: &str| {
            rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                format!("unreadable {what}").into(),
            )
        };
        let time = |column: usize| -> rusqlite::Result<TimeOfDay> {
            row.get::<_, String>(column)?
                .parse()
                .map_err(|_| bad(column, "time"))
        };
        let group_names = self
            .conn
            .prepare_cached(
                "SELECT g.name FROM lesson_groups lg JOIN groups g ON g.id = lg.group_id \
                 WHERE lg.lesson_id = ?1 ORDER BY g.col_lo",
            )?
            .query_map([id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let skip_dates = self
            .conn
            .prepare_cached("SELECT skip_date FROM lesson_skips WHERE lesson_id = ?1")?
            .query_map([id], |r| r.get::<_, String>(0))?
            .filter_map(|text| parse_date(text.ok()))
            .collect();
        Ok(Lesson {
            id,
            course_id: row.get(1)?,
            weekday: Weekday::from_index(row.get(2)?).ok_or_else(|| bad(2, "weekday"))?,
            pair: row.get(3)?,
            start: time(4)?,
            end: time(5)?,
            subject: row.get(6)?,
            teacher: row.get(7)?,
            url: row.get(8)?,
            provider: Provider::parse(&row.get::<_, String>(9)?).unwrap_or(Provider::Unknown),
            needs_link: row.get(10)?,
            is_manual: row.get(11)?,
            source_key: row.get(12)?,
            group_names,
            recurrence: Recurrence::from_columns(
                parse_date(row.get(13)?),
                parse_date(row.get(14)?),
            ),
            skip_dates,
        })
    }

    /// Every lesson of a group, by weekday then pair.
    pub fn lessons_for_group(&self, group_id: i64) -> Result<Vec<Lesson>, DbError> {
        let sql = format!(
            "SELECT {} FROM lessons l JOIN lesson_groups lg ON lg.lesson_id = l.id \
             WHERE lg.group_id = ?1 ORDER BY l.day_index, l.pair",
            Self::LESSON_COLUMNS
                .replace(", ", ", l.")
                .replacen("id", "l.id", 1)
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([group_id], |row| self.hydrate(row))?;
        rows.collect()
    }

    pub fn lesson(&self, id: i64) -> Result<Option<Lesson>, DbError> {
        let sql = format!("SELECT {} FROM lessons WHERE id = ?1", Self::LESSON_COLUMNS);
        self.conn
            .query_row(&sql, [id], |row| self.hydrate(row))
            .optional()
    }

    /// Add a manual lesson; its weekday follows its first date when it has one.
    pub fn add_lesson(&self, lesson: &Lesson, group_ids: &[i64]) -> Result<i64, DbError> {
        let (on_date, repeat_until) = lesson.recurrence.columns();
        let weekday = on_date.map_or(lesson.weekday, Weekday::of);
        self.conn.execute(
            "INSERT INTO lessons(course_id, day_index, pair, start_time, end_time, subject, \
             teacher, url, provider, needs_link, is_manual, source_key, on_date, repeat_until) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, '', ?11, ?12)",
            params![
                lesson.course_id,
                weekday.index(),
                lesson.pair,
                lesson.start.to_string(),
                lesson.end.to_string(),
                lesson.subject,
                lesson.teacher,
                lesson.url,
                lesson.provider.as_str(),
                lesson.url.is_none(),
                on_date.map(format_date),
                repeat_until.map(format_date),
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        for group_id in group_ids {
            self.conn.execute(
                "INSERT OR IGNORE INTO lesson_groups(lesson_id, group_id) VALUES (?1, ?2)",
                params![id, group_id],
            )?;
        }
        Ok(id)
    }

    /// Save an edited lesson; its groups change only when `group_ids` is given.
    pub fn update_lesson(&self, lesson: &Lesson, group_ids: Option<&[i64]>) -> Result<(), DbError> {
        let (on_date, repeat_until) = lesson.recurrence.columns();
        let weekday = on_date.map_or(lesson.weekday, Weekday::of);
        self.conn.execute(
            "UPDATE lessons SET day_index=?1, pair=?2, start_time=?3, end_time=?4, subject=?5, \
             teacher=?6, url=?7, provider=?8, needs_link=?9, on_date=?10, repeat_until=?11 \
             WHERE id=?12",
            params![
                weekday.index(),
                lesson.pair,
                lesson.start.to_string(),
                lesson.end.to_string(),
                lesson.subject,
                lesson.teacher,
                lesson.url,
                lesson.provider.as_str(),
                lesson.url.is_none(),
                on_date.map(format_date),
                repeat_until.map(format_date),
                lesson.id,
            ],
        )?;
        if let Some(group_ids) = group_ids {
            self.conn.execute(
                "DELETE FROM lesson_groups WHERE lesson_id = ?1",
                [lesson.id],
            )?;
            for group_id in group_ids {
                self.conn.execute(
                    "INSERT OR IGNORE INTO lesson_groups(lesson_id, group_id) VALUES (?1, ?2)",
                    params![lesson.id, group_id],
                )?;
            }
        }
        Ok(())
    }

    /// Move a whole lesson (drag and drop). A weekly row moves to `weekday`; a dated lesson moves
    /// to `on_date`, and a series carries its end date by the same number of days.
    pub fn move_lesson(
        &self,
        id: i64,
        weekday: Weekday,
        pair: u32,
        start: TimeOfDay,
        end: TimeOfDay,
        on_date: Option<NaiveDate>,
    ) -> Result<(), DbError> {
        let Some(current) = self.lesson(id)? else {
            return Ok(());
        };
        match (current.recurrence.first_date(), on_date) {
            (Some(first), Some(target)) => {
                let shift = target - first;
                let until = current.recurrence.columns().1.map(|until| until + shift);
                self.conn.execute(
                    "UPDATE lessons SET day_index=?1, pair=?2, start_time=?3, end_time=?4, \
                     on_date=?5, repeat_until=?6 WHERE id=?7",
                    params![
                        Weekday::of(target).index(),
                        pair,
                        start.to_string(),
                        end.to_string(),
                        format_date(target),
                        until.map(format_date),
                        id
                    ],
                )?;
            }
            _ => {
                self.conn.execute(
                    "UPDATE lessons SET day_index=?1, pair=?2, start_time=?3, end_time=?4 WHERE id=?5",
                    params![weekday.index(), pair, start.to_string(), end.to_string(), id],
                )?;
            }
        }
        Ok(())
    }

    /// Move one date of a repeating lesson: the series skips `from`, and a one-off copy takes
    /// place on `to`. Returns the copy's id.
    pub fn move_occurrence(
        &self,
        id: i64,
        from: NaiveDate,
        to: NaiveDate,
        pair: u32,
        start: TimeOfDay,
        end: TimeOfDay,
    ) -> Result<Option<i64>, DbError> {
        let Some(original) = self.lesson(id)? else {
            return Ok(None);
        };
        let group_ids: Vec<i64> = self
            .conn
            .prepare("SELECT group_id FROM lesson_groups WHERE lesson_id = ?1")?
            .query_map([id], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        self.conn.execute(
            "INSERT OR IGNORE INTO lesson_skips(lesson_id, skip_date) VALUES (?1, ?2)",
            params![id, format_date(from)],
        )?;
        let copy = Lesson {
            id: 0,
            weekday: Weekday::of(to),
            pair,
            start,
            end,
            is_manual: true,
            source_key: String::new(),
            recurrence: Recurrence::Once(to),
            skip_dates: BTreeSet::new(),
            group_names: Vec::new(),
            ..original
        };
        self.add_lesson(&copy, &group_ids).map(Some)
    }

    pub fn delete_lesson(&self, id: i64) -> Result<(), DbError> {
        self.conn
            .execute("DELETE FROM lessons WHERE id = ?1", [id])?;
        Ok(())
    }

    // --------------------------------------------------------------- the open log

    /// Reserve `(lesson, date)`. `false` means it was already claimed: do **not** open anything.
    /// The caller must claim *before* opening — this is the fire-once guarantee.
    pub fn claim_occurrence(
        &self,
        lesson_id: i64,
        date: NaiveDate,
        status: OccurrenceStatus,
        now: NaiveDateTime,
    ) -> Result<bool, DbError> {
        let result = self.conn.execute(
            "INSERT INTO occurrences(lesson_id, occur_date, status, fired_at) VALUES (?1, ?2, ?3, ?4)",
            params![lesson_id, format_date(date), status.as_str(), format_datetime(now)],
        );
        match result {
            Ok(_) => Ok(true),
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == ErrorCode::ConstraintViolation =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    /// Record the user's own verdict, overwriting any row (it bypasses the claim on purpose).
    pub fn mark_occurrence(
        &self,
        lesson_id: i64,
        date: NaiveDate,
        status: OccurrenceStatus,
        now: NaiveDateTime,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO occurrences(lesson_id, occur_date, status, fired_at) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(lesson_id, occur_date) DO UPDATE SET status = excluded.status, \
             fired_at = excluded.fired_at",
            params![lesson_id, format_date(date), status.as_str(), format_datetime(now)],
        )?;
        Ok(())
    }

    pub fn set_occurrence_status(
        &self,
        lesson_id: i64,
        date: NaiveDate,
        status: OccurrenceStatus,
        now: NaiveDateTime,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE occurrences SET status = ?1, fired_at = ?2 WHERE lesson_id = ?3 AND occur_date = ?4",
            params![status.as_str(), format_datetime(now), lesson_id, format_date(date)],
        )?;
        Ok(())
    }

    pub fn clear_occurrence(&self, lesson_id: i64, date: NaiveDate) -> Result<(), DbError> {
        self.conn.execute(
            "DELETE FROM occurrences WHERE lesson_id = ?1 AND occur_date = ?2",
            params![lesson_id, format_date(date)],
        )?;
        Ok(())
    }

    pub fn clear_occurrences(&self) -> Result<(), DbError> {
        self.conn.execute("DELETE FROM occurrences", [])?;
        Ok(())
    }

    fn occurrence_from(row: &Row<'_>) -> rusqlite::Result<Occurrence> {
        let bad = |column| {
            rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                "unreadable occurrence".into(),
            )
        };
        Ok(Occurrence {
            id: row.get(0)?,
            lesson_id: row.get(1)?,
            date: parse_date(row.get(2)?).ok_or_else(|| bad(2))?,
            status: OccurrenceStatus::parse(&row.get::<_, String>(3)?).ok_or_else(|| bad(3))?,
            fired_at: parse_datetime(&row.get::<_, String>(4)?).ok_or_else(|| bad(4))?,
        })
    }

    pub fn occurrence(
        &self,
        lesson_id: i64,
        date: NaiveDate,
    ) -> Result<Option<Occurrence>, DbError> {
        self.conn
            .query_row(
                "SELECT id, lesson_id, occur_date, status, fired_at FROM occurrences \
                 WHERE lesson_id = ?1 AND occur_date = ?2",
                params![lesson_id, format_date(date)],
                Self::occurrence_from,
            )
            .optional()
    }

    /// Every occurrence from `first` to `last` inclusive.
    pub fn occurrences_between(
        &self,
        first: NaiveDate,
        last: NaiveDate,
    ) -> Result<Vec<Occurrence>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, lesson_id, occur_date, status, fired_at FROM occurrences \
             WHERE occur_date BETWEEN ?1 AND ?2 ORDER BY occur_date, lesson_id",
        )?;
        let rows = stmt.query_map(
            params![format_date(first), format_date(last)],
            Self::occurrence_from,
        )?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file_default;

    fn sample() -> Vec<ParsedCourse> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/golden/sample_timetable.docx");
        parse_file_default(&path).unwrap()
    }

    fn at(text: &str) -> NaiveDateTime {
        parse_datetime(text).unwrap()
    }

    fn date(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    fn first_group(storage: &Storage) -> Group {
        let course = &storage.courses().unwrap()[0];
        storage.groups(course.id).unwrap().remove(0)
    }

    #[test]
    fn a_fresh_database_has_the_python_defaults() {
        let storage = Storage::open_in_memory().unwrap();
        assert_eq!(storage.settings().unwrap(), Settings::default());
    }

    #[test]
    fn migrations_run_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.db");
        Storage::open(&path)
            .unwrap()
            .set_setting("theme", "dark")
            .unwrap();
        let storage = Storage::open(&path).unwrap();
        assert_eq!(storage.settings().unwrap().theme, Theme::Dark);
    }

    #[test]
    fn import_stores_courses_groups_and_lessons() {
        let mut storage = Storage::open_in_memory().unwrap();
        storage
            .import_courses(&sample(), at("2026-10-05T07:00:00"))
            .unwrap();
        assert_eq!(storage.courses().unwrap().len(), 6);
        let group = first_group(&storage);
        assert_eq!(group.name, "А2-Бд26-11 група");
        let lessons = storage.lessons_for_group(group.id).unwrap();
        assert!(!lessons.is_empty());
        assert!(
            lessons
                .iter()
                .all(|lesson| lesson.recurrence == Recurrence::Weekly)
        );
        assert_eq!(
            storage.settings().unwrap().schedule_active_from,
            Some(at("2026-10-05T07:00:00"))
        );
    }

    #[test]
    fn the_open_log_claims_each_date_once() {
        let mut storage = Storage::open_in_memory().unwrap();
        storage
            .import_courses(&sample(), at("2026-10-05T07:00:00"))
            .unwrap();
        let lesson = storage
            .lessons_for_group(first_group(&storage).id)
            .unwrap()
            .remove(0);
        let now = at("2026-10-05T07:59:00");
        let day = date("2026-10-05");
        assert!(
            storage
                .claim_occurrence(lesson.id, day, OccurrenceStatus::Opened, now)
                .unwrap()
        );
        assert!(
            !storage
                .claim_occurrence(lesson.id, day, OccurrenceStatus::Opened, now)
                .unwrap()
        );
        assert!(
            storage
                .claim_occurrence(lesson.id, date("2026-10-12"), OccurrenceStatus::Opened, now)
                .unwrap()
        );
        storage
            .mark_occurrence(lesson.id, day, OccurrenceStatus::Skipped, now)
            .unwrap();
        assert_eq!(
            storage.occurrence(lesson.id, day).unwrap().unwrap().status,
            OccurrenceStatus::Skipped
        );
        storage.clear_occurrence(lesson.id, day).unwrap();
        assert!(storage.occurrence(lesson.id, day).unwrap().is_none());
    }

    #[test]
    fn re_import_keeps_ids_manual_lessons_and_clears_the_log() {
        let mut storage = Storage::open_in_memory().unwrap();
        storage
            .import_courses(&sample(), at("2026-10-05T07:00:00"))
            .unwrap();
        let group = first_group(&storage);
        let before = storage.lessons_for_group(group.id).unwrap();
        let mut manual = before[0].clone();
        manual.subject = "Своя пара".into();
        manual.recurrence = Recurrence::Once(date("2026-10-07"));
        let manual_id = storage.add_lesson(&manual, &[group.id]).unwrap();
        storage
            .claim_occurrence(
                before[0].id,
                date("2026-10-05"),
                OccurrenceStatus::Opened,
                at("2026-10-05T08:00:00"),
            )
            .unwrap();

        storage
            .import_courses(&sample(), at("2026-10-06T07:00:00"))
            .unwrap();
        assert_eq!(first_group(&storage).id, group.id);
        let after = storage.lessons_for_group(group.id).unwrap();
        let ids = |lessons: &[Lesson]| {
            lessons
                .iter()
                .filter(|l| !l.is_manual)
                .map(|l| l.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&before), ids(&after));
        let kept = storage.lesson(manual_id).unwrap().unwrap();
        assert!(kept.is_manual && kept.weekday == Weekday::Wednesday);
        assert!(
            storage
                .occurrences_between(date("2026-01-01"), date("2027-01-01"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn moving_one_date_of_a_series_leaves_the_rest() {
        let mut storage = Storage::open_in_memory().unwrap();
        storage
            .import_courses(&sample(), at("2026-10-05T07:00:00"))
            .unwrap();
        let group = first_group(&storage);
        let weekly = storage.lessons_for_group(group.id).unwrap().remove(0);
        assert_eq!(weekly.weekday, Weekday::Monday);
        let copy = storage
            .move_occurrence(
                weekly.id,
                date("2026-10-05"),
                date("2026-10-06"),
                3,
                "11:20".parse().unwrap(),
                "12:40".parse().unwrap(),
            )
            .unwrap()
            .unwrap();
        let weekly = storage.lesson(weekly.id).unwrap().unwrap();
        assert!(!weekly.occurs_on(date("2026-10-05")) && weekly.occurs_on(date("2026-10-12")));
        let copy = storage.lesson(copy).unwrap().unwrap();
        assert!(copy.occurs_on(date("2026-10-06")) && copy.is_manual);
        assert_eq!(copy.group_names, weekly.group_names);
    }

    #[test]
    fn moving_a_series_carries_its_end() {
        let mut storage = Storage::open_in_memory().unwrap();
        storage
            .import_courses(&sample(), at("2026-10-05T07:00:00"))
            .unwrap();
        let group = first_group(&storage);
        let mut series = storage.lessons_for_group(group.id).unwrap().remove(0);
        series.recurrence = Recurrence::WeeklyUntil {
            from: date("2026-10-05"),
            until: date("2026-10-26"),
        };
        let id = storage.add_lesson(&series, &[group.id]).unwrap();
        storage
            .move_lesson(
                id,
                Weekday::Monday,
                1,
                series.start,
                series.end,
                Some(date("2026-10-07")),
            )
            .unwrap();
        let moved = storage.lesson(id).unwrap().unwrap();
        assert_eq!(
            moved.recurrence,
            Recurrence::WeeklyUntil {
                from: date("2026-10-07"),
                until: date("2026-10-28")
            }
        );
        assert_eq!(moved.weekday, Weekday::Wednesday);
    }

    #[test]
    fn unknown_setting_values_read_as_defaults() {
        let storage = Storage::open_in_memory().unwrap();
        storage.set_setting("theme", "purple").unwrap();
        storage.set_setting("lead_minutes", "soon").unwrap();
        storage.set_setting("catchup_mode", "").unwrap();
        let settings = storage.settings().unwrap();
        assert_eq!((settings.theme, settings.lead_minutes), (Theme::System, 1));
        assert_eq!(settings.catchup_mode, CatchupMode::Notify);
    }
}
