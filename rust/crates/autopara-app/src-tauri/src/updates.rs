//! Updates from GitHub Releases (PORT_SPEC §G, plan step 6.2), through the Tauri updater: the
//! release's `latest.json` names the installer, and the plugin refuses anything whose signature
//! does not match the public key in `tauri.conf.json`.
//!
//! Checking happens shortly after start and every six hours (unless switched off); a manual check
//! always runs. Downloading may be automatic (`update_mode = auto`); installing is always the
//! user's click. A debug build only says that a version exists.

use std::sync::Mutex;
use std::time::Duration;

use autopara_core::i18n::{t, tf};
use autopara_core::model::UpdateMode;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::{Update, UpdaterExt};
use ts_rs::TS;

use crate::commands::AppState;

pub const UPDATE_EVENT: &str = "update";
const FIRST_CHECK: Duration = Duration::from_secs(15);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
const RELEASES: &str = "https://github.com/reyjendiz/AutoPara/releases/tag/v";

/// What the update strip shows.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum UpdateStatus {
    Available {
        version: String,
        notes: String,
        page: String,
        installable: bool,
    },
    Downloading {
        version: String,
        percent: u32,
    },
    Ready {
        version: String,
    },
    UpToDate {
        version: String,
        manual: bool,
    },
    Failed {
        manual: bool,
    },
}

#[derive(Default)]
pub struct Updates {
    pending: Mutex<Option<Update>>,
    downloaded: Mutex<Option<Vec<u8>>>,
}

/// Only a packaged build can replace itself.
fn installable() -> bool {
    !cfg!(debug_assertions)
}

fn emit(app: &AppHandle, status: &UpdateStatus) {
    let _ = app.emit(UPDATE_EVENT, status);
}

fn setting(app: &AppHandle, key: &str, value: &str) {
    let state = app.state::<AppState>();
    let storage = state
        .shared
        .storage
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _ = storage.set_setting(key, value);
}

fn settings(app: &AppHandle) -> Option<autopara_core::model::Settings> {
    let state = app.state::<AppState>();
    let storage = state
        .shared
        .storage
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    storage.settings().ok()
}

/// The first line of the release notes, as the strip's detail line.
fn first_line(notes: &str) -> String {
    notes
        .lines()
        .map(|line| line.trim().trim_start_matches(['#', '*', '-', ' ']).trim())
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(140).collect())
        .unwrap_or_default()
}

/// Ask GitHub. An automatic check obeys the setting and stays quiet about failures and about a
/// version the user skipped; a manual one always answers.
pub async fn check(app: AppHandle, manual: bool) {
    let Some(current) = settings(&app) else {
        return;
    };
    if !manual && current.update_mode == UpdateMode::Off {
        return;
    }
    let found = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(error) => Err(error),
    };
    let update = match found {
        Ok(update) => update,
        Err(error) => {
            tracing::info!(%error, "update check failed");
            if manual {
                emit(&app, &UpdateStatus::Failed { manual });
            }
            return;
        }
    };
    let now = app
        .state::<AppState>()
        .clock
        .now()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    setting(&app, "last_update_check", &now);
    let Some(update) = update else {
        emit(
            &app,
            &UpdateStatus::UpToDate {
                version: env!("CARGO_PKG_VERSION").into(),
                manual,
            },
        );
        return;
    };
    if !manual && update.version == current.skipped_update_version {
        return;
    }
    let version = update.version.clone();
    tracing::info!(%version, "a newer version is out");
    emit(
        &app,
        &UpdateStatus::Available {
            version: version.clone(),
            notes: first_line(update.body.as_deref().unwrap_or_default()),
            page: format!("{RELEASES}{version}"),
            installable: installable(),
        },
    );
    if current.notifications_enabled {
        let _ = app
            .notification()
            .builder()
            .title(t("notify_update_title"))
            .body(tf("notify_update_body", &[("version", &version)]))
            .show();
    }
    *app.state::<Updates>()
        .pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(update);
    if current.update_mode == UpdateMode::Auto && installable() {
        download(app).await;
    }
}

/// Fetch the pending update (verified by the plugin). Never installs it.
pub async fn download(app: AppHandle) {
    if !installable() {
        return;
    }
    let Some(update) = app
        .state::<Updates>()
        .pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
    else {
        return;
    };
    let version = update.version.clone();
    let mut received = 0usize;
    let mut last = u32::MAX;
    let progress_app = app.clone();
    let progress_version = version.clone();
    let result = update
        .download(
            move |chunk, total| {
                received += chunk;
                let percent =
                    total.map_or(0, |total| (received as u64 * 100 / total.max(1)) as u32);
                if percent != last {
                    last = percent;
                    emit(
                        &progress_app,
                        &UpdateStatus::Downloading {
                            version: progress_version.clone(),
                            percent,
                        },
                    );
                }
            },
            || {},
        )
        .await;
    match result {
        Ok(bytes) => {
            *app.state::<Updates>()
                .downloaded
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(bytes);
            emit(
                &app,
                &UpdateStatus::Ready {
                    version: version.clone(),
                },
            );
            crate::show_main_window(&app);
            if settings(&app).is_some_and(|s| s.notifications_enabled) {
                let _ = app
                    .notification()
                    .builder()
                    .title(t("notify_ready_title"))
                    .body(tf("notify_ready_body", &[("version", &version)]))
                    .show();
            }
        }
        Err(error) => {
            tracing::warn!(%error, "update download failed");
            emit(&app, &UpdateStatus::Failed { manual: true });
        }
    }
}

/// The user chose to install: run the installer (Windows) or replace the app (macOS), then restart.
pub fn install(app: &AppHandle) -> Result<(), String> {
    let updates = app.state::<Updates>();
    let update = updates
        .pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .ok_or("no_update")?;
    let bytes = updates
        .downloaded
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
        .ok_or("not_downloaded")?;
    tracing::info!(version = %update.version, "installing the update");
    update.install(bytes).map_err(|error| error.to_string())?;
    app.restart();
}

/// Check shortly after start, then every six hours.
pub fn schedule(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            check(app.clone(), false).await;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_shows_the_first_real_line_of_the_notes() {
        assert_eq!(
            first_line("\n## Windows: download it\nmore"),
            "Windows: download it"
        );
        assert_eq!(first_line(""), "");
        assert_eq!(first_line(&"x".repeat(300)).len(), 140);
    }
}
