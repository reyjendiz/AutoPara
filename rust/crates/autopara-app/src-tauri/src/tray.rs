//! The tray icon. A placeholder until phase 4 gives it its menu.

use autopara_core::i18n::t;
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Runtime};

pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::with_id("main").tooltip(t("app_title"));
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}
