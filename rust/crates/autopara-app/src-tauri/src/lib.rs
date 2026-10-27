//! The desktop shell: the window, the tray and the commands the interface calls.

mod ipc;
mod tray;

use autopara_core::i18n::t;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                window.set_title(t("app_title"))?;
            }
            tray::create(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![ipc::ping])
        .run(tauri::generate_context!())
        .expect("the Tauri runtime failed to start");
}
