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
    // Reserved for Lecture Assist (phase 7); shown, but not yet usable.
    let listen = MenuItem::with_id(app, "listen", t("tray_listen"), false, None::<&str>)?;
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

/// Follow the system's light or dark theme.
pub fn set_theme(app: &AppHandle, theme: Theme) {
    if let (Some(tray), Ok(icon)) = (app.tray_by_id(TRAY_ID), glyph(theme)) {
        let _ = tray.set_icon(Some(icon));
    }
}
