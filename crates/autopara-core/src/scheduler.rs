use chrono::{Duration, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::launcher::{is_openable, open_url};
use crate::models::{
    CATCHUP_MISSED, CATCHUP_OPEN, Lesson, STATUS_MANUAL, STATUS_MISSED, STATUS_OPENED,
};
use crate::storage::{Storage, StorageError};

pub const TICK_SECONDS: u64 = 15;

pub const ACTION_NONE: &str = "none";
pub const ACTION_OPEN: &str = "open";
pub const ACTION_CATCHUP: &str = "catchup";
pub const ACTION_MISS: &str = "miss";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub action: String,
    pub lesson_id: i64,
    pub reason: String,
}

impl Decision {
    fn new(action: &str, lesson_id: i64, reason: &str) -> Self {
        Self {
            action: action.to_string(),
            lesson_id,
            reason: reason.to_string(),
        }
    }
}

/// Pure decision function for a single lesson (matches Python `scheduler.evaluate` 1:1).
pub fn evaluate(
    lesson: &Lesson,
    now: NaiveDateTime,
    lead_minutes: i64,
    already_fired: bool,
    today: Option<NaiveDate>,
) -> Decision {
    let day = today.unwrap_or_else(|| now.date());
    if already_fired {
        return Decision::new(ACTION_NONE, lesson.id, "already fired");
    }
    if !lesson.occurs_on(day) {
        return Decision::new(ACTION_NONE, lesson.id, "not scheduled today");
    }
    let (Some(start), Some(end)) = (lesson.starts_on(day), lesson.ends_on(day)) else {
        return Decision::new(ACTION_NONE, lesson.id, "invalid time");
    };
    let trigger = start - Duration::minutes(lead_minutes.max(0));
    if now < trigger {
        return Decision::new(ACTION_NONE, lesson.id, "not due yet");
    }
    if now < start {
        return Decision::new(ACTION_OPEN, lesson.id, "within lead window");
    }
    if now < end {
        return Decision::new(ACTION_CATCHUP, lesson.id, "class already started");
    }
    Decision::new(ACTION_MISS, lesson.id, "class ended")
}

pub fn parse_active_from(value: &str) -> Option<NaiveDateTime> {
    if value.is_empty() {
        return None;
    }
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f"))
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S"))
        .ok()
}

pub fn predates_schedule(
    lesson: &Lesson,
    day: NaiveDate,
    active_from: Option<NaiveDateTime>,
) -> bool {
    let Some(active) = active_from else {
        return false;
    };
    lesson.starts_on(day).is_some_and(|start| start < active)
}

