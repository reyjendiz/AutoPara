//! The log file, the panic hook and the diagnostics text (plan step 4.4).
//!
//! The log lives beside the data (`<data>/logs/autopara.<date>.log`, a week of files) because a
//! windowed app has no console. A panic is written there before the default hook runs.

use std::fs;
use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};

pub const LOG_DIR: &str = "logs";
const KEEP_FILES: usize = 7;
const DIAGNOSTIC_LOG_LINES: usize = 200;

/// Start logging to `<data_root>/logs`. The guard must live as long as the app.
pub fn init(data_root: &Path) -> Option<WorkerGuard> {
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("autopara")
        .filename_suffix("log")
        .max_log_files(KEEP_FILES)
        .build(data_root.join(LOG_DIR))
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .try_init()
        .ok()?;

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(%info, "panic");
        previous(info);
    }));
    Some(guard)
}

/// The newest log file, if any.
fn latest_log(data_root: &Path) -> Option<PathBuf> {
    fs::read_dir(data_root.join(LOG_DIR))
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "log"))
        .max_by_key(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok())
}

/// What "copy diagnostics" puts on the clipboard: versions, where the data is, a summary of the
/// timetable and the end of the log. Never a meeting link: those are the timetable's secrets.
pub fn diagnostics(data_root: &Path, summary: &str) -> String {
    let tail = latest_log(data_root)
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|text| {
            let lines: Vec<&str> = text.lines().collect();
            let from = lines.len().saturating_sub(DIAGNOSTIC_LOG_LINES);
            lines[from..].join("\n")
        })
        .unwrap_or_default();
    format!(
        "AutoPara {version}\n{os} {arch}\nData: {data}\n{summary}\n\n--- log ---\n{tail}\n",
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        data = data_root.display(),
    )
}
