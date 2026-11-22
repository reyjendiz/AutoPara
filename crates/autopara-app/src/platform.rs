#[cfg(windows)]
use std::sync::atomic::{AtomicIsize, Ordering};

#[cfg(windows)]
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

/// Ensures only one AutoPara instance runs at a time; surfaces existing window if already open.
#[cfg(windows)]
pub fn acquire_single_instance() -> Option<windows_sys::Win32::Foundation::HANDLE> {
    use std::ptr::null;
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };

    let name = wide("Local\\AutoPara.PureRust.SingleInstance");
    // SAFETY: `name` is a valid NUL-terminated UTF-16 string.
    let handle = unsafe { CreateMutexW(null(), 1, name.as_ptr()) };
    // SAFETY: `GetLastError` is always safe to call after Win32 API calls.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        for title in [
            "AutoPara — автозапуск созвонів і пар",
            "AutoPara — auto-join calls & classes",
            "AutoPara",
        ] {
            let wtitle = wide(title);
            // SAFETY: `wtitle` is a valid NUL-terminated UTF-16 string.
            let hwnd = unsafe { FindWindowW(null(), wtitle.as_ptr()) };
            if !hwnd.is_null() {
                // SAFETY: `hwnd` is a valid top-level window handle returned by `FindWindowW`.
                unsafe {
                    ShowWindow(hwnd, SW_SHOW);
                    ShowWindow(hwnd, SW_RESTORE);
                    SetForegroundWindow(hwnd);
                }
                break;
            }
        }
        return None;
    }
    if handle.is_null() { None } else { Some(handle) }
}

#[cfg(target_os = "macos")]
pub struct MacInstanceGuard(std::path::PathBuf);

#[cfg(target_os = "macos")]
impl Drop for MacInstanceGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Ensures a single AutoPara instance on macOS using a PID file inside Application Support.
#[cfg(target_os = "macos")]
pub fn acquire_single_instance() -> Option<MacInstanceGuard> {
    let lock_path = autopara_core::storage::default_db_path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("autopara.pid");

    if let Ok(existing) = std::fs::read_to_string(&lock_path)
        && let Ok(pid) = existing.trim().parse::<u32>()
        && pid != std::process::id()
        && std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .is_ok_and(|s| s.success())
    {
        let _ = std::process::Command::new("open")
            .args(["-a", "AutoPara"])
            .spawn();
        return None;
    }

    let _ = std::fs::write(&lock_path, std::process::id().to_string());
    Some(MacInstanceGuard(lock_path))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn acquire_single_instance() -> Option<()> {
    Some(())
}

/// Remembers our window's HWND for show/hide from the tray. Taken from eframe itself: on a
/// `--hidden` autostart the window is never visible, so searching visible windows misses it.
#[cfg(windows)]
pub fn capture_hwnd_if_needed(frame: &eframe::Frame) {
    if MAIN_HWND.load(Ordering::Relaxed) != 0 {
        return;
    }
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
    };

    let hwnd = match frame.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(w)) => w.hwnd.get() as windows_sys::Win32::Foundation::HWND,
        _ => std::ptr::null_mut(),
    };

    if !hwnd.is_null() {
        MAIN_HWND.store(hwnd as isize, Ordering::Relaxed);
        // Native Windows 11 rounded corners + hairline dark border for frameless window.
        let pref: u32 = DWMWCP_ROUND as u32;
        let border_bgr: u32 = 0x002E_2A2A; // #2a2a2e in COLORREF (0x00BBGGRR)
        // SAFETY: `hwnd` belongs to our own process; attribute pointers match size.
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                (&pref as *const u32).cast(),
                std::mem::size_of::<u32>() as u32,
            );
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR as u32,
                (&border_bgr as *const u32).cast(),
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
}

#[cfg(not(windows))]
pub fn capture_hwnd_if_needed(_frame: &eframe::Frame) {}

#[cfg(windows)]
pub fn show_main_window(ctx: &egui::Context) {
    let raw = MAIN_HWND.load(Ordering::Relaxed);
    if raw != 0 {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
        };
        let hwnd = raw as windows_sys::Win32::Foundation::HWND;
        // SAFETY: `hwnd` was captured from our own main window.
        unsafe {
            ShowWindow(hwnd, SW_SHOW);
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

#[cfg(not(windows))]
pub fn show_main_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

#[cfg(windows)]
pub fn hide_main_window(ctx: &egui::Context) {
    let raw = MAIN_HWND.load(Ordering::Relaxed);
    if raw != 0 {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
        let hwnd = raw as windows_sys::Win32::Foundation::HWND;
        // SAFETY: `hwnd` was captured from our own main window.
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
    // eframe must know too, or on a --hidden start it shows the window after the first frame
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
}

#[cfg(not(windows))]
pub fn hide_main_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
}

/// Native Windows 10/11 toast notification for advance reminders (`ReminderDue`).
#[cfg(windows)]
pub fn send_reminder_toast(title: &str, body: &str) {
    use winrt_notification::{Duration, Sound, Toast};
    let _ = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(body)
        .sound(Some(Sound::Default))
        .duration(Duration::Short)
        .show();
}

/// Native macOS notification banner via `osascript` (zero extra Objective-C crates needed).
#[cfg(target_os = "macos")]
pub fn send_reminder_toast(title: &str, body: &str) {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        esc(body),
        esc(title)
    );
    let _ = std::process::Command::new("osascript")
        .args(["-e", &script])
        .spawn();
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn send_reminder_toast(_title: &str, _body: &str) {}

pub fn build_tray_icon(
    logo_bytes: &[u8],
) -> Option<(tray_icon::TrayIcon, tray_icon::menu::MenuId, tray_icon::menu::MenuId)> {
    use tray_icon::menu::{Menu, MenuItem};
    use tray_icon::{Icon, TrayIconBuilder};

    let img = image::load_from_memory(logo_bytes).ok()?.into_rgba8();
    let resized = image::imageops::resize(&img, 32, 32, image::imageops::FilterType::Lanczos3);
    let icon = Icon::from_rgba(resized.into_raw(), 32, 32).ok()?;

    let open_item = MenuItem::new("Відкрити / Open", true, None);
    let quit_item = MenuItem::new("Вийти / Quit", true, None);
    let open_id = open_item.id().clone();
    let quit_id = quit_item.id().clone();

    let menu = Menu::new();
    let _ = menu.append(&open_item);
    let _ = menu.append(&quit_item);

    let tray = TrayIconBuilder::new()
        .with_tooltip("AutoPara")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .ok()?;

    Some((tray, open_id, quit_id))
}
