//! The scheduler under a fake clock (plan step 3.7).

// Test helpers fail loudly by design; a panic is how a test reports.
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use autopara_core::clock::{Clock, FakeClock};
use autopara_core::db::{Storage, parse_datetime};
use autopara_core::import::select_group;
use autopara_core::model::{Lesson, OccurrenceStatus, Recurrence, Weekday};
use autopara_core::parser::parse_file_default;
use autopara_core::scheduler::{LinkOpener, Scheduler, SchedulerEvent, TICK};
use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Kyiv;

/// Every URL handed to the "browser", in order.
#[derive(Clone, Default)]
struct Browser(Arc<Mutex<Vec<String>>>);

impl LinkOpener for Browser {
    fn open(&mut self, url: &str) -> bool {
        self.0.lock().unwrap().push(url.to_owned());
        true
    }
}

impl Browser {
    fn opened(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

fn at(text: &str) -> NaiveDateTime {
    parse_datetime(text).unwrap()
}

fn date(text: &str) -> NaiveDate {
    text.parse().unwrap()
}

const HISTORY: &str = "https://meet.google.com/aaa-bbbb-ccc"; // Monday 08:00-09:20, shared by both groups
const PEDAGOGY: &str = "https://us02web.zoom.us/j/1000000001"; // Monday 09:30-10:50, group 11 only

/// The sample timetable, imported long before the test week, group 11 chosen.
fn storage_with(path: &Path) -> Storage {
    let sample =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden/sample_timetable.docx");
    let mut storage = Storage::open(path).unwrap();
    if storage.courses().unwrap().is_empty() {
        storage
            .import_courses(
                &parse_file_default(&sample).unwrap(),
                at("2026-01-01T00:00:00"),
            )
            .unwrap();
        assert!(select_group(&storage, 1, "А2-Бд26-11 група").unwrap());
    }
    storage
}

fn storage() -> (tempfile::TempDir, Storage) {
    let dir = tempfile::tempdir().unwrap();
    let storage = storage_with(&dir.path().join("a.db"));
    (dir, storage)
}

/// Ticks every 15 s from the clock's current time until `until`, collecting events.
fn run(
    scheduler: &mut Scheduler,
    storage: &Storage,
    clock: &FakeClock,
    browser: &mut Browser,
    until: NaiveDateTime,
) -> Vec<SchedulerEvent> {
    let mut events = Vec::new();
    while clock.now() <= until {
        events.extend(
            scheduler
                .tick(storage, clock.now(), clock.monotonic(), browser)
                .unwrap(),
        );
        clock.advance(TICK);
    }
    events
}

fn opened_count(events: &[SchedulerEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, SchedulerEvent::Opened { .. }))
        .count()
}

#[test]
fn a_class_opens_once_a_minute_before_it_starts() {
    let (_dir, storage) = storage();
    let clock = FakeClock::new(at("2026-10-05T07:50:00"));
    let mut browser = Browser::default();
    let mut scheduler = Scheduler::new();
    let events = run(
        &mut scheduler,
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T09:00:00"),
    );
    assert_eq!(browser.opened(), [HISTORY]);
    let opened_at = storage.lessons_for_group(1).unwrap();
    let history = opened_at
        .iter()
        .find(|l| l.url.as_deref() == Some(HISTORY))
        .unwrap();
    let row = storage
        .occurrence(history.id, date("2026-10-05"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (row.status, row.fired_at),
        (OccurrenceStatus::Opened, at("2026-10-05T07:59:00"))
    );
    assert_eq!(opened_count(&events), 1);
}

#[test]
fn a_restart_inside_the_lead_window_does_not_open_again() {
    let (_dir, storage) = storage();
    let clock = FakeClock::new(at("2026-10-05T07:58:45"));
    let mut browser = Browser::default();
    run(
        &mut Scheduler::new(),
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T07:59:15"),
    );
    run(
        &mut Scheduler::new(),
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T08:30:00"),
    );
    assert_eq!(browser.opened(), [HISTORY]);
}

#[test]
fn after_a_sleep_a_running_class_is_offered_not_opened_even_in_open_mode() {
    let (_dir, storage) = storage();
    storage.set_setting("catchup_mode", "open").unwrap();
    let clock = FakeClock::new(at("2026-10-05T09:00:00"));
    let mut browser = Browser::default();
    let mut scheduler = Scheduler::new();
    run(
        &mut scheduler,
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T09:20:00"),
    );
    // Asleep from 09:20 to 09:40: the wall clock jumps, the monotonic clock barely moves.
    let mono = clock.monotonic();
    clock.set(at("2026-10-05T09:40:00"), mono + Duration::from_secs(15));
    let events = run(
        &mut scheduler,
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T11:00:00"),
    );
    assert!(!browser.opened().contains(&PEDAGOGY.to_owned()));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SchedulerEvent::CatchupAvailable { .. }))
            .count(),
        1
    );
    // Nobody answered: it ends missed.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SchedulerEvent::Missed { .. }))
    );
}

