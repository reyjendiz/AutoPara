//! Day names in any supported language (PORT_SPEC §A.3, rules R4, R8, R10).
//!
//! Matching is table-driven: the cell's first line is normalised, case-folded and stripped of
//! punctuation, then compared with every alias exactly, and failing that by the longest alias of
//! at least three characters it starts with ("Понеділок 02.09", "Monday, 2 March").

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::model::Weekday;
use crate::text::{is_space, key};

/// Ukrainian, Russian, English, Polish, German, Spanish, French, Italian, Romanian. Two-letter
/// abbreviations shared between languages (German `so`, Polish `sob`) are deliberately absent.
const ALIASES: [(Weekday, &[&str]); 7] = [
    (
        Weekday::Monday,
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
        Weekday::Tuesday,
        &[
            "вівторок",
            "вт",
            "вторник",
            "tuesday",
            "tue",
            "tues",
            "wtorek",
            "dienstag",
            "martes",
            "mardi",
            "martedi",
            "martedì",
            "marti",
        ],
    ),
    (
        Weekday::Wednesday,
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
        Weekday::Thursday,
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
        Weekday::Friday,
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
        Weekday::Saturday,
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
        Weekday::Sunday,
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

/// Characters stripped from both ends of the cell text before matching.
const STRIP: &[char] = &[' ', '.', ',', ':', ';', '-', '–', '—', '(', ')', '[', ']'];

struct Table {
    exact: HashMap<String, Weekday>,
    /// Aliases of three characters or more, longest first, for prefix matching.
    by_length: Vec<(String, Weekday)>,
}

static TABLE: LazyLock<Table> = LazyLock::new(|| {
    let mut exact = HashMap::new();
    let mut ordered = Vec::new();
    for (day, names) in ALIASES {
        for name in names {
            let alias = key(name);
            if exact.insert(alias.clone(), day).is_none() {
                ordered.push((alias, day));
            }
        }
    }
    // A stable sort keeps the table's order between aliases of the same length, as Python's does.
    ordered.sort_by_key(|(alias, _)| std::cmp::Reverse(alias.chars().count()));
    ordered.retain(|(alias, _)| alias.chars().count() >= 3);
    Table {
        exact,
        by_length: ordered,
    }
});

/// The weekday a day cell names, in any supported language; `None` when it names none.
pub fn recognise(cell_text: &str) -> Option<Weekday> {
    let folded = key(cell_text);
    let needle = folded.trim_matches(|c: char| STRIP.contains(&c) || is_space(c));
    if needle.is_empty() {
        return None;
    }
    let table = &*TABLE;
    if let Some(day) = table.exact.get(needle) {
        return Some(*day);
    }
    table
        .by_length
        .iter()
        .find(|(alias, _)| needle.starts_with(alias.as_str()))
        .map(|(_, day)| *day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Weekday::*;

    #[test]
    fn every_alias_resolves_to_its_own_day() {
        for (day, names) in ALIASES {
            for name in names {
                assert_eq!(recognise(name), Some(day), "{name}");
                assert_eq!(
                    recognise(&name.to_uppercase()),
                    Some(day),
                    "{name} upper case"
                );
                assert_eq!(
                    recognise(&format!(" ({name}). ")),
                    Some(day),
                    "{name} punctuated"
                );
            }
        }
    }

    #[test]
    fn the_documents_own_spellings() {
        assert_eq!(recognise("П`ятниця"), Some(Friday));
        assert_eq!(recognise("п’ятниця"), Some(Friday));
        assert_eq!(recognise("ПТ."), Some(Friday));
        assert_eq!(recognise("Понеділок 02.09"), Some(Monday));
        assert_eq!(recognise("Monday, 2 March"), Some(Monday));
        assert_eq!(recognise("Wednesday 02.09"), Some(Wednesday));
    }

    #[test]
    fn several_languages() {
        let cases = [
            ("Poniedziałek", Monday),
            ("Montag", Monday),
            ("вторник", Tuesday),
            ("wtorek", Tuesday),
            ("środa", Wednesday),
            ("sroda", Wednesday),
            ("среда", Wednesday),
            ("czwartek", Thursday),
            ("Donnerstag", Thursday),
            ("piątek", Friday),
            ("Vendredi", Friday),
            ("Sábado", Saturday),
            ("sobota", Saturday),
            ("niedziela", Sunday),
            ("Воскресенье", Sunday),
        ];
        for (text, day) in cases {
            assert_eq!(recognise(text), Some(day), "{text}");
        }
    }

    #[test]
    fn short_aliases_only_match_whole() {
        assert_eq!(recognise("пн"), Some(Monday));
        assert_eq!(recognise("пнх"), None);
        assert_eq!(recognise("satx"), Some(Saturday));
    }

    #[test]
    fn not_a_day() {
        assert_eq!(recognise("Історія України"), None);
        assert_eq!(recognise(""), None);
        assert_eq!(recognise(" .,- "), None);
        assert_eq!(recognise("Funday"), None);
        assert_eq!(recognise("so"), None);
    }
}
