//! The interface's strings: one table, read by the Rust side and by the frontend alike.
//!
//! `i18n/uk.json` is the only place a user-visible word is written. Keys are English so a second
//! locale can be added later; Ukrainian is the only locale today (PORT_SPEC §E).

use std::collections::HashMap;
use std::sync::LazyLock;

/// The Ukrainian table, exactly as the frontend imports it.
pub const UK_JSON: &str = include_str!("../i18n/uk.json");

static UK: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(UK_JSON).expect("i18n/uk.json is a flat object of strings")
});

/// The Ukrainian text for `key`. An unknown key comes back as itself, so a typo shows on screen
/// rather than crashing the tray; the tests make sure no shipped key is missing.
pub fn t(key: &str) -> &'static str {
    match UK.get(key) {
        Some(text) => text,
        None => {
            debug_assert!(false, "missing i18n key {key}");
            // Leaking is fine here: it only happens for a key that is not in the table.
            Box::leak(key.to_owned().into_boxed_str())
        }
    }
}

/// `t(key)` with every `{name}` replaced by its value.
pub fn tf(key: &str, vars: &[(&str, &str)]) -> String {
    vars.iter().fold(t(key).to_owned(), |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_parses_and_has_the_window_title() {
        assert_eq!(t("app_title"), "AutoPara — автозапуск пар");
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(
            tf("settings_version", &[("version", "2.0.0")]),
            "Версія 2.0.0"
        );
    }
}
