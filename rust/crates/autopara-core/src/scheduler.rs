//! Deciding when a class opens, and opening it exactly once (PORT_SPEC §C, plan steps 3.2–3.6).
//!
//! The decisions are pure functions of a lesson and a time, kept apart so a reminder can never be
//! mistaken for a trigger. [`Scheduler::tick`] applies them to the chosen group's classes: it
//! claims a class in the open log *before* opening it, so the database's `UNIQUE(lesson, date)`
//! makes a second open impossible whatever happens to the process or the clock.
//!
//! One rule is stricter than the Python app's (DECISIONS.md, "Catch-up only from continuous
//! watching"): a class that is already running is opened without asking only when this scheduler
//! was running, awake and on an unbroken clock at the moment its trigger passed. After a start, a
//! wake from sleep or a clock change, a running class is always offered, never opened.

use std::collections::HashSet;
use std::time::Duration;

use chrono::{NaiveDate, NaiveDateTime, TimeDelta};
use serde::Serialize;

use crate::db::{DbError, Storage};
use crate::model::{CatchupMode, Lesson, OccurrenceStatus};

/// The longest the engine sleeps between two looks, however far away the next class is.
pub const TICK: Duration = Duration::from_secs(15);

/// How far the wall clock may drift from the monotonic clock between two ticks before it counts
/// as a clock change; and how long a gap between ticks still counts as continuous watching.
const JUMP_TOLERANCE: Duration = Duration::from_secs(30);
const MAX_GAP: Duration = Duration::from_secs(45);

/// What a lesson needs, right now, on `day`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    /// Inside the lead window: open it.
    Open,
    /// Started and still running, never opened: offer it (or, rarely, open it; see the module).
    Catchup,
    /// Over, never opened: record it as missed.
    Miss,
}

/// The pure trigger decision (Python `scheduler.evaluate`).
pub fn evaluate(
    lesson: &Lesson,
    now: NaiveDateTime,
    lead_minutes: u32,
    already_fired: bool,
    day: NaiveDate,
) -> Action {
    if already_fired || !lesson.occurs_on(day) {
        return Action::None;
    }
    let start = lesson.starts_on(day);
    let trigger = start - TimeDelta::minutes(i64::from(lead_minutes));
    if now < trigger {
        Action::None
    } else if now < start {
        Action::Open
    } else if now < lesson.ends_on(day) {
        Action::Catchup
    } else {
        Action::Miss
    }
}

/// Whether the advance reminder is due — a message, never a trigger (Python `reminder_due`).
pub fn reminder_due(
    lesson: &Lesson,
    now: NaiveDateTime,
    notify_minutes: u32,
    already_fired: bool,
    day: NaiveDate,
) -> bool {
    if already_fired || notify_minutes == 0 || !lesson.occurs_on(day) {
        return false;
    }
    let start = lesson.starts_on(day);
    start - TimeDelta::minutes(i64::from(notify_minutes)) <= now && now < start
}

/// Whether the lesson had already started when the timetable was imported: then it is not this
/// app's business at all — not opened, not offered, not marked (PORT_SPEC §C.2).
pub fn predates_schedule(
    lesson: &Lesson,
    day: NaiveDate,
    active_from: Option<NaiveDateTime>,
) -> bool {
    active_from.is_some_and(|from| lesson.starts_on(day) < from)
}

/// One class on one date, with its real start and end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlannedClass {
    pub lesson_id: i64,
    pub date: NaiveDate,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}

/// Every occurrence of `lessons` from `first` to `last` inclusive, in start order (step 3.2).
pub fn occurrences_between(
    lessons: &[Lesson],
    first: NaiveDate,
    last: NaiveDate,
) -> Vec<PlannedClass> {
    let mut planned: Vec<PlannedClass> = first
        .iter_days()
        .take_while(|day| *day <= last)
        .flat_map(|day| {
            lessons
                .iter()
                .filter(move |lesson| lesson.occurs_on(day))
                .map(move |lesson| PlannedClass {
                    lesson_id: lesson.id,
                    date: day,
                    start: lesson.starts_on(day),
                    end: lesson.ends_on(day),
                })
        })
        .collect();
    planned.sort_by_key(|class| (class.start, class.lesson_id));
    planned
}

/// Opens a meeting link. Returns whether the system accepted it.
pub trait LinkOpener {
    fn open(&mut self, url: &str) -> bool;
}

/// Only `https` links are ever handed to the system: link text comes from a document, and
/// `file:`, `javascript:` or plain `http:` must never reach the shell (plan step 3.5).
pub fn is_openable(url: Option<&str>) -> bool {
    url.is_some_and(|url| url.trim().to_lowercase().starts_with("https://"))
}

