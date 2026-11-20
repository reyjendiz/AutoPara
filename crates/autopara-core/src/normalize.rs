use chrono::{Datelike, Duration, NaiveDate};
use md5::{Digest, Md5};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;

pub const DAY_NAMES: [&str; 7] = [
    "Понеділок",
    "Вівторок",
    "Середа",
    "Четвер",
    "П'ятниця",
    "Субота",
    "Неділя",
];

pub const DAY_SHORT: [&str; 7] = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Нд"];

pub const TIME_TO_PAIR: [(&str, i64); 10] = [
    ("8.00", 1),
    ("9.30", 2),
    ("11.20", 3),
    ("13.00", 4),
    ("14.40", 5),
    ("16.10", 6),
    ("17.40", 7),
    ("19.10", 8),
    ("20.40", 9),
    ("22.10", 10),
];

pub const ACADEMIC_PAIRS: i64 = 6;
pub const DEFAULT_CLASS_MINUTES: i64 = 80;

const APOSTROPHES: [char; 6] = ['`', '\u{2019}', '\u{02BC}', '\u{02B9}', '\u{2018}', '\u{00B4}'];
const TRAILING_PUNCT: [char; 7] = ['.', ',', ';', ':', ')', ']', '}'];
const ROMAN: [&str; 8] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII"];
const COURSE_WORDS: [&str; 9] = [
    "курс", "kurs", "course", "year", "rok", "jahrgang", "año", "ano", "anno",
];

pub const SUBJECT_COLORS: [&str; 7] = [
    "#ec6b66", "#f1f36a", "#86cdf7", "#8566e6", "#8fe8a8", "#f26c9c", "#a8bdd6",
];

/// NFC-normalize, fold apostrophe variants, and collapse whitespace for comparison.
pub fn normalize_text(value: &str) -> String {
    let nfc: String = value
        .nfc()
        .map(|c| if APOSTROPHES.contains(&c) { '\'' } else { c })
        .collect();
    let mut out = String::with_capacity(nfc.len());
    let mut in_space = false;
    for ch in nfc.chars() {
        if ch.is_whitespace() {
            if !out.is_empty() && !in_space {
                out.push(' ');
                in_space = true;
            }
        } else {
            out.push(ch);
            in_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Case-fold helper matching Python's `.casefold()` for Latin/Cyrillic/Germanic text.
pub fn casefold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            'ß' | 'ẞ' => out.push_str("ss"),
            _ => {
                for lc in ch.to_lowercase() {
                    out.push(lc);
                }
            }
        }
    }
    out
}

struct DayAliases {
    map: HashMap<String, i64>,
    by_length: Vec<String>,
}

fn day_aliases() -> &'static DayAliases {
    static ALIASES: OnceLock<DayAliases> = OnceLock::new();
    ALIASES.get_or_init(|| {
        let mut map = HashMap::new();
        let groups: [(i64, &[&str]); 7] = [
            (
                0,
                &[
                    "понеділок",
                    "понеділка",
                    "пн",
                    "понедельник",
                    "monday",
                    "mon",
                    "poniedzialek",
                    "poniedziałek",
                    "montag",
                    "lunes",
                    "lundi",
                    "lunedi",
                    "lunedì",
                    "luni",
                ],
            ),
            (
                1,
                &[
                    "вівторок", "вт", "вторник", "tuesday", "tue", "tues", "wtorek", "dienstag",
                    "martes", "mardi", "martedi", "martedì", "marti",
                ],
            ),
            (
                2,
                &[
                    "середа",
                    "середи",
                    "ср",
                    "среда",
                    "wednesday",
                    "wed",
                    "sroda",
                    "środa",
                    "mittwoch",
                    "miercoles",
                    "miércoles",
                    "mercredi",
                    "mercoledi",
                    "mercoledì",
                ],
            ),
            (
                3,
                &[
                    "четвер",
                    "чт",
                    "четверг",
                    "thursday",
                    "thu",
                    "thur",
                    "thurs",
                    "czwartek",
                    "donnerstag",
                    "jueves",
                    "jeudi",
                    "giovedi",
                    "giovedì",
                    "joi",
                ],
            ),
            (
                4,
                &[
                    "п'ятниця",
                    "п'ятниці",
                    "пт",
                    "пятница",
                    "friday",
                    "fri",
                    "piatek",
                    "piątek",
                    "freitag",
                    "viernes",
                    "vendredi",
                    "venerdi",
                    "venerdì",
                    "vineri",
                ],
            ),
            (
                5,
                &[
                    "субота",
                    "сб",
                    "суббота",
                    "saturday",
                    "sat",
                    "sobota",
                    "samstag",
                    "sonnabend",
                    "sabado",
                    "sábado",
                    "samedi",
                    "sabato",
                    "sambata",
                    "sâmbătă",
                ],
            ),
            (
                6,
                &[
                    "неділя",
                    "нд",
                    "вс",
                    "воскресенье",
                    "sunday",
                    "sun",
                    "niedziela",
                    "sonntag",
                    "domingo",
                    "dimanche",
                    "domenica",
                    "duminica",
                    "duminică",
                ],
            ),
        ];
        for (idx, names) in groups {
            for name in names {
                map.insert(casefold(&normalize_text(name)), idx);
            }
        }
        let mut by_length: Vec<String> = map.keys().cloned().collect();
        by_length.sort_by(|a, b| {
            b.chars()
                .count()
                .cmp(&a.chars().count())
                .then_with(|| a.cmp(b))
        });
        DayAliases { map, by_length }
    })
}

