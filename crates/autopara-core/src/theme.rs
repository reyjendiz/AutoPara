use crate::models::{THEME_DARK, THEME_LIGHT};

#[cfg(windows)]
const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

#[cfg(windows)]
pub fn system_theme() -> &'static str {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu
        .open_subkey(PERSONALIZE_KEY)
        .and_then(|key| key.get_value::<u32, _>("AppsUseLightTheme"))
    {
        Ok(0) => THEME_DARK,
        _ => THEME_LIGHT,
    }
}

#[cfg(not(windows))]
pub fn system_theme() -> &'static str {
    THEME_LIGHT
}

pub fn resolve(theme: &str) -> &'static str {
    match theme {
        THEME_DARK => THEME_DARK,
        THEME_LIGHT => THEME_LIGHT,
        _ => system_theme(),
    }
}

pub fn next_theme(current: &str) -> &'static str {
    if resolve(current) == THEME_DARK {
        THEME_LIGHT
    } else {
        THEME_DARK
    }
}