/// What the scheduler tells the rest of the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SchedulerEvent {
    /// The link was handed to the browser.
    Opened { lesson_id: i64, date: NaiveDate },
    /// The class ended without being opened, or the browser refused it.
    Missed { lesson_id: i64, date: NaiveDate },
    /// The class is running and was never opened: ask the user (the missed-class banner).
    CatchupAvailable { lesson_id: i64, date: NaiveDate },
    /// The advance reminder.
    Reminder { lesson_id: i64, date: NaiveDate },
}

/// The scheduler's memory between ticks. Everything that must survive a restart is in the
/// database; this only remembers what was already *said* in this run.
#[derive(Debug, Default)]
pub struct Scheduler {
    offered: HashSet<(i64, NaiveDate)>,
    reminded: HashSet<(i64, NaiveDate)>,
    last_tick: Option<(NaiveDateTime, Duration)>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// The wall time up to which this scheduler has been watching without a break, if it was
    /// watching right up to `now`: the previous tick was recent, and the wall clock moved in step
    /// with the monotonic one. `None` after a start, a sleep or a clock change.
    fn watched_since(&self, now: NaiveDateTime, monotonic: Duration) -> Option<NaiveDateTime> {
        let (wall, mono) = self.last_tick?;
        let gap = monotonic.checked_sub(mono)?;
        let wall_gap = (now - wall).to_std().ok()?;
        let drift = wall_gap.abs_diff(gap);
        (gap <= MAX_GAP && drift <= JUMP_TOLERANCE).then_some(wall)
    }

    /// Look at the chosen group's classes at `now` and act (PORT_SPEC §C.1).
    pub fn tick(
        &mut self,
        storage: &Storage,
        now: NaiveDateTime,
        monotonic: Duration,
        opener: &mut dyn LinkOpener,
    ) -> Result<Vec<SchedulerEvent>, DbError> {
        let watched_since = self.watched_since(now, monotonic);
        self.last_tick = Some((now, monotonic));

        let settings = storage.settings()?;
        let Some(group_id) = settings.selected_group_id else {
            return Ok(Vec::new());
        };
        let today = now.date();
        let fired: HashSet<i64> = storage
            .occurrences_between(today, today)?
            .into_iter()
            .map(|occurrence| occurrence.lesson_id)
            .collect();

        let mut events = Vec::new();
        for lesson in storage.lessons_for_group(group_id)? {
            if !lesson.occurs_on(today)
                || predates_schedule(&lesson, today, settings.schedule_active_from)
            {
                continue;
            }
            let already_fired = fired.contains(&lesson.id);
            let key = (lesson.id, today);
            if settings.notifications_enabled
                && reminder_due(&lesson, now, settings.notify_minutes, already_fired, today)
                && self.reminded.insert(key)
            {
                events.push(SchedulerEvent::Reminder {
                    lesson_id: lesson.id,
                    date: today,
                });
            }

            match evaluate(&lesson, now, settings.lead_minutes, already_fired, today) {
                Action::None => {}
                Action::Open => self.open(storage, &lesson, today, now, opener, &mut events)?,
                Action::Miss => self.claim_missed(storage, &lesson, today, now, &mut events)?,
                Action::Catchup => {
                    let trigger = lesson.starts_on(today)
                        - TimeDelta::minutes(i64::from(settings.lead_minutes));
                    // Opening a running class unasked needs the trigger to have passed while this
                    // scheduler was watching (the previous tick was before it, unbroken since).
                    let saw_the_trigger = watched_since.is_some_and(|since| since < trigger);
                    if !is_openable(lesson.url.as_deref()) {
                        self.claim_missed(storage, &lesson, today, now, &mut events)?;
                    } else if settings.catchup_mode == CatchupMode::Open && saw_the_trigger {
                        self.open(storage, &lesson, today, now, opener, &mut events)?;
                    } else if settings.catchup_mode == CatchupMode::Missed {
                        self.claim_missed(storage, &lesson, today, now, &mut events)?;
                    } else if self.offered.insert(key) {
                        events.push(SchedulerEvent::CatchupAvailable {
                            lesson_id: lesson.id,
                            date: today,
                        });
                    }
                }
            }
        }
        Ok(events)
    }

