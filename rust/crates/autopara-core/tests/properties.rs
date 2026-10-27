//! Property tests for the two hand-written recognisers: time ranges and day names.

use autopara_core::days::recognise;
use autopara_core::model::Weekday;
use autopara_core::time::{TimeOfDay, parse_time_range};
use proptest::prelude::*;

fn time() -> impl Strategy<Value = (u8, u8)> {
    (0u8..24, 0u8..60)
}

fn separator() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("-"),
        Just(" - "),
        Just("–"),
        Just(" — "),
        Just(" до "),
        Just("/")
    ]
}

proptest! {
    /// Any `H.MM` or `H:MM` start comes back as that time, and an end only when it is later.
    #[test]
    fn a_range_yields_its_start_and_a_later_end(
        (h1, m1) in time(),
        (h2, m2) in time(),
        dot in any::<bool>(),
        sep in separator(),
    ) {
        let mark = if dot { "." } else { ":" };
        let text = format!("{h1}{mark}{m1:02}{sep}{h2}{mark}{m2:02}");
        let start = TimeOfDay::from_hm(h1, m1).unwrap();
        let end = TimeOfDay::from_hm(h2, m2).unwrap();
        let (got_start, got_end) = parse_time_range(&text);
        prop_assert_eq!(got_start, Some(start));
        prop_assert_eq!(got_end, (end > start).then_some(end));
    }

    /// A bare start never invents an end, whatever surrounds it.
    #[test]
    fn a_bare_start_has_no_end((h, m) in time(), prefix in "[a-zа-я ]{0,6}", suffix in "[a-zа-я ]{0,6}") {
        let text = format!("{prefix}{h}.{m:02}{suffix}");
        prop_assert_eq!(parse_time_range(&text), (TimeOfDay::from_hm(h, m), None));
    }

    /// Text without digits is never a time.
    #[test]
    fn no_digits_no_time(text in "[^0-9]{0,30}") {
        prop_assert_eq!(parse_time_range(&text), (None, None));
    }

    /// Times print as zero-padded `HH:MM` and parse back to themselves.
    #[test]
    fn times_round_trip((h, m) in time()) {
        let time = TimeOfDay::from_hm(h, m).unwrap();
        prop_assert_eq!(time.to_string().parse::<TimeOfDay>().unwrap(), time);
    }

    /// Case, surrounding punctuation and a trailing date never change which day is recognised.
    #[test]
    fn day_names_survive_case_and_decoration(
        index in 0usize..7,
        upper in any::<bool>(),
        decoration in prop_oneof![Just(""), Just("."), Just(","), Just(":"), Just(" -")],
        date in prop_oneof![Just(""), Just(" 02.09"), Just(" 14 жовтня")],
    ) {
        let names = ["Понеділок", "Вівторок", "Середа", "Четвер", "П'ятниця", "Субота", "Неділя"];
        let name = if upper { names[index].to_uppercase() } else { names[index].to_lowercase() };
        let text = format!("({name}{decoration}{date})");
        prop_assert_eq!(recognise(&text), Weekday::from_index(index as u8));
    }

    /// Recognition never panics, whatever the cell holds.
    #[test]
    fn recognition_is_total(text in "\\PC{0,40}") {
        let _ = recognise(&text);
    }
}
