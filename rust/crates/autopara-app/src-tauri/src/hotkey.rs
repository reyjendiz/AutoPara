//! The global shortcut that starts and stops listening (plan step 7.10).

use std::str::FromStr;

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::commands::{AppState, CommandResult};

pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+L";
const SETTING: &str = "assist_hotkey";

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                crate::assist::toggle(app);
            }
        })
        .build()
}

pub fn parse(text: &str) -> CommandResult<Shortcut> {
    Shortcut::from_str(text).map_err(|_| "assist_hotkey_invalid".to_string())
}

/// The saved shortcut, or the default.
pub fn saved(state: &AppState) -> String {
    state
        .storage()
        .get_setting(SETTING)
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_HOTKEY.to_string())
}

/// Swap to `text`. Another program holding it is reported as a conflict, not an error at start.
pub fn register(app: &AppHandle, text: &str) -> CommandResult<()> {
    let shortcut = parse(text)?;
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    shortcuts.register(shortcut).map_err(|error| {
        tracing::warn!(%error, "the listening shortcut is taken");
        "assist_hotkey_conflict".to_string()
    })
}

/// Whether the saved shortcut is ours right now.
pub fn active(app: &AppHandle) -> bool {
    parse(&saved(&app.state::<AppState>()))
        .is_ok_and(|shortcut| app.global_shortcut().is_registered(shortcut))
}

/// Save a new shortcut only if it could be registered; otherwise keep the old one.
#[tauri::command]
pub fn set_assist_hotkey(app: AppHandle, hotkey: String) -> CommandResult<()> {
    let state = app.state::<AppState>();
    let old = saved(&state);
    if let Err(code) = register(&app, &hotkey) {
        let _ = register(&app, &old);
        return Err(code);
    }
    state
        .storage()
        .set_setting(SETTING, &hotkey)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use autopara_core::i18n::t;

    use super::*;

    #[test]
    fn default_hotkey_parses() {
        assert!(parse(DEFAULT_HOTKEY).is_ok());
        assert!(parse("Ctrl+Shift+F9").is_ok());
    }

    #[test]
    fn nonsense_hotkey_is_rejected_with_a_string() {
        let code = parse("Ctrl+Banana").unwrap_err();
        assert!(!t(&code).is_empty());
        assert!(!t("assist_hotkey_conflict").is_empty());
    }
}
