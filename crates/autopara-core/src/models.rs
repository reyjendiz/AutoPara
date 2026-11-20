use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::normalize::last_pair_covered;

pub const STATUS_OPENED: &str = "opened";
pub const STATUS_MANUAL: &str = "manual";
pub const STATUS_MISSED: &str = "missed";
pub const STATUS_SKIPPED: &str = "skipped";

pub const PROVIDER_ZOOM: &str = "zoom";
pub const PROVIDER_MEET: &str = "google_meet";
pub const PROVIDER_UNKNOWN: &str = "unknown";

pub const CATCHUP_NOTIFY: &str = "notify";
pub const CATCHUP_OPEN: &str = "open";
pub const CATCHUP_MISSED: &str = "missed";

pub const VIEW_DAY: &str = "day";
pub const VIEW_WEEK: &str = "week";
pub const VIEW_MONTH: &str = "month";
pub const VIEW_MODES: [&str; 3] = [VIEW_DAY, VIEW_WEEK, VIEW_MONTH];

pub const UPDATE_ASK: &str = "ask";
pub const UPDATE_AUTO: &str = "auto";
pub const UPDATE_OFF: &str = "off";
pub const UPDATE_MODES: [&str; 3] = [UPDATE_ASK, UPDATE_AUTO, UPDATE_OFF];

pub const THEME_SYSTEM: &str = "system";
pub const THEME_LIGHT: &str = "light";
pub const THEME_DARK: &str = "dark";

pub const LANG_UK: &str = "uk";
pub const LANG_EN: &str = "en";

/// Reserved ordinal for the built-in personal calls workspace (no `.docx` needed).
pub const PERSONAL_COURSE_ORDINAL: i64 = 0;

pub fn parse_hhmm(value: &str) -> Option<NaiveTime> {
    let mut parts = value.split(':');
    let hour: u32 = parts.next()?.parse().ok()?;
    let minute: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    NaiveTime::from_hms_opt(hour, minute, 0)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Course {
    pub id: i64,
    pub ordinal: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub id: i64,
    pub course_id: i64,
    pub name: String,
    pub specialty: String,
    pub col_lo: i64,
    pub col_hi: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lesson {
    pub id: i64,
    pub course_id: i64,
    /// 0 = Mon .. 6 = Sun
    pub day_index: i64,
    pub pair: i64,
    /// 'HH:MM'
    pub start_time: String,
    /// 'HH:MM'
    pub end_time: String,
    pub subject: String,
    pub teacher: String,
    pub url: Option<String>,
    pub provider: String,
    pub needs_link: bool,
    pub is_manual: bool,
    pub source_key: String,
    pub group_names: Vec<String>,
    pub on_date: Option<NaiveDate>,
    pub repeat_until: Option<NaiveDate>,
    pub skip_dates: BTreeSet<NaiveDate>,
}

impl Lesson {
    pub fn repeats(&self) -> bool {
        match self.on_date {
            None => true,
            Some(start) => self.repeat_until.is_some_and(|until| until > start),
        }
    }

    pub fn is_dated(&self) -> bool {
        self.on_date.is_some()
    }

    /// Single source of truth for whether this lesson occurs on `day`.
    pub fn occurs_on(&self, day: NaiveDate) -> bool {
        if i64::from(day.weekday().num_days_from_monday()) != self.day_index {
            return false;
        }
        if self.skip_dates.contains(&day) {
            return false;
        }
        match self.on_date {
            None => true,
            Some(start) => {
                let last = self.repeat_until.unwrap_or(start).max(start);
                day >= start && day <= last
            }
        }
    }

    pub fn start(&self) -> Option<NaiveTime> {
        parse_hhmm(&self.start_time)
    }

    pub fn end(&self) -> Option<NaiveTime> {
        parse_hhmm(&self.end_time)
    }

    pub fn starts_on(&self, day: NaiveDate) -> Option<NaiveDateTime> {
        self.start().map(|t| day.and_time(t))
    }

    pub fn ends_on(&self, day: NaiveDate) -> Option<NaiveDateTime> {
        self.end().map(|t| day.and_time(t))
    }

    pub fn pair_span(&self) -> i64 {
        (last_pair_covered(&self.start_time, &self.end_time) - self.pair + 1).max(1)
    }

    pub fn time_range(&self) -> String {
        format!("{}–{}", self.start_time, self.end_time)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Occurrence {
    pub id: i64,
    pub lesson_id: i64,
    pub occur_date: String,
    pub status: String,
    pub fired_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub selected_course_id: Option<i64>,
    pub selected_group_id: Option<i64>,
    pub lead_minutes: i64,
    pub class_duration_minutes: i64,
    pub autostart_enabled: bool,
    pub catchup_mode: String,
    pub last_import_path: String,
    pub notifications_enabled: bool,
    pub notify_minutes: i64,
    pub theme: String,
    pub view_mode: String,
    pub update_mode: String,
    pub skipped_update_version: String,
    pub last_update_check: String,
    pub schedule_active_from: String,
    pub schedule_copy_path: String,
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            selected_course_id: None,
            selected_group_id: None,
            lead_minutes: 1,
            class_duration_minutes: 80,
            autostart_enabled: true,
            catchup_mode: CATCHUP_NOTIFY.to_string(),
            last_import_path: String::new(),
            notifications_enabled: true,
            notify_minutes: 10,
            theme: THEME_SYSTEM.to_string(),
            view_mode: VIEW_WEEK.to_string(),
            update_mode: UPDATE_ASK.to_string(),
            skipped_update_version: String::new(),
            last_update_check: String::new(),
            schedule_active_from: String::new(),
            schedule_copy_path: String::new(),
            language: LANG_UK.to_string(),
        }
    }
}
