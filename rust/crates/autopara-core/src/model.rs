//! Domain types (PORT_SPEC §B, §D). The database stores strings and integers; these types are what
//! the rest of the program works with.

use std::collections::BTreeSet;
use std::fmt;

use chrono::{Datelike, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::time::{TimeOfDay, last_pair_covered};

/// A day of the week, Monday first. Stored and serialised as 0 (Monday) ..= 6 (Sunday).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(into = "u8", try_from = "u8")]
#[ts(export, type = "number")]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    pub const ALL: [Weekday; 7] = [
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
        Weekday::Saturday,
        Weekday::Sunday,
    ];

    pub const fn index(self) -> u8 {
        self as u8
    }

    pub fn from_index(index: u8) -> Option<Self> {
        Self::ALL.get(usize::from(index)).copied()
    }

    pub fn of(date: NaiveDate) -> Self {
        Self::ALL[date.weekday().num_days_from_monday() as usize]
    }
}

impl From<Weekday> for u8 {
    fn from(day: Weekday) -> Self {
        day.index()
    }
}

impl TryFrom<u8> for Weekday {
    type Error = String;

    fn try_from(index: u8) -> Result<Self, Self::Error> {
        Self::from_index(index).ok_or_else(|| format!("no weekday {index}"))
    }
}

/// Generates a string-backed enum whose spelling is what the database and the Python app use.
macro_rules! stored_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
        #[ts(export)]
        pub enum $name {
            $(#[serde(rename = $text)] $variant),+
        }

        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }

            pub fn parse(text: &str) -> Option<Self> {
                match text { $($text => Some(Self::$variant),)+ _ => None }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

stored_enum!(
    /// The meeting service a link points at.
    Provider { Zoom = "zoom", GoogleMeet = "google_meet", Unknown = "unknown" }
);

stored_enum!(
    /// What became of one class on one date (the open log).
    OccurrenceStatus { Opened = "opened", Manual = "manual", Missed = "missed", Skipped = "skipped" }
);

stored_enum!(
    /// What to do about a class whose start has already passed.
    CatchupMode { Notify = "notify", Open = "open", Missed = "missed" }
);

stored_enum!(
    /// The colour scheme; `System` follows the OS.
    Theme { System = "system", Light = "light", Dark = "dark" }
);

stored_enum!(
    /// What the calendar shows.
    ViewMode { Day = "day", Week = "week", Month = "month" }
);

stored_enum!(
    /// Whether, and how, to look for a newer release.
    UpdateMode { Ask = "ask", Auto = "auto", Off = "off" }
);

/// When a lesson happens (PORT_SPEC §A.10, "Lessons that belong to a date").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Recurrence {
    /// Every week on the lesson's weekday: every imported class.
    Weekly,
    /// That date only.
    Once { date: NaiveDate },
    /// Every week from `from` to `until`, both included.
    WeeklyUntil { from: NaiveDate, until: NaiveDate },
}

impl Recurrence {
    /// The two nullable columns the database keeps: `(on_date, repeat_until)`.
    pub fn columns(self) -> (Option<NaiveDate>, Option<NaiveDate>) {
        match self {
            Recurrence::Weekly => (None, None),
            Recurrence::Once { date } => (Some(date), None),
            Recurrence::WeeklyUntil { from, until } => (Some(from), Some(until)),
        }
    }

    pub fn from_columns(on_date: Option<NaiveDate>, repeat_until: Option<NaiveDate>) -> Self {
        match (on_date, repeat_until) {
            (None, _) => Recurrence::Weekly,
            (Some(date), None) => Recurrence::Once { date },
            (Some(from), Some(until)) => Recurrence::WeeklyUntil { from, until },
        }
    }

