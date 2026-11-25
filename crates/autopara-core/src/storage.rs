use chrono::{Datelike, Duration, Local, NaiveDate};
use rusqlite::{Connection, ErrorCode, OptionalExtension, params};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::models::{
    CATCHUP_NOTIFY, Course, GCAL_KEY_PREFIX, Group, LANG_EN, LANG_UK, Lesson, Occurrence, PERSONAL_COURSE_ORDINAL,
    SESSION_KEY_PREFIX, Settings, THEME_SYSTEM, UPDATE_ASK, UPDATE_MODES, VIEW_MODES, VIEW_WEEK,
};
use crate::schedule_parser::ParsedCourse;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlite error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT
);

CREATE TABLE IF NOT EXISTS courses (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    ordinal INTEGER NOT NULL UNIQUE,
    name    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS groups (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    name      TEXT NOT NULL,
    specialty TEXT NOT NULL DEFAULT '',
    col_lo    INTEGER NOT NULL DEFAULT 0,
    col_hi    INTEGER NOT NULL DEFAULT 0,
    UNIQUE(course_id, name)
);

CREATE TABLE IF NOT EXISTS lessons (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    course_id    INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    day_index    INTEGER NOT NULL,
    pair         INTEGER NOT NULL,
    start_time   TEXT NOT NULL,
    end_time     TEXT NOT NULL,
    subject      TEXT NOT NULL,
    teacher      TEXT NOT NULL DEFAULT '',
    url          TEXT,
    provider     TEXT NOT NULL DEFAULT 'unknown',
    needs_link   INTEGER NOT NULL DEFAULT 0,
    is_manual    INTEGER NOT NULL DEFAULT 0,
    source_key   TEXT NOT NULL DEFAULT '',
    on_date      TEXT,
    repeat_until TEXT
);

CREATE TABLE IF NOT EXISTS lesson_groups (
    lesson_id INTEGER NOT NULL REFERENCES lessons(id) ON DELETE CASCADE,
    group_id  INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    PRIMARY KEY (lesson_id, group_id)
);

CREATE TABLE IF NOT EXISTS occurrences (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    lesson_id  INTEGER NOT NULL REFERENCES lessons(id) ON DELETE CASCADE,
    occur_date TEXT NOT NULL,
    status     TEXT NOT NULL,
    fired_at   TEXT NOT NULL,
    UNIQUE(lesson_id, occur_date)
);

CREATE TABLE IF NOT EXISTS lesson_skips (
    lesson_id INTEGER NOT NULL REFERENCES lessons(id) ON DELETE CASCADE,
    skip_date TEXT NOT NULL,
    PRIMARY KEY (lesson_id, skip_date)
);

CREATE INDEX IF NOT EXISTS idx_lessons_day ON lessons(day_index);
CREATE INDEX IF NOT EXISTS idx_occurrences_date ON occurrences(occur_date);
"#;

const DEFAULTS: [(&str, &str); 14] = [
    ("lead_minutes", "1"),
    ("class_duration_minutes", "80"),
    ("autostart_enabled", "1"),
    ("catchup_mode", CATCHUP_NOTIFY),
    ("last_import_path", ""),
    ("notifications_enabled", "1"),
    ("notify_minutes", "10"),
    ("schedule_active_from", ""),
    ("schedule_copy_path", ""),
    ("theme", THEME_SYSTEM),
    ("view_mode", VIEW_WEEK),
    ("update_mode", UPDATE_ASK),
    ("skipped_update_version", ""),
    ("language", LANG_UK),
];

pub fn default_db_path() -> PathBuf {
    let base = if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata)
    } else if let Ok(home) = std::env::var("HOME") {
        #[cfg(target_os = "macos")]
        {
            PathBuf::from(home).join("Library").join("Application Support")
        }
        #[cfg(not(target_os = "macos"))]
        {
            PathBuf::from(home)
        }
    } else {
        PathBuf::from(".")
    };
    let dir = base.join("AutoPara");
    let _ = fs::create_dir_all(&dir);
    dir.join("autopara.db")
}

