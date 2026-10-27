//! Text normalisation for timetable documents (PORT_SPEC §A.2, §A.6).
//!
//! The source documents are inconsistent in ways that break naive matching: `П`ятниця` with a
//! backtick, Cyrillic `І` beside Latin `V`, decomposed (NFD) letters. Everything is compared after
//! [`normalize`] and, where case does not matter, [`fold`].

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::model::Provider;

const APOSTROPHES: [char; 6] = ['`', '’', 'ʼ', 'ʹ', '‘', '´'];
const URL_TRAILING_PUNCTUATION: &[char] = &['.', ',', ';', ':', ')', ']', '}'];

/// Python's `str.isspace`: Unicode white space plus the four ASCII separators 0x1C–0x1F, which
/// Rust's `char::is_whitespace` leaves out. The original code splits and strips with Python's
/// rules, so this one does too.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// NFC, apostrophe variants folded to `'`, runs of white space collapsed to one space, trimmed.
pub fn normalize(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut pending_space = false;
    for c in value.nfc() {
        let c = if APOSTROPHES.contains(&c) { '\'' } else { c };
        if is_space(c) {
            pending_space = true;
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        pending_space = false;
        out.push(c);
    }
    out
}

/// Full Unicode case folding, as Python's `str.casefold` does it.
pub fn fold(value: &str) -> String {
    caseless::default_case_fold_str(value)
}

/// `normalize` then `fold`: the comparison key for subjects, teachers and day names.
pub fn key(value: &str) -> String {
    fold(&normalize(value))
}

/// `strip()` with Python's notion of white space.
pub fn strip_space(value: &str) -> &str {
    value.trim_matches(is_space)
}

static URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"https?://[^\s\x1c-\x1f<>"']+"#).expect("the URL pattern is valid")
});

/// A URL written inline in prose loses the punctuation that follows it.
pub fn clean_url(value: &str) -> &str {
    strip_space(value).trim_end_matches(URL_TRAILING_PUNCTUATION)
}

/// Every `http(s)://` URL in `text`, cleaned, in order.
pub fn find_urls(text: &str) -> Vec<&str> {
    URL.find_iter(text).map(|m| clean_url(m.as_str())).collect()
}

/// Order-preserving dedupe that drops empty values (every link appears twice, R3).
pub fn dedupe<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    for value in values {
        if !value.is_empty() && !out.contains(&value) {
            out.push(value);
        }
    }
    out
}

/// Which meeting service a link belongs to.
pub fn detect_provider(url: Option<&str>) -> Provider {
    let Some(url) = url.filter(|url| !url.is_empty()) else {
        return Provider::Unknown;
    };
    let lowered = url.to_lowercase();
    if lowered.contains("meet.google.com") {
        Provider::GoogleMeet
    } else if lowered.contains("zoom.us") || lowered.contains("zoom.com") {
        Provider::Zoom
    } else {
        Provider::Unknown
    }
}

/// A teacher is written as a parenthesised line inside the cell (R9).
pub fn is_teacher_line(line: &str) -> bool {
    let text = normalize(line);
    text.starts_with('(') && text.ends_with(')')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_folds_apostrophes_and_space() {
        assert_eq!(normalize("  П`ятниця \t\u{1c} x "), "П'ятниця x");
        assert_eq!(normalize("П’ятниця"), "П'ятниця");
    }

    #[test]
    fn normalize_composes() {
        let decomposed: String = "й".nfd().collect();
        assert_eq!(normalize(&decomposed), "й");
    }

    #[test]
    fn fold_is_full_case_folding() {
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(key(" ПОНЕДІЛОК "), "понеділок");
    }

    #[test]
    fn urls_are_found_and_cleaned() {
        assert_eq!(
            find_urls("see https://meet.google.com/a-b-c."),
            ["https://meet.google.com/a-b-c"]
        );
        assert_eq!(
            find_urls("(https://zoom.us/j/1), http://x.y"),
            ["https://zoom.us/j/1", "http://x.y"]
        );
        assert!(find_urls("ftp://nope").is_empty());
    }

    #[test]
    fn dedupe_keeps_order() {
        assert_eq!(dedupe(["b", "a", "b", ""]), ["b", "a"]);
    }

    #[test]
    fn providers() {
        assert_eq!(
            detect_provider(Some("https://MEET.google.com/x")),
            Provider::GoogleMeet
        );
        assert_eq!(
            detect_provider(Some("https://us02web.zoom.us/j/1")),
            Provider::Zoom
        );
        assert_eq!(
            detect_provider(Some("https://example.org")),
            Provider::Unknown
        );
        assert_eq!(detect_provider(None), Provider::Unknown);
    }

    #[test]
    fn teacher_lines() {
        assert!(is_teacher_line(" (Роман Н.М.) "));
        assert!(!is_teacher_line("Роман (Н.М.)"));
    }
}
