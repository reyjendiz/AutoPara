use std::sync::Mutex;
use std::time::{Duration, Instant};

const REPEAT_WINDOW: Duration = Duration::from_secs(3);

static LAST_OPEN: Mutex<Option<(String, Instant)>> = Mutex::new(None);

/// Only http:// and https:// URLs are opened — protects against file: / javascript: in documents.
pub fn is_openable(url: Option<&str>) -> bool {
    let Some(raw) = url else {
        return false;
    };
    let lower = raw.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Open `url` in the default browser via `ShellExecuteW` on Windows (zero console window flash).
pub fn open_url(url: Option<&str>) -> bool {
    if !is_openable(url) {
        return false;
    }
    // INFALLIBLE: `is_openable` returned true, so `url` is `Some`.
    let target = url.unwrap().trim().to_string();

    if let Ok(mut guard) = LAST_OPEN.lock() {
        let now = Instant::now();
        if let Some((prev_url, prev_at)) = guard.as_ref()
            && prev_url == &target
            && now.duration_since(*prev_at) < REPEAT_WINDOW
        {
            return true;
        }
        *guard = Some((target.clone(), now));
    }

    open_in_shell(&target)
}

#[cfg(windows)]
fn open_in_shell(url: &str) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    let op = wide("open");
    let target = wide(url);
    // SAFETY: `op` and `target` are valid NUL-terminated UTF-16 strings alive across the call.
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            op.as_ptr(),
            target.as_ptr(),
            null_mut(),
            null_mut(),
            SW_SHOWNORMAL,
        )
    };
    (result as isize) > 32
}

#[cfg(not(windows))]
fn open_in_shell(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(not(target_os = "macos"))]
    let cmd = "xdg-open";

    std::process::Command::new(cmd)
        .arg(url)
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guards_schemes() {
        assert!(is_openable(Some("https://zoom.us/j/123")));
        assert!(is_openable(Some("  HTTP://meet.google.com/abc ")));
        assert!(!is_openable(Some("file:///C:/Windows/win.ini")));
        assert!(!is_openable(Some("javascript:alert(1)")));
        assert!(!is_openable(None));
    }
}