#[test]
fn a_class_that_starts_while_watching_still_opens_in_open_mode() {
    let (_dir, storage) = storage();
    storage.set_setting("catchup_mode", "open").unwrap();
    storage.set_setting("lead_minutes", "0").unwrap(); // the trigger is the start: always catch-up
    let clock = FakeClock::new(at("2026-10-05T09:25:00"));
    let mut browser = Browser::default();
    run(
        &mut Scheduler::new(),
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T09:31:00"),
    );
    assert_eq!(browser.opened(), [PEDAGOGY]);
}

#[test]
fn a_start_during_a_class_only_offers_it() {
    let (_dir, storage) = storage();
    storage.set_setting("catchup_mode", "open").unwrap();
    let clock = FakeClock::new(at("2026-10-05T10:00:00"));
    let mut browser = Browser::default();
    let events = run(
        &mut Scheduler::new(),
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T10:30:00"),
    );
    assert!(browser.opened().is_empty());
    assert!(matches!(
        events.as_slice(),
        [
            SchedulerEvent::Missed { .. },
            SchedulerEvent::CatchupAvailable { .. }
        ]
    ));
}

#[test]
fn moving_the_clock_back_or_forward_opens_nothing_twice() {
    let (_dir, storage) = storage();
    storage.set_setting("catchup_mode", "open").unwrap();
    let clock = FakeClock::new(at("2026-10-05T07:58:30"));
    let mut browser = Browser::default();
    let mut scheduler = Scheduler::new();
    run(
        &mut scheduler,
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T07:59:30"),
    );
    clock.set_wall(at("2026-10-05T07:58:00")); // back: through the trigger again
    run(
        &mut scheduler,
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T08:05:00"),
    );
    assert_eq!(browser.opened(), [HISTORY]);
    clock.set_wall(at("2026-10-05T09:40:00")); // forward: into the next class
    let events = run(
        &mut scheduler,
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T09:45:00"),
    );
    assert_eq!(
        browser.opened(),
        [HISTORY],
        "a jump into a running class only offers it"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SchedulerEvent::CatchupAvailable { .. }))
    );
}

#[test]
fn a_shared_class_opens_once_whichever_group_is_shown() {
    let (_dir, storage) = storage();
    let clock = FakeClock::new(at("2026-10-05T07:58:45"));
    let mut browser = Browser::default();
    let mut scheduler = Scheduler::new();
    run(
        &mut scheduler,
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T07:59:15"),
    );
    assert!(select_group(&storage, 1, "А2-Бд26-12 група").unwrap());
    run(
        &mut scheduler,
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T08:30:00"),
    );
    assert_eq!(browser.opened(), [HISTORY]);
}

#[test]
fn marks_settle_a_class_and_clearing_them_makes_it_eligible_again() {
    let (_dir, storage) = storage();
    let lessons = storage.lessons_for_group(1).unwrap();
    let history = lessons
        .iter()
        .find(|l| l.url.as_deref() == Some(HISTORY))
        .unwrap()
        .id;
    let clock = FakeClock::new(at("2026-10-05T07:50:00"));
    let mut browser = Browser::default();
    let mut scheduler = Scheduler::new();
    scheduler
        .mark(
            &storage,
            history,
            date("2026-10-05"),
            OccurrenceStatus::Skipped,
            clock.now(),
        )
        .unwrap();
    run(
        &mut scheduler,
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T07:59:30"),
    );
    assert!(browser.opened().is_empty(), "skipped in advance");
    scheduler
        .clear_mark(&storage, history, date("2026-10-05"))
        .unwrap();
    run(
        &mut scheduler,
        &storage,
        &clock,
        &mut browser,
        at("2026-10-05T07:59:45"),
    );
    assert_eq!(browser.opened(), [HISTORY]);
    scheduler
        .mark(
            &storage,
            history,
            date("2026-10-05"),
            OccurrenceStatus::Opened,
            clock.now(),
        )
        .unwrap();
    let row = storage
        .occurrence(history, date("2026-10-05"))
        .unwrap()
        .unwrap();
    assert_eq!(row.status, OccurrenceStatus::Opened);
}

#[test]
fn opening_by_hand_for_a_future_date_does_not_consume_it() {
    let (_dir, storage) = storage();
    let history = storage.lessons_for_group(1).unwrap()[0].id;
    let mut browser = Browser::default();
    let mut scheduler = Scheduler::new();
    let now = at("2026-10-05T12:00:00");
    assert!(
        scheduler
            .open_now(
                &storage,
                history,
                date("2026-10-12"),
                false,
                now,
                &mut browser
            )
            .unwrap()
    );
    assert!(
        storage
            .occurrence(history, date("2026-10-12"))
            .unwrap()
            .is_none()
    );
    assert!(
        scheduler
            .open_now(
                &storage,
                history,
                date("2026-10-05"),
                true,
                now,
                &mut browser
            )
            .unwrap()
    );
    let row = storage
        .occurrence(history, date("2026-10-05"))
        .unwrap()
        .unwrap();
    assert_eq!(row.status, OccurrenceStatus::Manual);
}

