//! The desktop shell: the window, the tray, the background scheduler and the commands the
//! interface calls.

mod autostart;
mod commands;
mod ipc;
mod logging;
mod opener;
#[allow(dead_code)] // used by the assistant commands that follow
mod secrets;
mod tray;
mod updates;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use autopara_core::clock::{Clock, SystemClock};
use autopara_core::db::Storage;
use autopara_core::engine::{self, EngineMessage, Shared};
use autopara_core::i18n::{t, tf};
use autopara_core::import::{DATABASE_FILE, default_data_root};
use autopara_core::legacy::import_legacy;
use autopara_core::scheduler::{Scheduler, SchedulerEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_window_state::StateFlags;
use tokio::sync::mpsc;

use crate::commands::AppState;
use crate::opener::SharedOpener;

/// Scheduler events reach the interface under this name.
pub const SCHEDULER_EVENT: &str = "scheduler";

/// Bring the main window forward (tray click, a second launch, a class waiting for an answer).
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// `AUTOPARA_DATA_DIR` overrides the folder, so a development or test run never touches the
/// real one.
fn data_root(app: &AppHandle) -> PathBuf {
    std::env::var_os("AUTOPARA_DATA_DIR")
        .map(PathBuf::from)
        .or_else(default_data_root)
        .or_else(|| app.path().app_data_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Act on what the scheduler reports: tell the interface, notify, surface the window.
fn deliver(app: &AppHandle, message: EngineMessage) {
    let event = match message {
        EngineMessage::Error(error) => {
            tracing::error!(%error, "scheduler tick failed");
            return;
        }
        EngineMessage::Event(event) => event,
    };
    tracing::info!(?event, "scheduler");
    let _ = app.emit(SCHEDULER_EVENT, &event);

    let state = app.state::<AppState>();
    let (lesson, notifications) = {
        let storage = state
            .shared
            .storage
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let lesson_id = match &event {
            SchedulerEvent::Opened { lesson_id, .. }
            | SchedulerEvent::Missed { lesson_id, .. }
            | SchedulerEvent::CatchupAvailable { lesson_id, .. }
            | SchedulerEvent::Reminder { lesson_id, .. } => *lesson_id,
        };
        let enabled = storage
            .settings()
            .map(|s| s.notifications_enabled)
            .unwrap_or(true);
        (storage.lesson(lesson_id).ok().flatten(), enabled)
    };
    let Some(lesson) = lesson else { return };
    let start = lesson.start.to_string();
    let vars = [
        ("subject", lesson.subject.as_str()),
        ("start", start.as_str()),
    ];
    let message = match event {
        // The reminder is only produced when notifications are on (PORT_SPEC §C.6).
        SchedulerEvent::Reminder { .. } => Some(("notify_reminder_title", "notify_reminder_body")),
        SchedulerEvent::Opened { .. } if notifications => {
            Some(("notify_opened_title", "notify_opened_body"))
        }
        SchedulerEvent::CatchupAvailable { .. } => {
            // The banner is the answer; the window comes forward whatever the setting says.
            show_main_window(app);
            notifications.then_some(("notify_catchup_title", "notify_catchup_body"))
        }
        _ => None,
    };
    if let Some((title, body)) = message {
        let _ = app
            .notification()
            .builder()
            .title(t(title))
            .body(tf(body, &vars))
            .show();
    }
}

pub fn run() {
    let hidden = std::env::args().any(|arg| arg == "--hidden");
    tauri::Builder::default()
        // A second launch surfaces the first instead of starting a rival scheduler.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app)
        }))
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::all() - StateFlags::VISIBLE)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            let handle = app.handle().clone();
            let root = data_root(&handle);
            std::fs::create_dir_all(&root)?;
            let log_guard = logging::init(&root);
            tracing::info!(version = env!("CARGO_PKG_VERSION"), data = %root.display(), "starting");

            let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
            let mut storage = Storage::open(&root.join(DATABASE_FILE))?;
            match import_legacy(&mut storage, &root, clock.now()) {
                Ok(Some(report)) => tracing::info!(?report, "imported the previous version's data"),
                Ok(None) => {}
                Err(error) => {
                    tracing::error!(%error, "could not import the previous version's data")
                }
            }
            // Lecture Assist keeps its text for a limited time: 30 days unless set otherwise.
            let days = storage
                .get_setting("assist_retention_days")?
                .and_then(|value| value.parse().ok())
                .unwrap_or(30);
            match storage.purge_assist_older_than(days, clock.now()) {
                Ok(0) => {}
                Ok(removed) => tracing::info!(removed, "removed old assistant sessions"),
                Err(error) => tracing::warn!(%error, "could not remove old assistant sessions"),
            }
            let wanted = storage.settings()?.autostart_enabled;
            match autostart::sync(wanted) {
                Ok(actual) => {
                    storage.set_setting("autostart_enabled", if actual { "1" } else { "0" })?
                }
                Err(error) => tracing::warn!(%error, "could not update the autostart entry"),
            }

            let shared = Arc::new(Shared {
                storage: Mutex::new(storage),
                scheduler: Mutex::new(Scheduler::new()),
            });
            let opener = SharedOpener::new(handle.clone(), clock.clone());
            let (sender, mut receiver) = mpsc::unbounded_channel();
            let engine = tauri::async_runtime::block_on({
                let (shared, clock, opener) = (shared.clone(), clock.clone(), opener.clone());
                async move { engine::spawn(shared, clock, Box::new(opener), sender) }
            });
            let forward = handle.clone();
            tauri::async_runtime::spawn(async move {
                while let Some(message) = receiver.recv().await {
                    deliver(&forward, message);
                }
            });
            app.manage(AppState {
                shared,
                clock,
                opener,
                data_root: root,
                engine: Mutex::new(Some(engine)),
            });
            app.manage(Mutex::new(log_guard));

            if let Some(window) = app.get_webview_window("main") {
                window.set_title(t("app_title"))?;
                if !hidden {
                    window.show()?;
                }
            }
            tray::create(&handle)?;
            app.manage(updates::Updates::default());
            updates::schedule(handle.clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing the window hides it; only the tray's quit exits.
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::ThemeChanged(theme) => tray::set_theme(window.app_handle(), *theme),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            ipc::ping,
            commands::app_info,
            commands::get_settings,
            commands::save_settings,
            commands::set_view,
            commands::set_theme,
            commands::list_courses,
            commands::select_group,
            commands::group_lessons,
            commands::occurrences,
            commands::preview_timetable,
            commands::import_timetable,
            commands::pick_timetable,
            commands::save_lesson,
            commands::move_lesson,
            commands::delete_lesson,
            commands::open_lesson,
            commands::mark_lesson,
            commands::clear_mark,
            commands::diagnostics,
            commands::check_updates,
            commands::download_update,
            commands::install_update,
            commands::skip_update,
            commands::open_release_page,
        ])
        .build(tauri::generate_context!())
        .expect("the Tauri runtime failed to start")
        .run(|app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                tracing::info!("quitting");
                if let Some(engine) = app
                    .state::<AppState>()
                    .engine
                    .lock()
                    .ok()
                    .and_then(|mut e| e.take())
                {
                    tauri::async_runtime::block_on(engine.stop());
                }
            }
        });
}
