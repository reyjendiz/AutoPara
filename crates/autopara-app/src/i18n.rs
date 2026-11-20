use autopara_core::models::LANG_EN;
use chrono::{Datelike, NaiveDate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Uk,
    En,
}

impl Lang {
    pub fn from_code(code: &str) -> Self {
        if code == LANG_EN { Self::En } else { Self::Uk }
    }

    pub fn is_en(self) -> bool {
        self == Self::En
    }
}

pub const DAYS_UK: [&str; 7] = [
    "Понеділок",
    "Вівторок",
    "Середа",
    "Четвер",
    "П'ятниця",
    "Субота",
    "Неділя",
];
pub const DAYS_EN: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

pub const MONTHS_GEN_UK: [&str; 12] = [
    "січня",
    "лютого",
    "березня",
    "квітня",
    "травня",
    "червня",
    "липня",
    "серпня",
    "вересня",
    "жовтня",
    "листопада",
    "грудня",
];

pub const MONTHS_SHORT_EN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub fn days_full(lang: Lang) -> &'static [&'static str; 7] {
    if lang.is_en() { &DAYS_EN } else { &DAYS_UK }
}

pub fn format_week_title(monday: NaiveDate, sunday: NaiveDate, lang: Lang) -> (String, String) {
    let m1 = (monday.month() - 1) as usize;
    let m2 = (sunday.month() - 1) as usize;
    let year = sunday.year().to_string();
    let main = if lang.is_en() {
        if m1 == m2 {
            format!("{} {}–{}", MONTHS_SHORT_EN[m1], monday.day(), sunday.day())
        } else {
            format!(
                "{} {} – {} {}",
                MONTHS_SHORT_EN[m1],
                monday.day(),
                MONTHS_SHORT_EN[m2],
                sunday.day()
            )
        }
    } else if m1 == m2 {
        format!("{}–{} {}", monday.day(), sunday.day(), MONTHS_GEN_UK[m1])
    } else {
        format!(
            "{} {} – {} {}",
            monday.day(),
            MONTHS_GEN_UK[m1],
            sunday.day(),
            MONTHS_GEN_UK[m2]
        )
    };
    (main, year)
}

pub fn format_short_date(day: NaiveDate, lang: Lang) -> String {
    let m = (day.month() - 1) as usize;
    if lang.is_en() {
        format!("{} {}, {}", MONTHS_SHORT_EN[m], day.day(), day.year())
    } else {
        format!("{} {} {}", day.day(), MONTHS_GEN_UK[m], day.year())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Zoom,
    Meet,
    Teams,
    Discord,
    Link,
}

pub fn detect_display_provider(url: &str, lang: Lang) -> (ProviderKind, &'static str, &'static str) {
    let u = url.to_ascii_lowercase();
    if u.contains("meet.google.com") {
        (ProviderKind::Meet, "Meet", "Google Meet")
    } else if u.contains("zoom.us") || u.contains("zoom.com") {
        (ProviderKind::Zoom, "Zoom", "Zoom")
    } else if u.contains("teams.microsoft.com") || u.contains("teams.live.com") {
        (ProviderKind::Teams, "Teams", "Microsoft Teams")
    } else if u.contains("discord.gg") || u.contains("discord.com") {
        (ProviderKind::Discord, "Discord", "Discord")
    } else if u.contains("meet.jit.si") {
        (ProviderKind::Meet, "Jitsi", "Jitsi Meet")
    } else if u.contains("webex.com") {
        (ProviderKind::Zoom, "Webex", "Cisco Webex")
    } else if u.contains("t.me/") {
        (ProviderKind::Zoom, "Telegram", "Telegram")
    } else if lang.is_en() {
        (ProviderKind::Link, "Link", "Link")
    } else {
        (ProviderKind::Link, "Посилання", "Посилання")
    }
}

pub struct Tr {
    pub lang: Lang,
}

impl Tr {
    pub fn new(lang: Lang) -> Self {
        Self { lang }
    }

    pub fn pick(&self, uk: &'static str, en: &'static str) -> &'static str {
        if self.lang.is_en() { en } else { uk }
    }

    pub fn duration_label(&self, mins: i64) -> String {
        let h = mins / 60;
        let m = mins % 60;
        if self.lang.is_en() {
            if h > 0 && m > 0 {
                format!("Duration: {h}h {m}m")
            } else if h > 0 {
                format!("Duration: {h}h")
            } else {
                format!("Duration: {m}m")
            }
        } else if h > 0 && m > 0 {
            format!("Тривалість: {h} год {m} хв")
        } else if h > 0 {
            format!("Тривалість: {h} год")
        } else {
            format!("Тривалість: {m} хв")
        }
    }

    pub fn found_count(&self, n: usize) -> String {
        if self.lang.is_en() {
            format!("Found: {n}")
        } else {
            format!("Знайдено: {n}")
        }
    }

    pub fn groups_badge(&self, n: usize) -> String {
        if self.lang.is_en() {
            format!("{n} groups")
        } else {
            format!("{n} групи")
        }
    }

    pub fn items_count(&self, n: usize) -> String {
        if self.lang.is_en() {
            if n == 0 {
                "empty".into()
            } else {
                format!("{n} events")
            }
        } else if n == 0 {
            "порожньо".into()
        } else {
            format!("{n} подій")
        }
    }

    pub fn queue_more(&self, n: usize) -> String {
        if self.lang.is_en() {
            format!("{n} more queued")
        } else {
            format!("ще {n} у черзі")
        }
    }

    pub fn create_at(&self, day: NaiveDate, start: &str) -> String {
        let d = format_short_date(day, self.lang);
        if self.lang.is_en() {
            format!("+ Add call · {d}, {start}")
        } else {
            format!("+ Додати созвон · {d}, {start}")
        }
    }
}