fn day_key(value: &str) -> String {
    let strip_chars: &[char] = &[
        ' ', '.', ',', ':', ';', '-', '\u{2013}', '\u{2014}', '(', ')', '[', ']',
    ];
    casefold(&normalize_text(value))
        .trim_matches(strip_chars)
        .to_string()
}

pub fn day_index(value: &str) -> Option<i64> {
    let needle = day_key(value);
    if needle.is_empty() {
        return None;
    }
    let aliases = day_aliases();
    if let Some(&idx) = aliases.map.get(&needle) {
        return Some(idx);
    }
    for alias in &aliases.by_length {
        if alias.chars().count() >= 3 && needle.starts_with(alias.as_str()) {
            return aliases.map.get(alias).copied();
        }
    }
    None
}

pub fn day_name(index: i64) -> &'static str {
    DAY_NAMES[index.rem_euclid(7) as usize]
}

pub fn week_start(day: NaiveDate) -> NaiveDate {
    day - Duration::days(i64::from(day.weekday().num_days_from_monday()))
}

pub fn parse_time(value: &str) -> Option<String> {
    let text = normalize_text(value);
    let sep = text.find(['.', ':'])?;
    let (h_str, rest) = text.split_at(sep);
    let m_str = &rest[1..];
    if h_str.is_empty()
        || h_str.len() > 2
        || m_str.len() != 2
        || !h_str.bytes().all(|b| b.is_ascii_digit())
        || !m_str.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let hour: u32 = h_str.parse().ok()?;
    let minute: u32 = m_str.parse().ok()?;
    if hour < 24 && minute < 60 {
        Some(format!("{hour:02}:{minute:02}"))
    } else {
        None
    }
}

pub fn parse_time_range(value: &str) -> (Option<String>, Option<String>) {
    let text = normalize_text(value);
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        // Match (\d{1,2})\s*[.:]\s*(\d{2}) — greedily take 2 digits for hour if valid, else 1.
        let mut h_end = i + 1;
        if h_end < bytes.len() && bytes[h_end].is_ascii_digit() {
            h_end += 1;
        }
        let mut j = h_end;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < bytes.len() && (bytes[j] == b'.' || bytes[j] == b':') {
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j + 2 <= bytes.len()
                && bytes[j].is_ascii_digit()
                && bytes[j + 1].is_ascii_digit()
            {
                let h: u32 = text[i..h_end].parse().unwrap_or(99);
                let m: u32 = text[j..j + 2].parse().unwrap_or(99);
                if h < 24 && m < 60 {
                    found.push(format!("{h:02}:{m:02}"));
                }
                i = j + 2;
                continue;
            }
        }
        i += 1;
    }
    let start = found.first().cloned();
    let end = match (found.first(), found.get(1)) {
        (Some(s), Some(e)) if e > s => Some(e.clone()),
        _ => None,
    };
    (start, end)
}

pub fn pair_from_time(value: &str) -> Option<i64> {
    let text = normalize_text(value).replace(':', ".");
    if let Some((_, p)) = TIME_TO_PAIR.iter().find(|(t, _)| *t == text) {
        return Some(*p);
    }
    if let Some(canonical) = parse_time(value) {
        let mut parts = canonical.split(':');
        let hour: u32 = parts.next()?.parse().ok()?;
        let minute = parts.next()?;
        let key = format!("{hour}.{minute}");
        return TIME_TO_PAIR
            .iter()
            .find(|(t, _)| *t == key)
            .map(|(_, p)| *p);
    }
    None
}

pub fn pair_start_time(pair: i64) -> String {
    if let Some((raw, _)) = TIME_TO_PAIR.iter().find(|(_, p)| *p == pair) {
        let mut parts = raw.split('.');
        if let (Some(h), Some(m)) = (parts.next(), parts.next())
            && let Ok(hour) = h.parse::<u32>()
        {
            return format!("{hour:02}:{m}");
        }
    }
    String::new()
}

pub fn pair_slot(hhmm: &str) -> i64 {
    if let Some(exact) = pair_from_time(hhmm) {
        return exact;
    }
    let mut slot = 1;
    for (_, pair) in TIME_TO_PAIR {
        if pair_start_time(pair).as_str() <= hhmm {
            slot = pair;
        }
    }
    slot
}

