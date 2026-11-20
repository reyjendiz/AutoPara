use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::models::{LANG_EN, Lesson};

pub const COUNTDOWN_LIMIT_MINUTES: i64 = 90;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextUp {
    pub lesson_id: i64,
    pub subject: String,
    pub start_time: String,
    pub ongoing: bool,
    pub minutes: i64,
}

pub fn find(lessons: &[Lesson], now: NaiveDateTime, settled: &HashSet<i64>) -> Option<NextUp> {
    let today = now.date();
    let mut candidates: Vec<&Lesson> = lessons
        .iter()
        .filter(|l| {
            l.occurs_on(today)
                && !settled.contains(&l.id)
                && l.ends_on(today).is_some_and(|end| end > now)
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|a, b| a.start_time.cmp(&b.start_time));
    let lesson = candidates[0];
    let start = lesson.starts_on(today)?;
    let end = lesson.ends_on(today)?;
    if start <= now {
        let secs = (end - now).num_seconds() as f64;
        let mins = ((secs / 60.0).round() as i64).max(1);
        Some(NextUp {
            lesson_id: lesson.id,
            subject: lesson.subject.clone(),
            start_time: lesson.start_time.clone(),
            ongoing: true,
            minutes: mins,
        })
    } else {
        let secs = (start - now).num_seconds() as f64;
        let mins = ((secs / 60.0).round() as i64).max(1);
        Some(NextUp {
            lesson_id: lesson.id,
            subject: lesson.subject.clone(),
            start_time: lesson.start_time.clone(),
            ongoing: false,
            minutes: mins,
        })
    }
}

pub fn label(item: &NextUp, language: &str) -> String {
    if language == LANG_EN {
        if item.ongoing {
            return format!("Now · {} · {}m left", item.subject, item.minutes);
        }
        let when = if item.minutes <= COUNTDOWN_LIMIT_MINUTES {
            format!("in {}m", item.minutes)
        } else {
            format!("at {}", item.start_time)
        };
        return format!("Next · {} · {when}", item.subject);
    }

    if item.ongoing {
        return format!("Зараз · {} · ще {} хв", item.subject, item.minutes);
    }
    let when = if item.minutes <= COUNTDOWN_LIMIT_MINUTES {
        format!("через {} хв", item.minutes)
    } else {
        format!("о {}", item.start_time)
    };
    format!("Далі · {} · {when}", item.subject)
}