#[test]
fn http_and_other_schemes_are_never_opened() {
    let (_dir, storage) = storage();
    let group = storage.lessons_for_group(1).unwrap()[0].clone();
    let mut ids = Vec::new();
    for url in [
        "http://zoom.us/j/1",
        "file:///C:/Windows/system32/calc.exe",
        "javascript:alert(1)",
    ] {
        let lesson = Lesson {
            url: Some(url.into()),
            recurrence: Recurrence::Once {
                date: date("2026-10-07"),
            },
            ..group.clone()
        };
        ids.push(storage.add_lesson(&lesson, &[1]).unwrap());
    }
    let clock = FakeClock::new(at("2026-10-07T07:50:00"));
    let mut browser = Browser::default();
    let mut scheduler = Scheduler::new();
    run(
        &mut scheduler,
        &storage,
        &clock,
        &mut browser,
        at("2026-10-07T09:30:00"),
    );
    assert!(browser.opened().is_empty());
    for id in ids {
        assert!(
            !scheduler
                .open_now(
                    &storage,
                    id,
                    date("2026-10-07"),
                    true,
                    clock.now(),
                    &mut browser
                )
                .unwrap()
        );
    }
}

/// Two copies of the app racing on one database: the UNIQUE constraint lets one open.
#[test]
fn two_instances_racing_open_a_class_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.db");
    drop(storage_with(&path));
    let browser = Browser::default();
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let (path, mut browser) = (path.clone(), browser.clone());
            std::thread::spawn(move || {
                let storage = storage_with(&path);
                let clock = FakeClock::new(at("2026-10-05T07:58:30"));
                run(
                    &mut Scheduler::new(),
                    &storage,
                    &clock,
                    &mut browser,
                    at("2026-10-05T08:30:00"),
                );
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(browser.opened(), [HISTORY]);
}

/// Drive the clock with real Kyiv local time across a DST change, one tick every 15 s of real time.
fn run_kyiv(
    scheduler: &mut Scheduler,
    storage: &Storage,
    browser: &mut Browser,
    from_utc: &str,
    to_utc: &str,
) -> Vec<SchedulerEvent> {
    let utc = |text: &str| Utc.from_utc_datetime(&at(text));
    let (start, end) = (utc(from_utc), utc(to_utc));
    let clock = FakeClock::new(start.with_timezone(&Kyiv).naive_local());
    let mut events = Vec::new();
    let mut instant = start;
    while instant <= end {
        let elapsed = (instant - start).to_std().unwrap();
        clock.set(instant.with_timezone(&Kyiv).naive_local(), elapsed);
        events.extend(
            scheduler
                .tick(storage, clock.now(), clock.monotonic(), browser)
                .unwrap(),
        );
        instant += chrono::TimeDelta::seconds(15);
    }
    events
}

fn add_sunday(storage: &Storage, start: &str, end: &str, url: &str) {
    let template = storage.lessons_for_group(1).unwrap()[0].clone();
    let lesson = Lesson {
        weekday: Weekday::Sunday,
        start: start.parse().unwrap(),
        end: end.parse().unwrap(),
        url: Some(url.into()),
        recurrence: Recurrence::Weekly,
        ..template
    };
    storage.add_lesson(&lesson, &[1]).unwrap();
}

#[test]
fn kyiv_autumn_change_an_hour_that_happens_twice_opens_once() {
    let (_dir, storage) = storage();
    add_sunday(&storage, "03:30", "04:50", "https://zoom.us/j/330");
    // 2026-10-25: 04:00 EEST becomes 03:00 EET, so 03:29 local happens twice.
    let mut browser = Browser::default();
    run_kyiv(
        &mut Scheduler::new(),
        &storage,
        &mut browser,
        "2026-10-24T22:00:00",
        "2026-10-25T03:00:00",
    );
    assert_eq!(browser.opened(), ["https://zoom.us/j/330"]);
}

#[test]
fn kyiv_spring_change_a_class_in_the_missing_hour_is_offered_and_the_morning_opens() {
    let (_dir, storage) = storage();
    storage.set_setting("catchup_mode", "open").unwrap();
    add_sunday(&storage, "03:30", "04:50", "https://zoom.us/j/330");
    add_sunday(&storage, "08:00", "09:20", "https://zoom.us/j/800");
    // 2026-03-29: 03:00 EET becomes 04:00 EEST; 03:29 local never happens.
    let mut browser = Browser::default();
    let events = run_kyiv(
        &mut Scheduler::new(),
        &storage,
        &mut browser,
        "2026-03-28T23:00:00",
        "2026-03-29T07:00:00",
    );
    assert_eq!(browser.opened(), ["https://zoom.us/j/800"]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SchedulerEvent::CatchupAvailable { .. }))
    );
}