    pub fn first_date(self) -> Option<NaiveDate> {
        self.columns().0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Course {
    #[ts(type = "number")]
    pub id: i64,
    pub ordinal: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Group {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub course_id: i64,
    pub name: String,
    pub specialty: String,
    pub col_lo: u32,
    pub col_hi: u32,
}

/// One class of the timetable: a weekly row, a one-off, or a weekly series.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Lesson {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub course_id: i64,
    pub weekday: Weekday,
    /// The slot the lesson starts in; part of `source_key`, no longer used for drawing.
    pub pair: u32,
    pub start: TimeOfDay,
    pub end: TimeOfDay,
    pub subject: String,
    /// As written in the document, parentheses included.
    pub teacher: String,
    pub url: Option<String>,
    pub provider: Provider,
    pub needs_link: bool,
    pub is_manual: bool,
    pub source_key: String,
    pub group_names: Vec<String>,
    pub recurrence: Recurrence,
    /// Dates the pattern would include but the class was moved away from.
    pub skip_dates: BTreeSet<NaiveDate>,
}

impl Lesson {
    /// Whether the lesson takes place on `day` — the single rule every view and the scheduler use.
    pub fn occurs_on(&self, day: NaiveDate) -> bool {
        if Weekday::of(day) != self.weekday || self.skip_dates.contains(&day) {
            return false;
        }
        match self.recurrence {
            Recurrence::Weekly => true,
            Recurrence::Once { date } => day == date,
            Recurrence::WeeklyUntil { from, until } => from <= day && day <= until.max(from),
        }
    }

    /// A weekly row, or a series that spans more than one date.
    pub fn repeats(&self) -> bool {
        match self.recurrence {
            Recurrence::Weekly => true,
            Recurrence::Once { .. } => false,
            Recurrence::WeeklyUntil { from, until } => until > from,
        }
    }

    pub fn starts_on(&self, day: NaiveDate) -> NaiveDateTime {
        at(day, self.start)
    }

    pub fn ends_on(&self, day: NaiveDate) -> NaiveDateTime {
        at(day, self.end)
    }

    /// How many consecutive pairs the lesson covers (a merged block covers several).
    pub fn pair_span(&self) -> u32 {
        (u32::from(last_pair_covered(self.start, self.end)) + 1)
            .saturating_sub(self.pair)
            .max(1)
    }
}

/// `day` at `time`, as a naive local date-time.
pub fn at(day: NaiveDate, time: TimeOfDay) -> NaiveDateTime {
    day.and_hms_opt(u32::from(time.hour()), u32::from(time.minute()), 0)
        .expect("a TimeOfDay is always a valid time")
}

/// One row of the open log: what became of `lesson_id` on `date`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Occurrence {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub lesson_id: i64,
    pub date: NaiveDate,
    pub status: OccurrenceStatus,
    pub fired_at: NaiveDateTime,
}

/// The typed view of the `settings` table, with the Python app's defaults (PORT_SPEC §D).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Settings {
    #[ts(type = "number | null")]
    pub selected_course_id: Option<i64>,
    #[ts(type = "number | null")]
    pub selected_group_id: Option<i64>,
    pub lead_minutes: u32,
    pub class_duration_minutes: u32,
    pub autostart_enabled: bool,
    pub catchup_mode: CatchupMode,
    pub notifications_enabled: bool,
    pub notify_minutes: u32,
    pub theme: Theme,
    pub view_mode: ViewMode,
    pub update_mode: UpdateMode,
    pub skipped_update_version: String,
    pub last_update_check: Option<NaiveDateTime>,
    pub schedule_active_from: Option<NaiveDateTime>,
    pub schedule_copy_path: String,
    pub last_import_path: String,
    /// Join a class opened in AutoPara's tab by itself, with the camera and microphone off.
    pub auto_join: bool,
    /// The name a class page asks for when nobody is signed in.
    pub join_name: String,
    /// Meet and Zoom classes in AutoPara's tab; off, in the system browser (which hands a Zoom
    /// link to the Zoom app).
    pub open_in_app: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            selected_course_id: None,
            selected_group_id: None,
            lead_minutes: 1,
            class_duration_minutes: 80,
            autostart_enabled: true,
            catchup_mode: CatchupMode::Notify,
            notifications_enabled: true,
            notify_minutes: 10,
            theme: Theme::System,
            view_mode: ViewMode::Week,
            update_mode: UpdateMode::Ask,
            skipped_update_version: String::new(),
            last_update_check: None,
            schedule_active_from: None,
            schedule_copy_path: String::new(),
            last_import_path: String::new(),
            auto_join: false,
            join_name: String::new(),
            open_in_app: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    fn lesson(recurrence: Recurrence, weekday: Weekday) -> Lesson {
        Lesson {
            id: 1,
            course_id: 1,
            weekday,
            pair: 1,
            start: "08:00".parse().unwrap(),
            end: "09:20".parse().unwrap(),
            subject: "S".into(),
            teacher: String::new(),
            url: None,
            provider: Provider::Unknown,
            needs_link: true,
            is_manual: false,
            source_key: String::new(),
            group_names: vec![],
            recurrence,
            skip_dates: BTreeSet::new(),
        }
    }

    #[test]
    fn weekly_occurs_on_its_weekday_only() {
        let weekly = lesson(Recurrence::Weekly, Weekday::Monday);
        assert!(weekly.occurs_on(date("2026-10-05")));
        assert!(!weekly.occurs_on(date("2026-10-06")));
        assert!(weekly.repeats());
    }

    #[test]
    fn a_series_is_bounded_and_skips_moved_dates() {
        let mut series = lesson(
            Recurrence::WeeklyUntil {
                from: date("2026-10-05"),
                until: date("2026-10-19"),
            },
            Weekday::Monday,
        );
        series.skip_dates.insert(date("2026-10-12"));
        let days: Vec<bool> = [
            "2026-09-28",
            "2026-10-05",
            "2026-10-12",
            "2026-10-19",
            "2026-10-26",
        ]
        .map(|d| series.occurs_on(date(d)))
        .to_vec();
        assert_eq!(days, [false, true, false, true, false]);
    }

    #[test]
    fn a_one_off_does_not_repeat() {
        let once = lesson(
            Recurrence::Once {
                date: date("2026-10-07"),
            },
            Weekday::Wednesday,
        );
        assert!(once.occurs_on(date("2026-10-07")));
        assert!(!once.occurs_on(date("2026-10-14")));
        assert!(!once.repeats());
    }

    #[test]
    fn stored_spellings_round_trip() {
        assert_eq!(Provider::parse("google_meet"), Some(Provider::GoogleMeet));
        assert_eq!(OccurrenceStatus::Skipped.as_str(), "skipped");
        assert_eq!(serde_json::to_string(&Weekday::Sunday).unwrap(), "6");
    }
}