    /// Claim first, then open: the claim is the fire-once guarantee.
    fn open(
        &mut self,
        storage: &Storage,
        lesson: &Lesson,
        day: NaiveDate,
        now: NaiveDateTime,
        opener: &mut dyn LinkOpener,
        events: &mut Vec<SchedulerEvent>,
    ) -> Result<(), DbError> {
        let Some(url) = lesson.url.as_deref().filter(|url| is_openable(Some(url))) else {
            return self.claim_missed(storage, lesson, day, now, events);
        };
        if !storage.claim_occurrence(lesson.id, day, OccurrenceStatus::Opened, now)? {
            return Ok(()); // someone else (another tick, another process) already did
        }
        if opener.open(url.trim()) {
            events.push(SchedulerEvent::Opened {
                lesson_id: lesson.id,
                date: day,
            });
        } else {
            // The claim stays, so a browser that refuses does not cause a retry every tick.
            storage.set_occurrence_status(lesson.id, day, OccurrenceStatus::Missed, now)?;
            events.push(SchedulerEvent::Missed {
                lesson_id: lesson.id,
                date: day,
            });
        }
        Ok(())
    }

    fn claim_missed(
        &mut self,
        storage: &Storage,
        lesson: &Lesson,
        day: NaiveDate,
        now: NaiveDateTime,
        events: &mut Vec<SchedulerEvent>,
    ) -> Result<(), DbError> {
        if storage.claim_occurrence(lesson.id, day, OccurrenceStatus::Missed, now)? {
            events.push(SchedulerEvent::Missed {
                lesson_id: lesson.id,
                date: day,
            });
        }
        Ok(())
    }

    /// The user asked to join (a card click, or "Підключитися зараз"). `claim` is false for a
    /// date after today, so peeking at next week's class does not consume it.
    pub fn open_now(
        &mut self,
        storage: &Storage,
        lesson_id: i64,
        day: NaiveDate,
        claim: bool,
        now: NaiveDateTime,
        opener: &mut dyn LinkOpener,
    ) -> Result<bool, DbError> {
        let Some(lesson) = storage.lesson(lesson_id)? else {
            return Ok(false);
        };
        let Some(url) = lesson.url.as_deref().filter(|url| is_openable(Some(url))) else {
            return Ok(false);
        };
        if claim {
            // Free or not, opening again is the user's explicit choice.
            storage.claim_occurrence(lesson.id, day, OccurrenceStatus::Manual, now)?;
        }
        let opened = opener.open(url.trim());
        if opened && claim {
            storage.set_occurrence_status(lesson.id, day, OccurrenceStatus::Manual, now)?;
        }
        Ok(opened)
    }

    /// The user's own verdict from the card menu; it also settles the class for that date.
    pub fn mark(
        &mut self,
        storage: &Storage,
        lesson_id: i64,
        day: NaiveDate,
        status: OccurrenceStatus,
        now: NaiveDateTime,
    ) -> Result<(), DbError> {
        storage.mark_occurrence(lesson_id, day, status, now)?;
        self.offered.insert((lesson_id, day));
        Ok(())
    }

    /// Undo a mark: the class may open again that day.
    pub fn clear_mark(
        &mut self,
        storage: &Storage,
        lesson_id: i64,
        day: NaiveDate,
    ) -> Result<(), DbError> {
        storage.clear_occurrence(lesson_id, day)?;
        self.offered.remove(&(lesson_id, day));
        Ok(())
    }

    /// When the next decision could be due after `now`: the earliest reminder, trigger, start or
    /// end still ahead today, capped at [`TICK`] so settings changes and clock jumps are noticed.
    pub fn next_wake(&self, storage: &Storage, now: NaiveDateTime) -> Result<Duration, DbError> {
        let settings = storage.settings()?;
        let Some(group_id) = settings.selected_group_id else {
            return Ok(TICK);
        };
        let today = now.date();
        let lead = TimeDelta::minutes(i64::from(settings.lead_minutes));
        let notify = TimeDelta::minutes(i64::from(settings.notify_minutes));
        let soonest = storage
            .lessons_for_group(group_id)?
            .iter()
            .filter(|lesson| lesson.occurs_on(today))
            .flat_map(|lesson| {
                let start = lesson.starts_on(today);
                [start - notify, start - lead, start, lesson.ends_on(today)]
            })
            // Midnight starts a new day's classes.
            .chain(
                today
                    .succ_opt()
                    .and_then(|tomorrow| tomorrow.and_hms_opt(0, 0, 0)),
            )
            .filter(|moment| *moment > now)
            .min();
        Ok(soonest
            .and_then(|moment| (moment - now).to_std().ok())
            .map_or(TICK, |wait| wait.min(TICK)))
    }
}
