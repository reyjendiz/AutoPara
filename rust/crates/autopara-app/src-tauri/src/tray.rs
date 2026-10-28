//! The tray icon and its menu (plan step 4.2).

use autopara_core::i18n::t;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Theme, Wry};

pub const TRAY_ID: &str = "main";

/// Asks the window to open one of its screens (`import`, `settings`).
pub const TRAY_ACTION_EVENT: &str = "tray-action";

const LIGHT: &[u8] = include_bytes!("../icons/tray-light.png");
const DARK: &[u8] = include_bytes!("../icons/tray-dark.png");

/// The glyph for a light or dark system theme.
fn glyph(theme: Theme) -> tauri::Result<Image<'static>> {
    Image::from_bytes(if theme == Theme::Dark { DARK } else { LIGHT })
}

pub fn create(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let show = MenuItem::with_id(app, "show", t("tray_show"), true, None::<&str>)?;
    let import = MenuItem::with_id(app, "import", t("tray_import"), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", t("tray_settings"), true, None::<&str>)?;
    let updates = MenuItem::with_id(app, "updates", t("tray_updates"), true, None::<&str>)?;
    let listen = MenuItem::with_id(app, "listen", t("tray_listen"), true, None::<&str>)?;
    app.manage(ListenItem(listen.clone()));
    let quit = MenuItem::with_id(app, "quit", t("tray_quit"), true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu: Menu<Wry> = Menu::with_items(
        app,
        &[
            &show, &import, &settings, &updates, &listen, &separator, &quit,
        ],
    )?;

    let theme = app
        .get_webview_window("main")
        .and_then(|window| window.theme().ok())
        .unwrap_or(Theme::Light);
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(glyph(theme)?)
        .tooltip(t("app_title"))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => crate::show_main_window(app),
            "import" | "settings" => {
                crate::show_main_window(app);
                let _ = app.emit(TRAY_ACTION_EVENT, event.id().as_ref());
            }
            "updates" => {
                crate::show_main_window(app);
                tauri::async_runtime::spawn(crate::updates::check(app.clone(), true));
            }
            "listen" => crate::assist::toggle(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                crate::show_main_window(tray.app_handle());
            }
        })
        .build(app)
}

/// The "listen" menu item, relabelled while the assistant runs.
pub struct ListenItem(MenuItem<Wry>);

/// The glyph with a red dot in the corner: audio is being captured (plan 7.0).
fn recording(glyph: &Image<'_>) -> Image<'static> {
    let (width, height) = (glyph.width(), glyph.height());
    let mut rgba = glyph.rgba().to_vec();
    let radius = width.min(height) as f32 * 0.22;
    let (cx, cy) = (width as f32 - radius - 0.5, height as f32 - radius - 0.5);
    for y in 0..height {
        for x in 0..width {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            if dx * dx + dy * dy <= radius * radius {
                let i = ((y * width + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[0xE5, 0x39, 0x35, 0xFF]);
            }
        }
    }
    Image::new_owned(rgba, width, height)
}

fn current_theme(app: &AppHandle) -> Theme {
    app.get_webview_window("main")
        .and_then(|window| window.theme().ok())
        .unwrap_or(Theme::Light)
}

fn refresh_icon(app: &AppHandle, theme: Theme) {
    let capturing = app.state::<crate::assist::Assist>().capturing();
    if let (Some(tray), Ok(icon)) = (app.tray_by_id(TRAY_ID), glyph(theme)) {
        let icon = if capturing { recording(&icon) } else { icon };
        let _ = tray.set_icon(Some(icon));
    }
}

/// Follow the system's light or dark theme.
pub fn set_theme(app: &AppHandle, theme: Theme) {
    refresh_icon(app, theme);
}

/// Relabel the menu item and mark the icon while audio is captured.
pub fn set_listening(app: &AppHandle, running: bool, capturing: bool) {
    if let Some(item) = app.try_state::<ListenItem>() {
        let label = if running {
            "tray_stop_listening"
        } else {
            "tray_listen"
        };
        let _ = item.0.set_text(t(label));
    }
    let tooltip = if capturing {
        format!("{} — {}", t("app_title"), t("assist_recording"))
    } else {
        t("app_title").to_string()
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tooltip));
    }
    refresh_icon(app, current_theme(app));
}