fn schedules_dir(db_path: &Path) -> PathBuf {
    let dir = db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("schedules");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn iso_or_none(value: Option<NaiveDate>) -> Option<String> {
    value.map(|d| d.format("%Y-%m-%d").to_string())
}

fn date_or_none(value: Option<String>) -> Option<NaiveDate> {
    let s = value?;
    if s.is_empty() {
        return None;
    }
    NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()
}

fn now_iso_seconds() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

pub struct Storage {
    pub path: PathBuf,
    conn: Connection,
}

impl Storage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch(SCHEMA)?;
        let storage = Self { path, conn };
        storage.migrate()?;
        storage.seed_defaults()?;
        Ok(storage)
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        let storage = Self {
            path: PathBuf::from(":memory:"),
            conn,
        };
        storage.migrate()?;
        storage.seed_defaults()?;
        Ok(storage)
    }

    fn migrate(&self) -> Result<(), StorageError> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(lessons)")?;
        let cols: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .collect();
        for name in ["on_date", "repeat_until"] {
            if !cols.iter().any(|c| c == name) {
                self.conn
                    .execute(&format!("ALTER TABLE lessons ADD COLUMN {name} TEXT"), [])?;
            }
        }
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_lessons_on_date ON lessons(on_date)",
            [],
        )?;
        Ok(())
    }

    fn seed_defaults(&self) -> Result<(), StorageError> {
        for (key, value) in DEFAULTS {
            self.conn.execute(
                "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, ?2)",
                params![key, value],
            )?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------ settings

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, StorageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn settings(&self) -> Result<Settings, StorageError> {
        let as_int = |key: &str, fallback: i64| -> i64 {
            self.get_setting(key)
                .ok()
                .flatten()
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(fallback)
        };
        let as_str = |key: &str, fallback: &str| -> String {
            self.get_setting(key)
                .ok()
                .flatten()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| fallback.to_string())
        };

        let selected_course_id = self
            .get_setting("selected_course_id")?
            .and_then(|v| v.parse::<i64>().ok());
        let selected_group_id = self
            .get_setting("selected_group_id")?
            .and_then(|v| v.parse::<i64>().ok());
        let view_raw = as_str("view_mode", VIEW_WEEK);
        let view_mode = if VIEW_MODES.contains(&view_raw.as_str()) {
            view_raw
        } else {
            VIEW_WEEK.to_string()
        };
        let update_raw = as_str("update_mode", UPDATE_ASK);
        let update_mode = if UPDATE_MODES.contains(&update_raw.as_str()) {
            update_raw
        } else {
            UPDATE_ASK.to_string()
        };
        let lang_raw = as_str("language", LANG_UK);
        let language = if lang_raw == LANG_EN {
            LANG_EN.to_string()
        } else {
            LANG_UK.to_string()
        };

        Ok(Settings {
            selected_course_id,
            selected_group_id,
            lead_minutes: as_int("lead_minutes", 1),
            class_duration_minutes: as_int("class_duration_minutes", 80),
            autostart_enabled: self.get_setting("autostart_enabled")?.as_deref() != Some("0"),
            catchup_mode: as_str("catchup_mode", CATCHUP_NOTIFY),
            last_import_path: self.get_setting("last_import_path")?.unwrap_or_default(),
            notifications_enabled: self.get_setting("notifications_enabled")?.as_deref() != Some("0"),
            notify_minutes: as_int("notify_minutes", 10),
            theme: as_str("theme", THEME_SYSTEM),
            view_mode,
            update_mode,
            skipped_update_version: self
                .get_setting("skipped_update_version")?
                .unwrap_or_default(),
            last_update_check: self.get_setting("last_update_check")?.unwrap_or_default(),
            schedule_active_from: self
                .get_setting("schedule_active_from")?
                .unwrap_or_default(),
            schedule_copy_path: self.get_setting("schedule_copy_path")?.unwrap_or_default(),
            language,
            gcal_auto_open: self.get_setting("gcal_auto_open")?.as_deref() == Some("1"),
        })
    }

    // --------------------------------------------------------- personal workspace

    /// Ensure a standalone personal workspace exists (`ordinal = 0`) so users can add call links
    /// from scratch without ever importing a `.docx` timetable.
    pub fn ensure_personal_workspace(&self) -> Result<(Course, Group), StorageError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO courses(ordinal, name) VALUES (?1, ?2)",
            params![PERSONAL_COURSE_ORDINAL, "Мої созвони"],
        )?;
        let course: Course = self.conn.query_row(
            "SELECT id, ordinal, name FROM courses WHERE ordinal = ?1",
            params![PERSONAL_COURSE_ORDINAL],
            |r| {
                Ok(Course {
                    id: r.get(0)?,
                    ordinal: r.get(1)?,
                    name: r.get(2)?,
                })
            },
        )?;

        self.conn.execute(
            "INSERT OR IGNORE INTO groups(course_id, name, specialty, col_lo, col_hi) \
             VALUES (?1, ?2, '', 0, 0)",
            params![course.id, "Особисті"],
        )?;
        let group: Group = self.conn.query_row(
            "SELECT id, course_id, name, specialty, col_lo, col_hi \
             FROM groups WHERE course_id = ?1 AND name = ?2",
            params![course.id, "Особисті"],
            |r| {
                Ok(Group {
                    id: r.get(0)?,
                    course_id: r.get(1)?,
                    name: r.get(2)?,
                    specialty: r.get(3)?,
                    col_lo: r.get(4)?,
                    col_hi: r.get(5)?,
                })
            },
        )?;

        let current = self.settings()?;
        if current.selected_group_id.is_none()
            || current
                .selected_group_id
                .and_then(|gid| self.group(gid).ok().flatten())
                .is_none()
        {
            self.set_setting("selected_course_id", &course.id.to_string())?;
            self.set_setting("selected_group_id", &group.id.to_string())?;
        }

        Ok((course, group))
    }

    // ---------------------------------------------------------- courses & groups

    pub fn courses(&self) -> Result<Vec<Course>, StorageError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, ordinal, name FROM courses ORDER BY ordinal")?;
        let rows = stmt.query_map([], |r| {
            Ok(Course {
                id: r.get(0)?,
                ordinal: r.get(1)?,
                name: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn groups(&self, course_id: i64) -> Result<Vec<Group>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, course_id, name, specialty, col_lo, col_hi \
             FROM groups WHERE course_id = ?1 ORDER BY col_lo",
        )?;
        let rows = stmt.query_map(params![course_id], |r| {
            Ok(Group {
                id: r.get(0)?,
                course_id: r.get(1)?,
                name: r.get(2)?,
                specialty: r.get(3)?,
                col_lo: r.get(4)?,
                col_hi: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn course(&self, course_id: i64) -> Result<Option<Course>, StorageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, ordinal, name FROM courses WHERE id = ?1",
                params![course_id],
                |r| {
                    Ok(Course {
                        id: r.get(0)?,
                        ordinal: r.get(1)?,
                        name: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn group(&self, group_id: i64) -> Result<Option<Group>, StorageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, course_id, name, specialty, col_lo, col_hi FROM groups WHERE id = ?1",
                params![group_id],
                |r| {
                    Ok(Group {
                        id: r.get(0)?,
                        course_id: r.get(1)?,
                        name: r.get(2)?,
                        specialty: r.get(3)?,
                        col_lo: r.get(4)?,
                        col_hi: r.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    // -------------------------------------------------------------------- import

    pub fn import_courses(
        &mut self,
        parsed: &[ParsedCourse],
        source_path: &str,
    ) -> Result<(), StorageError> {
        let tx = self.conn.transaction()?;
        for course in parsed {
            tx.execute(
                "INSERT INTO courses(ordinal, name) VALUES (?1, ?2) \
                 ON CONFLICT(ordinal) DO UPDATE SET name = excluded.name",
                params![course.ordinal, course.name],
            )?;
            let course_id: i64 = tx.query_row(
                "SELECT id FROM courses WHERE ordinal = ?1",
                params![course.ordinal],
                |r| r.get(0),
            )?;

            let mut group_ids: HashMap<String, i64> = HashMap::new();
            for group in &course.groups {
                tx.execute(
                    "INSERT INTO groups(course_id, name, specialty, col_lo, col_hi) \
                     VALUES (?1, ?2, ?3, ?4, ?5) \
                     ON CONFLICT(course_id, name) DO UPDATE SET \
                     specialty = COALESCE(NULLIF(excluded.specialty, ''), specialty), col_lo = excluded.col_lo, col_hi = excluded.col_hi",
                    params![course_id, group.name, group.specialty, group.col_lo, group.col_hi],
                )?;
                let gid: i64 = tx.query_row(
                    "SELECT id FROM groups WHERE course_id = ?1 AND name = ?2",
                    params![course_id, group.name],
                    |r| r.get(0),
                )?;
                group_ids.insert(group.name.clone(), gid);
            }

            let incoming_keys: BTreeSet<String> = course
                .lessons
                .iter()
                .map(|l| l.source_key(course.ordinal))
                .collect();

            let existing: Vec<(i64, String)> = {
                let mut stmt = tx.prepare(
                    "SELECT id, source_key FROM lessons WHERE course_id = ?1 AND is_manual = 0",
                )?;
                stmt.query_map(params![course_id], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?
            };
            // a session file replaces the previous session, a weekly file the previous weeks
            let has_session = course.lessons.iter().any(|l| l.on_date.is_some());
            let has_weekly = !has_session || course.lessons.iter().any(|l| l.on_date.is_none());
            for (id, key) in existing {
                let replaced = if key.starts_with(SESSION_KEY_PREFIX) { has_session } else { has_weekly };
                if replaced && !incoming_keys.contains(&key) {
                    tx.execute("DELETE FROM lessons WHERE id = ?1", params![id])?;
                }
            }

            for lesson in &course.lessons {
                let key = lesson.source_key(course.ordinal);
                let found: Option<i64> = tx
                    .query_row(
                        "SELECT id FROM lessons WHERE course_id = ?1 AND source_key = ?2 AND is_manual = 0",
                        params![course_id, key],
                        |r| r.get(0),
                    )
                    .optional()?;
                let needs_link = i64::from(lesson.needs_link());
                let lesson_id = if let Some(id) = found {
                    tx.execute(
                        "UPDATE lessons SET course_id=?1, day_index=?2, pair=?3, start_time=?4, \
                         end_time=?5, subject=?6, teacher=?7, url=?8, provider=?9, needs_link=?10, \
                         source_key=?11, on_date=?12 WHERE id=?13",
                        params![
                            course_id,
                            lesson.day_index,
                            lesson.pair,
                            lesson.start_time,
                            lesson.end_time,
                            lesson.subject,
                            lesson.teacher,
                            lesson.url,
                            lesson.provider,
                            needs_link,
                            key,
                            iso_or_none(lesson.on_date),
                            id
                        ],
                    )?;
                    id
                } else {
                    tx.execute(
                        "INSERT INTO lessons(course_id, day_index, pair, start_time, end_time, \
                         subject, teacher, url, provider, needs_link, source_key, on_date) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                        params![
                            course_id,
                            lesson.day_index,
                            lesson.pair,
                            lesson.start_time,
                            lesson.end_time,
                            lesson.subject,
                            lesson.teacher,
                            lesson.url,
                            lesson.provider,
                            needs_link,
                            key,
                            iso_or_none(lesson.on_date)
                        ],
                    )?;
                    tx.last_insert_rowid()
                };

                tx.execute(
                    "DELETE FROM lesson_groups WHERE lesson_id = ?1",
                    params![lesson_id],
                )?;
                for name in &lesson.group_names {
                    if let Some(&gid) = group_ids.get(name) {
                        tx.execute(
                            "INSERT OR IGNORE INTO lesson_groups(lesson_id, group_id) VALUES (?1, ?2)",
                            params![lesson_id, gid],
                        )?;
                    }
                }
            }
        }
        tx.commit()?;

        self.clear_occurrences()?;
        self.set_setting("schedule_active_from", &now_iso_seconds())?;
        if !source_path.is_empty() {
            self.set_setting("last_import_path", source_path)?;
            if let Some(copy) = self.archive_schedule(Path::new(source_path)) {
                self.set_setting("schedule_copy_path", &copy.display().to_string())?;
            }
        }
        Ok(())
    }

    fn archive_schedule(&self, source: &Path) -> Option<PathBuf> {
        if !source.is_file() || self.path == Path::new(":memory:") {
            return None;
        }
        let dir = schedules_dir(&self.path);
        let name = source.file_name()?;
        let dest = dir.join(name);
        let src_canon = fs::canonicalize(source).ok()?;
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file()
                    && fs::canonicalize(&p).ok().as_deref() != Some(src_canon.as_path())
                {
                    let _ = fs::remove_file(p);
                }
            }
        }
        if fs::canonicalize(&dest).ok().as_deref() != Some(src_canon.as_path()) {
            fs::copy(source, &dest).ok()?;
        }
        Some(dest)
    }

    // ------------------------------------------------------------------- lessons

    fn hydrate_row(&self, row: &rusqlite::Row<'_>) -> Result<Lesson, rusqlite::Error> {
        let id: i64 = row.get("id")?;
        let mut g_stmt = self.conn.prepare(
            "SELECT g.name FROM lesson_groups lg JOIN groups g ON g.id = lg.group_id \
             WHERE lg.lesson_id = ?1 ORDER BY g.col_lo",
        )?;
        let group_names: Vec<String> = g_stmt
            .query_map(params![id], |r| r.get(0))?
            .filter_map(Result::ok)
            .collect();

        let mut s_stmt = self
            .conn
            .prepare("SELECT skip_date FROM lesson_skips WHERE lesson_id = ?1")?;
        let skip_dates: BTreeSet<NaiveDate> = s_stmt
            .query_map(params![id], |r| r.get::<_, String>(0))?
            .filter_map(Result::ok)
            .filter_map(|s| date_or_none(Some(s)))
            .collect();

        Ok(Lesson {
            id,
            course_id: row.get("course_id")?,
            day_index: row.get("day_index")?,
            pair: row.get("pair")?,
            start_time: row.get("start_time")?,
            end_time: row.get("end_time")?,
            subject: row.get("subject")?,
            teacher: row.get("teacher")?,
            url: row.get("url")?,
            provider: row.get("provider")?,
            needs_link: row.get::<_, i64>("needs_link")? != 0,
            is_manual: row.get::<_, i64>("is_manual")? != 0,
            source_key: row.get("source_key")?,
            group_names,
            on_date: date_or_none(row.get("on_date")?),
            repeat_until: date_or_none(row.get("repeat_until")?),
            skip_dates,
        })
    }

    pub fn lessons_for_group(&self, group_id: i64) -> Result<Vec<Lesson>, StorageError> {
        // Google Calendar meetings belong to the person, not to a group: show them in every group.
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT l.* FROM lessons l \
             LEFT JOIN lesson_groups lg ON lg.lesson_id = l.id \
             WHERE lg.group_id = ?1 OR l.source_key LIKE ?2 \
             ORDER BY l.day_index, l.pair, l.start_time",
        )?;
        let calendar_like = format!("{GCAL_KEY_PREFIX}%");
        let rows = stmt.query_map(params![group_id, calendar_like], |r| self.hydrate_row(r))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn lesson(&self, lesson_id: i64) -> Result<Option<Lesson>, StorageError> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM lessons WHERE id = ?1")?;
        Ok(stmt
            .query_row(params![lesson_id], |r| self.hydrate_row(r))
            .optional()?)
    }

    pub fn add_lesson(&self, lesson: &Lesson, group_ids: &[i64]) -> Result<i64, StorageError> {
        let day_index = lesson
            .on_date
            .map(|d| i64::from(d.weekday().num_days_from_monday()))
            .unwrap_or(lesson.day_index);
        let needs_link = i64::from(lesson.url.as_ref().is_none_or(|u| u.is_empty()));
        self.conn.execute(
            "INSERT INTO lessons(course_id, day_index, pair, start_time, end_time, subject, \
             teacher, url, provider, needs_link, is_manual, source_key, on_date, repeat_until) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?13, ?11, ?12)",
            params![
                lesson.course_id,
                day_index,
                lesson.pair,
                lesson.start_time,
                lesson.end_time,
                lesson.subject,
                lesson.teacher,
                lesson.url,
                lesson.provider,
                needs_link,
                iso_or_none(lesson.on_date),
                iso_or_none(lesson.repeat_until),
                lesson.source_key,
            ],
        )?;
        let lesson_id = self.conn.last_insert_rowid();
        for &gid in group_ids {
            self.conn.execute(
                "INSERT OR IGNORE INTO lesson_groups(lesson_id, group_id) VALUES (?1, ?2)",
                params![lesson_id, gid],
            )?;
        }
        Ok(lesson_id)
    }

    pub fn update_lesson(
        &self,
        lesson: &Lesson,
        group_ids: Option<&[i64]>,
    ) -> Result<(), StorageError> {
        let day_index = lesson
            .on_date
            .map(|d| i64::from(d.weekday().num_days_from_monday()))
            .unwrap_or(lesson.day_index);
        let needs_link = i64::from(lesson.url.as_ref().is_none_or(|u| u.is_empty()));
        self.conn.execute(
            "UPDATE lessons SET day_index=?1, pair=?2, start_time=?3, end_time=?4, subject=?5, \
             teacher=?6, url=?7, provider=?8, needs_link=?9, on_date=?10, repeat_until=?11 \
             WHERE id=?12",
            params![
                day_index,
                lesson.pair,
                lesson.start_time,
                lesson.end_time,
                lesson.subject,
                lesson.teacher,
                lesson.url,
                lesson.provider,
                needs_link,
                iso_or_none(lesson.on_date),
                iso_or_none(lesson.repeat_until),
                lesson.id,
            ],
        )?;
        if let Some(gids) = group_ids {
            self.conn.execute(
                "DELETE FROM lesson_groups WHERE lesson_id = ?1",
                params![lesson.id],
            )?;
            for &gid in gids {
                self.conn.execute(
                    "INSERT OR IGNORE INTO lesson_groups(lesson_id, group_id) VALUES (?1, ?2)",
                    params![lesson.id, gid],
                )?;
            }
        }
        Ok(())
    }

    pub fn move_lesson(
        &self,
        lesson_id: i64,
        day_index: i64,
        pair: i64,
        start_time: &str,
        end_time: &str,
        on_date: Option<NaiveDate>,
    ) -> Result<(), StorageError> {
        let Some(current) = self.lesson(lesson_id)? else {
            return Ok(());
        };
        if let (Some(curr_date), Some(target_date)) = (current.on_date, on_date) {
            let shift: Duration = target_date - curr_date;
            let until = current.repeat_until.map(|u| u + shift);
            self.conn.execute(
                "UPDATE lessons SET day_index=?1, pair=?2, start_time=?3, end_time=?4, \
                 on_date=?5, repeat_until=?6 WHERE id=?7",
                params![
                    i64::from(target_date.weekday().num_days_from_monday()),
                    pair,
                    start_time,
                    end_time,
                    target_date.format("%Y-%m-%d").to_string(),
                    iso_or_none(until),
                    lesson_id,
                ],
            )?;
        } else {
            self.conn.execute(
                "UPDATE lessons SET day_index=?1, pair=?2, start_time=?3, end_time=?4 WHERE id=?5",
                params![day_index, pair, start_time, end_time, lesson_id],
            )?;
        }
        Ok(())
    }

    pub fn move_occurrence(
        &self,
        lesson_id: i64,
        from_day: NaiveDate,
        to_day: NaiveDate,
        pair: i64,
        start_time: &str,
        end_time: &str,
    ) -> Result<Option<i64>, StorageError> {
        let Some(original) = self.lesson(lesson_id)? else {
            return Ok(None);
        };
        let group_ids: Vec<i64> = {
            let mut stmt = self
                .conn
                .prepare("SELECT group_id FROM lesson_groups WHERE lesson_id = ?1")?;
            stmt.query_map(params![lesson_id], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        self.conn.execute(
            "INSERT OR IGNORE INTO lesson_skips(lesson_id, skip_date) VALUES (?1, ?2)",
            params![lesson_id, from_day.format("%Y-%m-%d").to_string()],
        )?;
        let moved = Lesson {
            id: 0,
            course_id: original.course_id,
            day_index: i64::from(to_day.weekday().num_days_from_monday()),
            pair,
            start_time: start_time.to_string(),
            end_time: end_time.to_string(),
            subject: original.subject,
            teacher: original.teacher,
            url: original.url,
            provider: original.provider,
            needs_link: original.needs_link,
            is_manual: true,
            source_key: String::new(),
            group_names: Vec::new(),
            on_date: Some(to_day),
            repeat_until: None,
            skip_dates: BTreeSet::new(),
        };
        Ok(Some(self.add_lesson(&moved, &group_ids)?))
    }

    /// Make the calendar calls dated `from` or later match `events` exactly (insert, update,
    /// delete by `source_key`). Older ones are kept as history. Returns whether anything changed.
    pub fn sync_calendar_lessons(
        &self,
        events: &[Lesson],
        from: NaiveDate,
    ) -> Result<bool, StorageError> {
        let (course, group) = self.ensure_personal_workspace()?;
        let existing: HashMap<String, Lesson> = {
            let mut stmt = self.conn.prepare(
                "SELECT * FROM lessons WHERE source_key LIKE ?1 AND on_date >= ?2",
            )?;
            let rows = stmt.query_map(
                params![format!("{GCAL_KEY_PREFIX}%"), from.format("%Y-%m-%d").to_string()],
                |r| self.hydrate_row(r),
            )?;
            rows.filter_map(Result::ok)
                .map(|l| (l.source_key.clone(), l))
                .collect()
        };

        let tx = self.conn.unchecked_transaction()?;
        let mut changed = false;
        let mut seen = BTreeSet::new();
        for ev in events {
            seen.insert(ev.source_key.clone());
            let mut lesson = ev.clone();
            lesson.course_id = course.id;
            match existing.get(&ev.source_key) {
                Some(old) => {
                    lesson.id = old.id;
                    let same = old.start_time == lesson.start_time
                        && old.end_time == lesson.end_time
                        && old.on_date == lesson.on_date
                        && old.subject == lesson.subject
                        && old.url == lesson.url;
                    if !same {
                        self.update_lesson(&lesson, None)?;
                        changed = true;
                    }
                }
                None => {
                    self.add_lesson(&lesson, &[group.id])?;
                    changed = true;
                }
            }
        }
        for (key, old) in &existing {
            if !seen.contains(key) {
                self.delete_lesson(old.id)?;
                changed = true;
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    #[cfg(test)]
    pub(crate) fn conn_for_tests(&self) -> &Connection {
        &self.conn
    }

    /// Remove every imported timetable: its courses, groups and classes (cascade). The
    /// personal workspace stays, with the calls added by hand and the Google Calendar meetings.
    pub fn delete_imported_timetables(&self) -> Result<(), StorageError> {
        self.conn.execute(
            "DELETE FROM courses WHERE ordinal != ?1",
            params![PERSONAL_COURSE_ORDINAL],
        )?;
        self.set_setting("schedule_active_from", "")?;
        // the selected group is gone: this points the selection back at the personal one
        self.ensure_personal_workspace()?;
        Ok(())
    }

    /// Remove every calendar call (on sign-out).
    pub fn delete_calendar_lessons(&self) -> Result<(), StorageError> {
        self.conn.execute(
            "DELETE FROM lessons WHERE source_key LIKE ?1",
            params![format!("{GCAL_KEY_PREFIX}%")],
        )?;
        Ok(())
    }

    pub fn delete_lesson(&self, lesson_id: i64) -> Result<(), StorageError> {
        self.conn
            .execute("DELETE FROM lessons WHERE id = ?1", params![lesson_id])?;
        Ok(())
    }

    // --------------------------------------------------------------- occurrences

    /// Atomically claim today's occurrence (fire-once guarantee).
    pub fn claim_occurrence(
        &self,
        lesson_id: i64,
        day: NaiveDate,
        status: &str,
    ) -> Result<bool, StorageError> {
        let res = self.conn.execute(
            "INSERT INTO occurrences(lesson_id, occur_date, status, fired_at) \
             VALUES (?1, ?2, ?3, ?4)",
            params![
                lesson_id,
                day.format("%Y-%m-%d").to_string(),
                status,
                now_iso_seconds()
            ],
        );
        match res {
            Ok(_) => Ok(true),
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == ErrorCode::ConstraintViolation =>
            {
                Ok(false)
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn mark_occurrence(
        &self,
        lesson_id: i64,
        day: NaiveDate,
        status: &str,
    ) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO occurrences(lesson_id, occur_date, status, fired_at) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(lesson_id, occur_date) DO UPDATE SET \
             status = excluded.status, fired_at = excluded.fired_at",
            params![
                lesson_id,
                day.format("%Y-%m-%d").to_string(),
                status,
                now_iso_seconds()
            ],
        )?;
        Ok(())
    }

    pub fn clear_occurrences(&self) -> Result<(), StorageError> {
        self.conn.execute("DELETE FROM occurrences", [])?;
        Ok(())
    }

    pub fn clear_occurrence(&self, lesson_id: i64, day: NaiveDate) -> Result<(), StorageError> {
        self.conn.execute(
            "DELETE FROM occurrences WHERE lesson_id = ?1 AND occur_date = ?2",
            params![lesson_id, day.format("%Y-%m-%d").to_string()],
        )?;
        Ok(())
    }

    pub fn set_occurrence_status(
        &self,
        lesson_id: i64,
        day: NaiveDate,
        status: &str,
    ) -> Result<(), StorageError> {
        self.conn.execute(
            "UPDATE occurrences SET status = ?1, fired_at = ?2 \
             WHERE lesson_id = ?3 AND occur_date = ?4",
            params![
                status,
                now_iso_seconds(),
                lesson_id,
                day.format("%Y-%m-%d").to_string()
            ],
        )?;
        Ok(())
    }

    pub fn occurrence(
        &self,
        lesson_id: i64,
        day: NaiveDate,
    ) -> Result<Option<Occurrence>, StorageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, lesson_id, occur_date, status, fired_at \
                 FROM occurrences WHERE lesson_id = ?1 AND occur_date = ?2",
                params![lesson_id, day.format("%Y-%m-%d").to_string()],
                |r| {
                    Ok(Occurrence {
                        id: r.get(0)?,
                        lesson_id: r.get(1)?,
                        occur_date: r.get(2)?,
                        status: r.get(3)?,
                        fired_at: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn occurrences_between(
        &self,
        first: NaiveDate,
        last: NaiveDate,
    ) -> Result<HashMap<(i64, String), Occurrence>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, lesson_id, occur_date, status, fired_at \
             FROM occurrences WHERE occur_date BETWEEN ?1 AND ?2",
        )?;
        let rows = stmt.query_map(
            params![
                first.format("%Y-%m-%d").to_string(),
                last.format("%Y-%m-%d").to_string()
            ],
            |r| {
                Ok(Occurrence {
                    id: r.get(0)?,
                    lesson_id: r.get(1)?,
                    occur_date: r.get(2)?,
                    status: r.get(3)?,
                    fired_at: r.get(4)?,
                })
            },
        )?;
        let mut map = HashMap::new();
        for item in rows {
            let occ = item?;
            map.insert((occ.lesson_id, occ.occur_date.clone()), occ);
        }
        Ok(map)
    }

    pub fn occurrences_on(&self, day: NaiveDate) -> Result<HashMap<i64, Occurrence>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, lesson_id, occur_date, status, fired_at \
             FROM occurrences WHERE occur_date = ?1",
        )?;
        let rows = stmt.query_map(params![day.format("%Y-%m-%d").to_string()], |r| {
            Ok(Occurrence {
                id: r.get(0)?,
                lesson_id: r.get(1)?,
                occur_date: r.get(2)?,
                status: r.get(3)?,
                fired_at: r.get(4)?,
            })
        })?;
        let mut map = HashMap::new();
        for item in rows {
            let occ = item?;
            map.insert(occ.lesson_id, occ);
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_workspace_and_fire_once() {
        let storage = Storage::open_in_memory().unwrap();
        let (course, group) = storage.ensure_personal_workspace().unwrap();
        assert_eq!(course.ordinal, PERSONAL_COURSE_ORDINAL);

        let today = NaiveDate::from_ymd_opt(2026, 3, 2).unwrap();
        let lesson = Lesson {
            id: 0,
            course_id: course.id,
            day_index: 0,
            pair: 2,
            start_time: "09:30".into(),
            end_time: "10:15".into(),
            subject: "Daily standup".into(),
            teacher: String::new(),
            url: Some("https://meet.google.com/abc-defg-hij".into()),
            provider: "google_meet".into(),
            needs_link: false,
            is_manual: true,
            source_key: String::new(),
            group_names: vec![],
            on_date: Some(today),
            repeat_until: None,
            skip_dates: BTreeSet::new(),
        };
        let id = storage.add_lesson(&lesson, &[group.id]).unwrap();
        assert!(storage.claim_occurrence(id, today, "opened").unwrap());
        assert!(!storage.claim_occurrence(id, today, "opened").unwrap());
    }

    #[test]
    fn session_and_weekly_imports_do_not_wipe_each_other() {
        let file = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/sample.doc");
        let parsed = crate::schedule_parser::parse_file(file, None).unwrap();
        // put both tables of the sample into the same course
        let weekly = [ParsedCourse { ordinal: 2, ..parsed[0].clone() }];
        let session = [parsed[1].clone()];
        let mut storage = Storage::open_in_memory().unwrap();
        let counts = |storage: &Storage| {
            let count = |sql: &str| storage.conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
            (
                count("SELECT COUNT(*) FROM lessons WHERE on_date IS NULL"),
                count("SELECT COUNT(*) FROM lessons WHERE on_date IS NOT NULL"),
            )
        };
        storage.import_courses(&weekly, "").unwrap();
        storage.import_courses(&session, "").unwrap();
        storage.import_courses(&session, "").unwrap();
        assert_eq!(counts(&storage), (2, 4));
        storage.import_courses(&weekly, "").unwrap();
        assert_eq!(counts(&storage), (2, 4));

        let mut shorter = session.clone();
        shorter[0].lessons.truncate(1);
        storage.import_courses(&shorter, "").unwrap();
        assert_eq!(counts(&storage), (2, 1));
        let date: String = storage
            .conn
            .query_row("SELECT on_date FROM lessons WHERE on_date IS NOT NULL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(date, "2026-12-05");
    }
}
