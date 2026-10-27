//! Times of day and the university's pair slots (PORT_SPEC §A.4).

use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::text::normalize;

const MINUTES_PER_DAY: u16 = 24 * 60;

/// A wall-clock time with minute precision, `00:00`..=`23:59`. Ordered chronologically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeOfDay(u16);

impl TimeOfDay {
    pub const fn from_hm(hour: u8, minute: u8) -> Option<Self> {
        if hour < 24 && minute < 60 {
            Some(Self(hour as u16 * 60 + minute as u16))
        } else {
            None
        }
    }

    pub const fn minutes(self) -> u16 {
        self.0
    }

    pub const fn hour(self) -> u8 {
        (self.0 / 60) as u8
    }

    pub const fn minute(self) -> u8 {
        (self.0 % 60) as u8
    }

    /// `self + minutes`, clamped at `23:59` (the Python `add_minutes`).
    pub fn add_minutes(self, minutes: u32) -> Self {
        let total = (u32::from(self.0) + minutes).min(u32::from(MINUTES_PER_DAY - 1));
        Self(total as u16)
    }

    /// Minutes from `self` to `end`, never negative.
    pub fn minutes_until(self, end: Self) -> u16 {
        end.0.saturating_sub(self.0)
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour(), self.minute())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a time of day: {0:?}")]
pub struct BadTime(pub String);

impl FromStr for TimeOfDay {
    type Err = BadTime;

    /// `H:MM`, `HH:MM`, `H.MM` or `HH.MM` — the whole string (Python `parse_time`).
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let text = normalize(value);
        let bad = || BadTime(value.to_owned());
        let (hour, minute) = text.split_once([':', '.']).ok_or_else(bad)?;
        let digits = |s: &str, max_len| {
            (!s.is_empty() && s.len() <= max_len && s.bytes().all(|b| b.is_ascii_digit()))
                .then(|| s.parse::<u8>().ok())
                .flatten()
        };
        match (
            digits(hour, 2),
            (minute.len() == 2).then(|| digits(minute, 2)).flatten(),
        ) {
            (Some(h), Some(m)) => Self::from_hm(h, m).ok_or_else(bad),
            _ => Err(bad()),
        }
    }
}

impl Serialize for TimeOfDay {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for TimeOfDay {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// The university's slots: pairs 1–6 are real, 7–10 continue the 80 + 10 minute rhythm so a class
/// can sit anywhere in the evening (`normalize.TIME_TO_PAIR`).
pub const PAIR_STARTS: [(u8, TimeOfDay); 10] = [
    (1, hm(8, 0)),
    (2, hm(9, 30)),
    (3, hm(11, 20)),
    (4, hm(13, 0)),
    (5, hm(14, 40)),
    (6, hm(16, 10)),
    (7, hm(17, 40)),
    (8, hm(19, 10)),
    (9, hm(20, 40)),
    (10, hm(22, 10)),
];

/// How many pairs a parsed lesson is expected to fall inside.
pub const ACADEMIC_PAIRS: u8 = 6;

/// The class length assumed when a document gives only a start time.
pub const DEFAULT_CLASS_MINUTES: u32 = 80;

const fn hm(hour: u8, minute: u8) -> TimeOfDay {
    TimeOfDay(hour as u16 * 60 + minute as u16)
}

/// The pair that starts exactly at `time`, if any.
pub fn pair_at(time: TimeOfDay) -> Option<u8> {
    PAIR_STARTS
        .iter()
        .find(|(_, start)| *start == time)
        .map(|(pair, _)| *pair)
}

/// The start of `pair`, if it is one of the slots.
pub fn pair_start(pair: u8) -> Option<TimeOfDay> {
    PAIR_STARTS
        .iter()
        .find(|(p, _)| *p == pair)
        .map(|(_, start)| *start)
}

/// The grid row any time belongs in: its exact slot, else the last slot already begun, else 1.
pub fn pair_slot(time: TimeOfDay) -> u8 {
    pair_at(time).unwrap_or_else(|| {
        PAIR_STARTS
            .iter()
            .filter(|(_, start)| *start <= time)
            .map(|(pair, _)| *pair)
            .next_back()
            .unwrap_or(1)
    })
}

/// The highest pair that begins inside `[start, end)` — what a block of several slots covers.
pub fn last_pair_covered(start: TimeOfDay, end: TimeOfDay) -> u8 {
    PAIR_STARTS
        .iter()
        .filter(|(_, slot)| start <= *slot && *slot < end)
        .map(|(pair, _)| *pair)
        .fold(pair_slot(start), u8::max)
}

static CLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([0-9]{1,2})[\s\x1c-\x1f]*[.:][\s\x1c-\x1f]*([0-9]{2})")
        .expect("the clock pattern is valid")
});

/// A start and an optional end from a `Час` cell: `8.00`, `8.00-9.20`, `8:00 – 9:20` (R11).
///
/// Every `H.MM` in the text is read; the first valid one is the start, and the second is the end
/// only when it is later than the start.
pub fn parse_time_range(value: &str) -> (Option<TimeOfDay>, Option<TimeOfDay>) {
    let text = normalize(value);
    let found: Vec<TimeOfDay> = CLOCK
        .captures_iter(&text)
        .filter_map(|caps| {
            let hour = caps[1].parse::<u8>().ok()?;
            let minute = caps[2].parse::<u8>().ok()?;
            TimeOfDay::from_hm(hour, minute)
        })
        .collect();
    let start = found.first().copied();
    let end = match (start, found.get(1)) {
        (Some(start), Some(&end)) if end > start => Some(end),
        _ => None,
    };
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(text: &str) -> TimeOfDay {
        text.parse().unwrap()
    }

    #[test]
    fn parses_and_prints() {
        assert_eq!(t("8.00").to_string(), "08:00");
        assert_eq!(t("23:59").to_string(), "23:59");
        assert!("24:00".parse::<TimeOfDay>().is_err());
        assert!("8.0".parse::<TimeOfDay>().is_err());
        assert!("8-00".parse::<TimeOfDay>().is_err());
    }

    #[test]
    fn ranges() {
        assert_eq!(
            parse_time_range("8.00-9.20"),
            (Some(t("08:00")), Some(t("09:20")))
        );
        assert_eq!(
            parse_time_range("8:00 – 9:20"),
            (Some(t("08:00")), Some(t("09:20")))
        );
        assert_eq!(parse_time_range("8.00"), (Some(t("08:00")), None));
        assert_eq!(parse_time_range("немає"), (None, None));
        assert_eq!(parse_time_range("9.20-8.00"), (Some(t("09:20")), None));
    }

    #[test]
    fn slots() {
        assert_eq!(pair_slot(t("08:00")), 1);
        assert_eq!(pair_slot(t("07:15")), 1);
        assert_eq!(pair_slot(t("12:00")), 3);
        assert_eq!(pair_slot(t("23:00")), 10);
        assert_eq!(last_pair_covered(t("08:00"), t("09:20")), 1);
        assert_eq!(last_pair_covered(t("08:00"), t("12:40")), 3);
    }

    #[test]
    fn adding_clamps_at_midnight() {
        assert_eq!(t("23:00").add_minutes(80).to_string(), "23:59");
        assert_eq!(t("09:30").add_minutes(80).to_string(), "10:50");
    }
}
