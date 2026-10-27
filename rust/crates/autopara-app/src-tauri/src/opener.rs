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
        match self.app.opener().open_url(url, None::<&str>) {
            Ok(()) => {
                tracing::info!(url, "opened");
                true
            }
            Err(error) => {
                tracing::warn!(url, %error, "the system refused to open the link");
                false
            }
        }
    }
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
