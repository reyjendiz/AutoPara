//! Google Calendar link: background sync every minute and sign-in/out, shared with the UI.

use autopara_core::gcal::{self, GcalError};
use autopara_core::launcher;
use chrono::{Duration as ChronoDuration, Local, Utc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::SharedBackend;

const SYNC_EVERY: Duration = Duration::from_secs(60);
const DAYS_AHEAD: i64 = 30;

#[derive(Debug, Clone, Default)]
pub struct GcalStatus {
    pub connected: bool,
    pub signing_in: bool,
    pub account: Option<String>,
    /// local "HH:MM" of the last successful sync
    pub last_sync: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct Gcal {
    pub status: Arc<Mutex<GcalStatus>>,
    sync_now: Arc<AtomicBool>,
}

impl Gcal {
    /// Starts the sync thread. Does nothing (and the UI hides the section) when the build
    /// has no Google client configured.
    pub fn start(
        backend: Arc<Mutex<SharedBackend>>,
        quit: Arc<AtomicBool>,
        ctx: egui::Context,
    ) -> Option<Self> {
        if !gcal::is_configured() {
            return None;
        }
        let me = Self {
            status: Arc::new(Mutex::new(GcalStatus {
                connected: gcal::load_refresh_token().is_some(),
                ..Default::default()
            })),
            sync_now: Arc::new(AtomicBool::new(true)),
        };
        let bg = me.clone();
        std::thread::spawn(move || bg.run(&backend, &quit, &ctx));
        Some(me)
    }

    pub fn snapshot(&self) -> GcalStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn set(&self, f: impl FnOnce(&mut GcalStatus)) {
        if let Ok(mut s) = self.status.lock() {
            f(&mut s);
        }
    }

    pub fn sign_in(&self, ctx: &egui::Context) {
        if self.snapshot().signing_in {
            return;
        }
        self.set(|s| {
            s.signing_in = true;
            s.error = None;
        });
        let me = self.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let res = gcal::sign_in(|url| launcher::open_url(Some(url)))
                .and_then(|token| gcal::save_refresh_token(&token));
            me.set(|s| {
                s.signing_in = false;
                match &res {
                    Ok(()) => s.connected = true,
                    Err(GcalError::Denied) => {}
                    Err(e) => s.error = Some(e.to_string()),
                }
            });
            if res.is_ok() {
                me.sync_now.store(true, Ordering::Relaxed);
            }
            ctx.request_repaint();
        });
    }

    /// Forget the account and remove the meetings it brought in.
    pub fn sign_out(&self, backend: &Arc<Mutex<SharedBackend>>) {
        gcal::clear_refresh_token();
        if let Ok(mut g) = backend.lock() {
            let _ = g.storage.delete_calendar_lessons();
            g.dirty = true;
        }
        self.set(|s| *s = GcalStatus::default());
    }

    fn run(&self, backend: &Arc<Mutex<SharedBackend>>, quit: &AtomicBool, ctx: &egui::Context) {
        let mut access: Option<(String, Instant)> = None;
        let mut next = Instant::now();
        loop {
            if quit.load(Ordering::Relaxed) {
                return;
            }
            if Instant::now() < next && !self.sync_now.swap(false, Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(250));
                continue;
            }
            next = Instant::now() + SYNC_EVERY;
            self.sync_now.store(false, Ordering::Relaxed);

            let Some(refresh) = gcal::load_refresh_token() else {
                access = None;
                continue;
            };
            match self.sync_once(backend, &refresh, &mut access) {
                Ok(changed) => {
                    self.set(|s| {
                        s.connected = true;
                        s.error = None;
                        s.last_sync = Some(Local::now().format("%H:%M").to_string());
                    });
                    if changed {
                        ctx.request_repaint();
                    }
                }
                Err(GcalError::Revoked) => {
                    gcal::clear_refresh_token();
                    access = None;
                    self.set(|s| {
                        s.connected = false;
                        s.error = Some(GcalError::Revoked.to_string());
                    });
                    ctx.request_repaint();
                }
                // offline etc.: keep the meetings we have and try again next minute
                Err(e) => {
                    access = None;
                    self.set(|s| s.error = Some(e.to_string()));
                }
            }
        }
    }

    fn sync_once(
        &self,
        backend: &Arc<Mutex<SharedBackend>>,
        refresh: &str,
        access: &mut Option<(String, Instant)>,
    ) -> Result<bool, GcalError> {
        if access.as_ref().is_none_or(|(_, until)| Instant::now() >= *until) {
            *access = Some(gcal::access_token(refresh)?);
        }
        let token = &access.as_ref().map(|(t, _)| t.clone()).unwrap_or_default();

        let today = Local::now().date_naive();
        let from = today
            .and_hms_opt(0, 0, 0)
            .and_then(|d| d.and_local_timezone(Local).earliest())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        let to = Utc::now() + ChronoDuration::days(DAYS_AHEAD);
        let (account, events) = gcal::fetch_events(token, from, to)?;

        let mut g = backend
            .lock()
            .map_err(|_| GcalError::Other("storage busy".into()))?;
        let untitled = match g.storage.settings().map(|s| s.language) {
            Ok(l) if l == autopara_core::models::LANG_EN => "(no title)",
            _ => "(без назви)",
        };
        let lessons: Vec<_> = events.iter().map(|e| gcal::to_lesson(e, untitled)).collect();
        let changed = g
            .storage
            .sync_calendar_lessons(&lessons, today)
            .map_err(|e| GcalError::Other(e.to_string()))?;
        if changed {
            g.dirty = true;
        }
        drop(g);
        self.set(|s| s.account = account);
        Ok(changed)
    }
}
