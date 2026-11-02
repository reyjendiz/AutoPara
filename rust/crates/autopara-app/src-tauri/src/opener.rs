//! Handing links to the system browser, through the opener plugin (plan step 3.5).

use std::sync::{Arc, Mutex, PoisonError};

use autopara_core::clock::Clock;
use autopara_core::engine::RepeatGuard;
use autopara_core::scheduler::LinkOpener;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// The system's default browser. Only ever called with links the scheduler accepted (https).
pub struct SystemBrowser {
    app: AppHandle,
}

impl LinkOpener for SystemBrowser {
    fn open(&mut self, url: &str) -> bool {
        let site = site(url);
        match self.app.opener().open_url(url, None::<&str>) {
            Ok(()) => {
                tracing::info!(site, "opened");
                true
            }
            Err(error) => {
                tracing::warn!(site, %error, "the system refused to open the link");
                false
            }
        }
    }
}

/// The host part of a link. Meeting links carry passcodes (`pwd=`), and the log's tail goes into
/// the diagnostics text a user may paste anywhere, so the log names only the site.
fn site(url: &str) -> &str {
    url.split('/').nth(2).unwrap_or_default()
}

/// One repeat guard shared by every path that opens a link (the scheduler, a card click, the
/// missed-class banner), so two of them racing still open one browser window.
#[derive(Clone)]
pub struct SharedOpener(Arc<Mutex<RepeatGuard<SystemBrowser>>>);

impl SharedOpener {
    pub fn new(app: AppHandle, clock: Arc<dyn Clock>) -> Self {
        Self(Arc::new(Mutex::new(RepeatGuard::new(
            SystemBrowser { app },
            clock,
        ))))
    }
}

impl LinkOpener for SharedOpener {
    fn open(&mut self, url: &str) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .open(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_names_only_the_site() {
        assert_eq!(
            site("https://us02web.zoom.us/j/123?pwd=secret"),
            "us02web.zoom.us"
        );
        assert_eq!(
            site("https://meet.google.com/abc-defg-hij"),
            "meet.google.com"
        );
        assert_eq!(site("nonsense"), "");
    }
}
