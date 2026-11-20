use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::models::Lesson;
use crate::normalize::{casefold, normalize_text};

pub const SEARCH_HORIZON_DAYS: i64 = 400;

const ALPHABET: &str = "абвгґдеєжзиіїйклмнопрстуфхцчшщьюяabcdefghijklmnopqrstuvwxyz";

fn alphabet_order() -> &'static HashMap<char, usize> {
    static ORDER: OnceLock<HashMap<char, usize>> = OnceLock::new();
    ORDER.get_or_init(|| {
        ALPHABET
            .chars()
            .enumerate()
            .map(|(i, c)| (c, i))
            .collect()
    })
}

fn fold(text: &str) -> String {
    casefold(&normalize_text(text))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LessonFilter {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub subject: String,
}

impl LessonFilter {
    pub fn active(&self) -> bool {
        !self.text.trim().is_empty() || !self.subject.is_empty()
    }

    pub fn matches(&self, lesson: &Lesson) -> bool {
        if !self.subject.is_empty() && fold(&lesson.subject) != fold(&self.subject) {
            return false;
        }
        let folded_text = fold(&self.text);
        let words: Vec<&str> = folded_text.split_whitespace().collect();
        if words.is_empty() {
            return true;
        }
        let haystack = format!("{} {}", fold(&lesson.subject), fold(&lesson.teacher));
        words.iter().all(|w| haystack.contains(w))
    }
}

pub fn apply(lessons: &[Lesson], flt: &LessonFilter) -> Vec<Lesson> {
    if !flt.active() {
        return lessons.to_vec();
    }
    lessons
        .iter()
        .filter(|l| flt.matches(l))
        .cloned()
        .collect()
}

pub fn alphabetical(text: &str) -> Vec<usize> {
    let order = alphabet_order();
    let base = ALPHABET.chars().count();
    fold(text)
        .chars()
        .map(|c| order.get(&c).copied().unwrap_or(base + (c as usize)))
        .collect()
}

pub fn subjects(lessons: &[Lesson]) -> Vec<String> {
    let mut seen: HashMap<String, String> = HashMap::new();
    for lesson in lessons {
        seen.entry(fold(&lesson.subject))
            .or_insert_with(|| normalize_text(&lesson.subject));
    }
    let mut list: Vec<String> = seen.into_values().collect();
    list.sort_by_key(|s| alphabetical(s));
    list
}

pub fn find_match(
    lessons: &[Lesson],
    flt: &LessonFilter,
    start: NaiveDate,
    direction: i64,
) -> Option<NaiveDate> {
    let candidates = apply(lessons, flt);
    if candidates.is_empty() {
        return None;
    }
    let step = Duration::days(if direction >= 0 { 1 } else { -1 });
    let mut day = start + step;
    for _ in 0..SEARCH_HORIZON_DAYS {
        if candidates.iter().any(|l| l.occurs_on(day)) {
            return Some(day);
        }
        day += step;
    }
    None
}
