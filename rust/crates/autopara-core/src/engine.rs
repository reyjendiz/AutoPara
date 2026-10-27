//! The scheduler as a background task (plan step 3.3): it wakes when the next decision is due, or
//! at least every [`TICK`](crate::scheduler::TICK), or when told something changed, and never
//! holds the database while it waits.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;

use crate::clock::Clock;
use crate::db::Storage;
use crate::scheduler::{LinkOpener, Scheduler, SchedulerEvent, TICK};

/// Drops a second hand-off of the same link within three seconds (Python `launcher.open_url`):
/// two paths to "open" racing each other must not become two browser windows. The fire-once
/// guarantee itself is the database claim; this only absorbs accidents like a double click.
pub struct RepeatGuard<O> {
    inner: O,
    clock: Arc<dyn Clock>,
    last: Option<(String, Duration)>,
}

const REPEAT_WINDOW: Duration = Duration::from_secs(3);

impl<O: LinkOpener> RepeatGuard<O> {
    pub fn new(inner: O, clock: Arc<dyn Clock>) -> Self {
        Self {
            inner,
            clock,
            last: None,
        }
    }
}

impl<O: LinkOpener> LinkOpener for RepeatGuard<O> {
    fn open(&mut self, url: &str) -> bool {
        let now = self.clock.monotonic();
        if let Some((previous, at)) = &self.last
            && previous == url
            && now.saturating_sub(*at) < REPEAT_WINDOW
        {
            return true;
        }
        self.last = Some((url.to_owned(), now));
        self.inner.open(url)
    }
}

/// What the engine reports: a scheduler event, or a failure it will retry on the next tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineMessage {
    Event(SchedulerEvent),
    Error(String),
}

/// Shared state: the database and the scheduler's memory, each behind its own lock so the
/// window's commands (open now, mark) and the engine never step on each other.
pub struct Shared {
    pub storage: Mutex<Storage>,
    pub scheduler: Mutex<Scheduler>,
}

pub struct EngineHandle {
    wake: Arc<Notify>,
    stop: Arc<Notify>,
    task: JoinHandle<()>,
}

impl EngineHandle {
    /// Look again now: settings changed, a class was edited, or the system woke up.
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    /// Stop the engine and wait for it.
    pub async fn stop(self) {
        self.stop.notify_one();
        let _ = self.task.await;
    }
}

/// Start the engine on the current tokio runtime.
pub fn spawn(
    shared: Arc<Shared>,
    clock: Arc<dyn Clock>,
    mut opener: Box<dyn LinkOpener + Send>,
    messages: mpsc::UnboundedSender<EngineMessage>,
) -> EngineHandle {
    let wake = Arc::new(Notify::new());
    let stop = Arc::new(Notify::new());
    let (task_wake, task_stop) = (wake.clone(), stop.clone());
    let task = tokio::spawn(async move {
        loop {
            let (outcome, wait) = {
                let storage = shared
                    .storage
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                let mut scheduler = shared
                    .scheduler
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                let now = clock.now();
                let outcome = scheduler.tick(&storage, now, clock.monotonic(), opener.as_mut());
                let wait = scheduler.next_wake(&storage, now).unwrap_or(TICK);
                (outcome, wait)
            };
            match outcome {
                Ok(events) => {
                    for event in events {
                        let _ = messages.send(EngineMessage::Event(event));
                    }
                }
                Err(error) => {
                    let _ = messages.send(EngineMessage::Error(error.to_string()));
                }
            }
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = task_wake.notified() => {}
                () = task_stop.notified() => break,
            }
        }
    });
    EngineHandle { wake, stop, task }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FakeClock;
    use crate::db::parse_datetime;
    use crate::import::select_group;
    use crate::parser::parse_file_default;

    struct Recorder(Arc<Mutex<Vec<String>>>);

    impl LinkOpener for Recorder {
        fn open(&mut self, url: &str) -> bool {
            self.0.lock().unwrap().push(url.to_owned());
            true
        }
    }

    #[test]
    fn the_repeat_guard_absorbs_a_double_click() {
        let clock = Arc::new(FakeClock::new(
            parse_datetime("2026-10-05T08:00:00").unwrap(),
        ));
        let opened = Arc::new(Mutex::new(Vec::new()));
        let mut guard = RepeatGuard::new(Recorder(opened.clone()), clock.clone());
        assert!(guard.open("https://a"));
        assert!(guard.open("https://a"));
        clock.advance(Duration::from_secs(4));
        assert!(guard.open("https://a"));
        assert_eq!(opened.lock().unwrap().len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn the_engine_opens_a_due_class_and_reports_it() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/golden/sample_timetable.docx");
        let mut storage = Storage::open_in_memory().unwrap();
        let start = parse_datetime("2026-10-05T07:59:10").unwrap(); // Monday, inside the lead window
        storage
            .import_courses(
                &parse_file_default(&path).unwrap(),
                start - chrono::TimeDelta::days(1),
            )
            .unwrap();
        assert!(select_group(&storage, 1, "А2-Бд26-11 група").unwrap());
        let shared = Arc::new(Shared {
            storage: Mutex::new(storage),
            scheduler: Mutex::new(Scheduler::new()),
        });
        let clock = Arc::new(FakeClock::new(start));
        let opened = Arc::new(Mutex::new(Vec::new()));
        let (sender, mut receiver) = mpsc::unbounded_channel();

        let handle = spawn(
            shared.clone(),
            clock,
            Box::new(Recorder(opened.clone())),
            sender,
        );
        // The reminder is due too (ten minutes ahead); it comes first and opens nothing.
        let reminder = receiver.recv().await.unwrap();
        assert!(
            matches!(
                reminder,
                EngineMessage::Event(SchedulerEvent::Reminder { .. })
            ),
            "{reminder:?}"
        );
        let opened_event = receiver.recv().await.unwrap();
        assert!(
            matches!(
                opened_event,
                EngineMessage::Event(SchedulerEvent::Opened { .. })
            ),
            "{opened_event:?}"
        );
        handle.stop().await;
        assert_eq!(
            opened.lock().unwrap().as_slice(),
            ["https://meet.google.com/aaa-bbbb-ccc"]
        );
    }
}
