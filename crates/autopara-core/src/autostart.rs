#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const VALUE_NAME: &str = "AutoPara";

#[cfg(target_os = "macos")]
fn mac_plist_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        std::path::PathBuf::from(home)
            .join("Library")
            .join("LaunchAgents")
            .join("com.autopara.app.plist"),
    )
}

#[cfg(target_os = "macos")]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(target_os = "macos")]
fn mac_plist_content(program: &str) -> String {
    let escaped = xml_escape(program);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.autopara.app</string>
    <key>ProgramArguments</key>
    <array>
        <string>{escaped}</string>
        <string>--hidden</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#
    )
}

pub fn startup_command() -> String {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "AutoPara".to_string());
    format!("\"{exe}\" --hidden")
}

#[cfg(windows)]
pub fn is_enabled() -> bool {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(RUN_KEY)
        .and_then(|key| key.get_value::<String, _>(VALUE_NAME))
        .is_ok_and(|val| !val.is_empty())
}

#[cfg(windows)]
pub fn current_command() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(RUN_KEY)
        .and_then(|key| key.get_value::<String, _>(VALUE_NAME))
        .ok()
}

#[cfg(windows)]
pub fn enable() -> bool {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.create_subkey(RUN_KEY)
        .and_then(|(key, _)| key.set_value(VALUE_NAME, &startup_command()))
        .is_ok()
}

#[cfg(windows)]
pub fn disable() -> bool {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE) {
        Ok(key) => match key.delete_value(VALUE_NAME) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
            Err(_) => false,
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
pub fn is_enabled() -> bool {
    mac_plist_path().is_some_and(|p| p.is_file())
}

#[cfg(target_os = "macos")]
pub fn current_command() -> Option<String> {
    let path = mac_plist_path()?;
    let content = std::fs::read_to_string(path).ok()?;
    let exe = std::env::current_exe().ok()?.display().to_string();
    if content.contains(&xml_escape(&exe)) {
        Some(startup_command())
    } else {
        Some(String::new())
    }
}

#[cfg(target_os = "macos")]
pub fn enable() -> bool {
    let Some(path) = mac_plist_path() else {
        return false;
    };
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return false;
    }
    std::fs::write(path, mac_plist_content(&exe.display().to_string())).is_ok()
}

#[cfg(target_os = "macos")]
pub fn disable() -> bool {
    let Some(path) = mac_plist_path() else {
        return true;
    };
    match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn current_command() -> Option<String> {
    None
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn enable() -> bool {
    false
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn disable() -> bool {
    true
}

pub fn set_enabled(enabled: bool) -> bool {
    if enabled { enable() } else { disable() }
}

pub fn sync(enabled: bool) -> bool {
    if enabled {
        if current_command().as_deref() != Some(startup_command().as_str()) {
            return enable();
        }
    } else if is_enabled() {
        return disable();
    }
    true
}