/// Pure advance reminder predicate — deliberately separate from `evaluate`.
pub fn reminder_due(
    lesson: &Lesson,
    now: NaiveDateTime,
    notify_minutes: i64,
    already_fired: bool,
    today: Option<NaiveDate>,
) -> bool {
    let day = today.unwrap_or_else(|| now.date());
    if already_fired || notify_minutes <= 0 || !lesson.occurs_on(day) {
        return false;
    }
    let Some(start) = lesson.starts_on(day) else {
        return false;
    };
    let remind_at = start - Duration::minutes(notify_minutes);
    now >= remind_at && now < start
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SchedulerEvent {
    LessonOpened { lesson_id: i64, subject: String },
    LessonMissed { lesson_id: i64, subject: String },
    CatchupAvailable { lesson_id: i64, subject: String, start_time: String, url: String },
    ReminderDue { lesson_id: i64, subject: String, start_time: String, minutes: i64 },
}

pub struct SchedulerEngine {
    notified: HashSet<(i64, String)>,
    reminded: HashSet<(i64, String)>,
    /// Cold-start guard: first tick never auto-opens an already-running class regardless of setting.
    cold_start: bool,
}

impl Default for SchedulerEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SchedulerEngine {
    pub fn new() -> Self {
        Self {
            notified: HashSet::new(),
            reminded: HashSet::new(),
            cold_start: true,
        }
    }

    pub fn tick(
        &mut self,
        storage: &Storage,
        now: NaiveDateTime,
    ) -> Result<(Vec<Decision>, Vec<SchedulerEvent>), StorageError> {
        self.tick_with_opener(storage, now, |url| open_url(Some(url)))
    }

    pub fn tick_with_opener<F>(
        &mut self,
        storage: &Storage,
        now: NaiveDateTime,
        mut opener: F,
    ) -> Result<(Vec<Decision>, Vec<SchedulerEvent>), StorageError>
    where
        F: FnMut(&str) -> bool,
    {
        let today = now.date();
        let today_iso = today.format("%Y-%m-%d").to_string();
        let settings = storage.settings()?;
        let Some(group_id) = settings.selected_group_id else {
            self.cold_start = false;
            return Ok((Vec::new(), Vec::new()));
        };

        let fired = storage.occurrences_on(today)?;
        let active_from = parse_active_from(&settings.schedule_active_from);
        let cold = self.cold_start;
        self.cold_start = false;

        let lessons: Vec<Lesson> = storage
            .lessons_for_group(group_id)?
            .into_iter()
            .filter(|l| l.occurs_on(today))
            .collect();

        let mut decisions = Vec::new();
        let mut events = Vec::new();

        for lesson in lessons {
            // Manual calls added by the user are always active; imported timetables honor active_from.
            if !lesson.is_manual && predates_schedule(&lesson, today, active_from) {
                decisions.push(Decision::new(ACTION_NONE, lesson.id, "predates the import"));
                continue;
            }
            let already_fired = fired.contains_key(&lesson.id);

            if settings.notifications_enabled
                && reminder_due(&lesson, now, settings.notify_minutes, already_fired, Some(today))
            {
                let key = (lesson.id, today_iso.clone());
                if self.reminded.insert(key) {
                    let mins = lesson
                        .starts_on(today)
                        .map(|s| ((s - now).num_seconds() as f64 / 60.0).round() as i64)
                        .unwrap_or(settings.notify_minutes)
                        .max(1);
                    events.push(SchedulerEvent::ReminderDue {
                        lesson_id: lesson.id,
                        subject: lesson.subject.clone(),
                        start_time: lesson.start_time.clone(),
                        minutes: mins,
                    });
                }
            }

            let decision = evaluate(&lesson, now, settings.lead_minutes, already_fired, Some(today));
            let action = decision.action.clone();
            decisions.push(decision);

            match action.as_str() {
                ACTION_OPEN => {
                    self.perform_open(storage, &lesson, today, STATUS_OPENED, &mut opener, &mut events)?;
                }
                ACTION_CATCHUP => {
                    if !is_openable(lesson.url.as_deref()) {
                        self.claim_only(storage, &lesson, today, STATUS_MISSED, &mut events)?;
                    } else if settings.catchup_mode == CATCHUP_OPEN && !cold {
                        self.perform_open(storage, &lesson, today, STATUS_OPENED, &mut opener, &mut events)?;
                    } else if settings.catchup_mode == CATCHUP_MISSED {
                        self.claim_only(storage, &lesson, today, STATUS_MISSED, &mut events)?;
                    } else {
                        let key = (lesson.id, today_iso.clone());
                        if self.notified.insert(key) {
                            events.push(SchedulerEvent::CatchupAvailable {
                                lesson_id: lesson.id,
                                subject: lesson.subject.clone(),
                                start_time: lesson.start_time.clone(),
                                url: lesson.url.clone().unwrap_or_default(),
                            });
                        }
                    }
                }
                ACTION_MISS => {
                    self.claim_only(storage, &lesson, today, STATUS_MISSED, &mut events)?;
                }
                _ => {}
            }
        }

        Ok((decisions, events))
    }

    fn perform_open<F>(
        &self,
        storage: &Storage,
        lesson: &Lesson,
        today: NaiveDate,
        status: &str,
        opener: &mut F,
        events: &mut Vec<SchedulerEvent>,
    ) -> Result<bool, StorageError>
    where
        F: FnMut(&str) -> bool,
    {
        if !is_openable(lesson.url.as_deref()) {
            self.claim_only(storage, lesson, today, STATUS_MISSED, events)?;
            return Ok(false);
        }
        if !storage.claim_occurrence(lesson.id, today, status)? {
            return Ok(false);
        }
        // INFALLIBLE: `is_openable` returned true above.
        let url = lesson.url.as_deref().unwrap().trim();
        if opener(url) {
            events.push(SchedulerEvent::LessonOpened {
                lesson_id: lesson.id,
                subject: lesson.subject.clone(),
            });
            Ok(true)
        } else {
            storage.set_occurrence_status(lesson.id, today, STATUS_MISSED)?;
            events.push(SchedulerEvent::LessonMissed {
                lesson_id: lesson.id,
                subject: lesson.subject.clone(),
            });
            Ok(false)
        }
    }

    fn claim_only(
        &self,
        storage: &Storage,
        lesson: &Lesson,
        today: NaiveDate,
        status: &str,
        events: &mut Vec<SchedulerEvent>,
    ) -> Result<(), StorageError> {
        if storage.claim_occurrence(lesson.id, today, status)? && status == STATUS_MISSED {
            events.push(SchedulerEvent::LessonMissed {
                lesson_id: lesson.id,
                subject: lesson.subject.clone(),
            });
        }
        Ok(())
    }

    pub fn open_now(
        &mut self,
        storage: &Storage,
        lesson_id: i64,
        day: NaiveDate,
        claim: bool,
    ) -> Result<bool, StorageError> {
        let Some(lesson) = storage.lesson(lesson_id)? else {
            return Ok(false);
        };
        if !is_openable(lesson.url.as_deref()) {
            return Ok(false);
        }
        if !claim {
            return Ok(open_url(lesson.url.as_deref()));
        }
        let _ = storage.claim_occurrence(lesson.id, day, STATUS_MANUAL)?;
        let opened = open_url(lesson.url.as_deref());
        if opened {
            storage.set_occurrence_status(lesson.id, day, STATUS_MANUAL)?;
            self.notified
                .insert((lesson.id, day.format("%Y-%m-%d").to_string()));
        }
        Ok(opened)
    }

    pub fn mark(
        &mut self,
        storage: &Storage,
        lesson_id: i64,
        status: &str,
        day: NaiveDate,
    ) -> Result<(), StorageError> {
        storage.mark_occurrence(lesson_id, day, status)?;
        self.notified
            .insert((lesson_id, day.format("%Y-%m-%d").to_string()));
        Ok(())
    }

    pub fn clear_mark(
        &mut self,
        storage: &Storage,
        lesson_id: i64,
        day: NaiveDate,
    ) -> Result<(), StorageError> {
        storage.clear_occurrence(lesson_id, day)?;
        self.notified
            .remove(&(lesson_id, day.format("%Y-%m-%d").to_string()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn sample_lesson(start: &str, end: &str) -> Lesson {
        Lesson {
            id: 1,
            course_id: 1,
            day_index: 0,
            pair: 1,
            start_time: start.into(),
            end_time: end.into(),
            subject: "Standup".into(),
            teacher: String::new(),
            url: Some("https://meet.google.com/abc-defg-hij".into()),
            provider: "google_meet".into(),
            needs_link: false,
            is_manual: true,
            source_key: String::new(),
            group_names: vec![],
            on_date: None,
            repeat_until: None,
            skip_dates: BTreeSet::new(),
        }
    }

    #[test]
    fn cold_start_never_ambushes() {
        let storage = Storage::open_in_memory().unwrap();
        let (course, group) = storage.ensure_personal_workspace().unwrap();
        storage.set_setting("catchup_mode", CATCHUP_OPEN).unwrap();
        let mut lesson = sample_lesson("09:00", "10:00");
        lesson.course_id = course.id;
        storage.add_lesson(&lesson, &[group.id]).unwrap();

        let mut engine = SchedulerEngine::new();
        let now = NaiveDate::from_ymd_opt(2026, 3, 2)
            .unwrap()
            .and_hms_opt(9, 15, 0)
            .unwrap();
        let mut opened_count = 0;
        let (_decisions, events) = engine
            .tick_with_opener(&storage, now, |_| {
                opened_count += 1;
                true
            })
            .unwrap();
        assert_eq!(opened_count, 0);
        assert!(matches!(events.first(), Some(SchedulerEvent::CatchupAvailable { .. })));
    }
}