pub fn last_pair_covered(start_time: &str, end_time: &str) -> i64 {
    let mut last = pair_slot(start_time);
    for (_, pair) in TIME_TO_PAIR {
        let slot = pair_start_time(pair);
        if start_time <= slot.as_str() && slot.as_str() < end_time {
            last = last.max(pair);
        }
    }
    last
}

pub fn add_minutes(hhmm: &str, minutes: i64) -> String {
    let (hour, minute) = split_hhmm(hhmm);
    let total = (hour * 60 + minute + minutes).clamp(0, 24 * 60 - 1);
    format!("{:02}:{:02}", total / 60, total % 60)
}

pub fn minutes_between(start: &str, end: &str) -> i64 {
    let (sh, sm) = split_hhmm(start);
    let (eh, em) = split_hhmm(end);
    ((eh * 60 + em) - (sh * 60 + sm)).max(0)
}

fn split_hhmm(value: &str) -> (i64, i64) {
    let mut parts = value.split(':');
    let h = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let m = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (h, m)
}

pub fn clean_url(value: &str) -> String {
    let mut url = value.trim().to_string();
    while let Some(last) = url.chars().last() {
        if TRAILING_PUNCT.contains(&last) {
            url.pop();
        } else {
            break;
        }
    }
    url
}

pub fn find_urls(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut rest = text;
    while let Some(pos) = find_http_start(rest) {
        let slice = &rest[pos..];
        let end = slice
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\''))
            .unwrap_or(slice.len());
        let raw = &slice[..end];
        if raw.len() > "http://".len() {
            result.push(clean_url(raw));
        }
        rest = &slice[end..];
    }
    result
}

fn find_http_start(s: &str) -> Option<usize> {
    let a = s.find("https://");
    let b = s.find("http://");
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        (None, None) => None,
    }
}

pub fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for value in values {
        if !value.is_empty() && seen.insert(value.clone()) {
            result.push(value);
        }
    }
    result
}

/// Core DB provider classification — matches original Python `detect_provider` 1:1.
pub fn detect_provider(url: Option<&str>) -> String {
    let Some(u) = url else {
        return "unknown".to_string();
    };
    if u.is_empty() {
        return "unknown".to_string();
    }
    let lowered = u.to_lowercase();
    if lowered.contains("meet.google.com") {
        return "google_meet".to_string();
    }
    if lowered.contains("zoom.us") || lowered.contains("zoom.com") {
        return "zoom".to_string();
    }
    "unknown".to_string()
}

pub fn is_teacher_line(line: &str) -> bool {
    let text = normalize_text(line);
    text.starts_with('(') && text.ends_with(')')
}

pub fn strip_parens(line: &str) -> String {
    let text = normalize_text(line);
    if text.starts_with('(') && text.ends_with(')') && text.len() >= 2 {
        text[1..text.len() - 1].trim().to_string()
    } else {
        text
    }
}

pub fn is_course_heading(text: &str) -> bool {
    let folded = casefold(&normalize_text(text));
    folded.split_whitespace().any(|word| {
        let trimmed = word.trim_matches(['.', ',', ':', ';']);
        COURSE_WORDS.contains(&trimmed)
    })
}

pub fn course_label(ordinal: i64) -> String {
    if ordinal >= 1 && (ordinal as usize) <= ROMAN.len() {
        format!("{} курс", ROMAN[(ordinal - 1) as usize])
    } else {
        format!("{ordinal} курс")
    }
}

/// Deterministic subject color matching Python's `hashlib.md5(subject.strip().casefold()).hexdigest()[:8]`.
pub fn subject_color(subject: &str) -> &'static str {
    let key = casefold(subject.trim());
    let digest = Md5::digest(key.as_bytes());
    let num = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) as usize;
    SUBJECT_COLORS[num % SUBJECT_COLORS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_names_multilingual_and_backtick() {
        assert_eq!(day_index("П`ятниця"), Some(4));
        assert_eq!(day_index("Понеділок 02.09"), Some(0));
        assert_eq!(day_index("Wednesday"), Some(2));
        assert_eq!(day_index("środa"), Some(2));
        assert_eq!(day_index("so"), None);
    }

    #[test]
    fn time_ranges_and_slots() {
        assert_eq!(
            parse_time_range("8.00-9.20"),
            (Some("08:00".into()), Some("09:20".into()))
        );
        assert_eq!(parse_time_range("9.30"), (Some("09:30".into()), None));
        assert_eq!(pair_from_time("14.40"), Some(5));
        assert_eq!(last_pair_covered("09:30", "12:40"), 3);
    }

    #[test]
    fn provider_and_subject_color() {
        assert_eq!(detect_provider(Some("https://meet.google.com/a-b-c")), "google_meet");
        assert_eq!(detect_provider(Some("https://us02web.zoom.us/j/123")), "zoom");
        assert_eq!(detect_provider(Some("https://teams.microsoft.com/l/meetup")), "unknown");
        assert!(SUBJECT_COLORS.contains(&subject_color("Психологія")));
    }
}
