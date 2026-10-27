//! Starting with the system, hidden in the tray (PORT_SPEC §B.3, §D; plan step 4.3).
//!
//! The same places the Python app and its installer use: the `AutoPara` value of
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` holding `"<exe>" --hidden` on Windows, and
//! `~/Library/LaunchAgents/com.autopara.app.plist` on macOS. The installer writes the identical
//! string, so the app sees its entry as already enabled instead of fighting it.
//!
//! A debug build never writes anything: running the app from a checkout must not replace the
//! user's real autostart entry with a path into `target/`.

use std::io;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

pub const VALUE_NAME: &str = "AutoPara";
#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The command the system should run at logon.
pub fn startup_command() -> io::Result<String> {
    Ok(format!(
        "\"{}\" --hidden",
        std::env::current_exe()?.display()
    ))
}

#[cfg(windows)]
fn current() -> Option<String> {
    windows_registry::CURRENT_USER
        .open(RUN_KEY)
        .ok()?
        .get_string(VALUE_NAME)
        .ok()
}

#[cfg(windows)]
fn write(command: Option<&str>) -> io::Result<()> {
    let key = windows_registry::CURRENT_USER
        .create(RUN_KEY)
        .map_err(io::Error::other)?;
    match command {
        Some(command) => key
            .set_string(VALUE_NAME, command)
            .map_err(io::Error::other),
        None => match key.remove_value(VALUE_NAME) {
            Ok(()) => Ok(()),
            Err(_) if current().is_none() => Ok(()),
            Err(error) => Err(io::Error::other(error)),
        },
    }
}

#[cfg(target_os = "macos")]
fn plist_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/LaunchAgents/com.autopara.app.plist"))
}

#[cfg(target_os = "macos")]
fn current() -> Option<String> {
    let text = std::fs::read_to_string(plist_path()?).ok()?;
    text.contains("<string>--hidden</string>").then(|| text)
}

#[cfg(target_os = "macos")]
fn write(command: Option<&str>) -> io::Result<()> {
    let path = plist_path().ok_or_else(|| io::Error::other("no home directory"))?;
    if command.is_none() {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe()?;
    let escape = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.autopara.app</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>--hidden</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
        escape(&exe.to_string_lossy())
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, plist)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn current() -> Option<String> {
    None
}

#[cfg(not(any(windows, target_os = "macos")))]
fn write(_command: Option<&str>) -> io::Result<()> {
    Ok(())
}

/// Whether the system will start AutoPara at logon.
pub fn is_enabled() -> bool {
    current().is_some()
}

/// Make the system entry match `enabled`, refreshing a stale path; returns the resulting state.
pub fn sync(enabled: bool) -> io::Result<bool> {
    if cfg!(debug_assertions) {
        tracing::info!(enabled, "debug build: the autostart entry is left alone");
        return Ok(is_enabled());
    }
    let wanted = startup_command()?;
    if enabled {
        // Windows: also refresh a stale path (the app moved). macOS: as the Python app, only
        // create the entry when it is missing.
        let stale = if cfg!(windows) {
            current().as_deref() != Some(wanted.as_str())
        } else {
            !is_enabled()
        };
        if stale {
            write(Some(&wanted))?;
        }
    } else if is_enabled() {
        write(None)?;
    }
    Ok(is_enabled())
}
