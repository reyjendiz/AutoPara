//! The scheduler against the Python app's decisions (`rust/fixtures/golden/*.scheduler.json`).
//!
//! The pure decisions must match row for row. The tick-by-tick simulations must match event for
//! event, except where the Rust rule is deliberately stricter (DECISIONS.md, "Catch-up only from
//! continuous watching"): a class the Python app opened while it was already running, because
//! the app had just started or woken in `open` mode, must instead be offered once, never opened,
//! and end up missed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use autopara_core::db::{Storage, parse_datetime};
use autopara_core::import::select_group;
use autopara_core::model::{Lesson, Provider, Recurrence};
use autopara_core::parser::{ParsedLesson, parse_file_default};
use autopara_core::scheduler::{
    Action, LinkOpener, Scheduler, SchedulerEvent, TICK, evaluate, predates_schedule, reminder_due,
};
use chrono::{NaiveDate, NaiveDateTime};
use serde_json::Value;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden")
}

fn documents() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(golden_dir())
        .expect("the golden directory is readable")
        .map(|entry| entry.expect("a golden directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "docx"))
        .collect();
    found.sort();
    found
}

fn time(value: &Value) -> NaiveDateTime {
    parse_datetime(value.as_str().expect("a time string")).expect("an ISO time")
}

fn as_lesson(index: usize, parsed: &ParsedLesson) -> Lesson {
    Lesson {
        id: index as i64 + 1,
        course_id: 1,
        weekday: parsed.weekday,
        pair: parsed.pair,
        start: parsed.start,
        end: parsed.end,
        subject: parsed.subject.clone(),
        teacher: parsed.teacher.clone(),
        url: parsed.url.clone(),
        provider: Provider::Unknown,
        needs_link: parsed.needs_link(),
        is_manual: false,
        source_key: String::new(),
        group_names: parsed.group_names.clone(),
        recurrence: Recurrence::Weekly,
        skip_dates: BTreeSet::new(),
    }
}

fn action_name(action: Action) -> &'static str {
    match action {
        Action::None => "none",
        Action::Open => "open",
        Action::Catchup => "catchup",
        Action::Miss => "miss",
    }
}

#[test]
fn pure_decisions_match_python_row_for_row() {
    for document in documents() {
        let courses = parse_file_default(&document).expect("the document parses");
        let golden: Value = serde_json::from_str(
            &fs::read_to_string(document.with_extension("scheduler.json")).expect("golden file"),
        )
        .expect("golden JSON");
        let rows = golden["pure"].as_array().expect("pure rows");
        for row in rows {
            let course = &courses[row["course"].as_u64().expect("course") as usize - 1];
            let index = row["lesson"].as_u64().expect("lesson") as usize;
            let lesson = as_lesson(index, &course.lessons[index]);
            let context = format!("{} {row}", document.display());
            match row["kind"].as_str() {
                Some("evaluate") => {
                    let now = time(&row["now"]);
                    let got = evaluate(
                        &lesson,
                        now,
                        row["lead"].as_u64().expect("lead") as u32,
                        row["already_fired"].as_bool().expect("fired"),
                        now.date(),
                    );
                    assert_eq!(action_name(got), row["result"], "{context}");
                }
                Some("reminder") => {
                    let now = time(&row["now"]);
                    let got = reminder_due(
                        &lesson,
                        now,
                        row["notify"].as_u64().expect("notify") as u32,
                        false,
                        now.date(),
                    );
                    assert_eq!(Value::Bool(got), row["result"], "{context}");
                }
                Some("predates") => {
                    let day: NaiveDate =
                        row["date"].as_str().expect("date").parse().expect("a date");
                    let got = predates_schedule(&lesson, day, Some(time(&row["active_from"])));
                    assert_eq!(Value::Bool(got), row["result"], "{context}");
                }
                other => panic!("unknown row kind {other:?}"),
            }
        }
    }
}

/// One event, as the Python exporter wrote it: `(time, event, lesson source key)`. The browser
/// hand-off is tied to the lesson whose `lesson_opened` follows it.
type Event = (String, String, String);

fn python_events(simulation: &Value) -> Vec<Event> {
    let raw = simulation["events"].as_array().expect("events");
    let mut events = Vec::new();
    for (position, event) in raw.iter().enumerate() {
        let kind = event["event"].as_str().expect("event kind").to_owned();
        let lesson = if kind == "browser" {
            raw[position + 1]["lesson"]
                .as_str()
                .expect("browser is followed by lesson_opened")
        } else {
            event["lesson"].as_str().expect("a lesson")
        };
        events.push((
            event["now"].as_str().expect("now").to_owned(),
            kind,
            lesson.to_owned(),
        ));
    }
    events
}

struct Recorder {
    now: NaiveDateTime,
    lesson: String,
    events: Vec<Event>,
}

impl LinkOpener for Recorder {
    fn open(&mut self, _url: &str) -> bool {
        self.events.push((
            self.now.format("%Y-%m-%dT%H:%M:%S").to_string(),
            "browser".into(),
            self.lesson.clone(),
        ));
        true
    }
}

/// Run one simulation the way the Python exporter did: a fresh database, ticks every 15 s, a new
/// scheduler for every cold start. Between two segments of one run the machine slept: the wall
/// clock jumped while the monotonic clock did not.
fn rust_simulation(
    document: &std::path::Path,
    simulation: &Value,
) -> (Vec<Event>, BTreeMap<(String, String), String>) {
    let mut storage = Storage::open_in_memory().expect("database");
    storage
        .import_courses(
            &parse_file_default(document).expect("parses"),
            time(&Value::from("2026-01-01T00:00:00")),
        )
        .expect("import");
    let ordinal = simulation["course"].as_u64().expect("course") as u32;
    assert!(
        select_group(
            &storage,
            ordinal,
            simulation["group"].as_str().expect("group")
        )
        .expect("select")
    );
    for (key, value) in [
        (
            "schedule_active_from",
            simulation["schedule_active_from"]
                .as_str()
                .expect("active")
                .to_owned(),
        ),
        (
            "catchup_mode",
            simulation["catchup_mode"]
                .as_str()
                .expect("mode")
                .to_owned(),
        ),
        ("lead_minutes", simulation["lead_minutes"].to_string()),
        ("notify_minutes", simulation["notify_minutes"].to_string()),
        (
            "notifications_enabled",
            if simulation["notifications_enabled"] == true {
                "1"
            } else {
                "0"
            }
            .to_owned(),
        ),
    ] {
        storage.set_setting(key, &value).expect("setting");
    }

    let key_of = |storage: &Storage, id: i64| {
        storage
            .lesson(id)
            .expect("lesson")
            .expect("exists")
            .source_key
    };
    let mut recorder = Recorder {
        now: time(&Value::from("2026-01-01T00:00:00")),
        lesson: String::new(),
        events: Vec::new(),
    };
    let mut scheduler = Scheduler::new();
    let mut monotonic = Duration::ZERO;
    for segment in simulation["segments"].as_array().expect("segments") {
        if segment["cold_start"] == true {
            scheduler = Scheduler::new();
        }
        let (mut now, end) = (time(&segment["start"]), time(&segment["end"]));
        while now <= end {
            recorder.now = now;
            // Which lesson a browser hand-off belongs to is learnt from the event that follows.
            let before = recorder.events.len();
            let events = scheduler
                .tick(&storage, now, monotonic, &mut recorder)
                .expect("tick");
            let mut browsers = recorder.events.split_off(before).into_iter();
            for event in events {
                let stamp = now.format("%Y-%m-%dT%H:%M:%S").to_string();
                let (kind, id) = match event {
                    SchedulerEvent::Opened { lesson_id, .. } => {
                        let (when, _, _) = browsers
                            .next()
                            .expect("a browser hand-off precedes an open");
                        recorder
                            .events
                            .push((when, "browser".into(), key_of(&storage, lesson_id)));
                        ("lesson_opened", lesson_id)
                    }
                    SchedulerEvent::Missed { lesson_id, .. } => ("lesson_missed", lesson_id),
                    SchedulerEvent::CatchupAvailable { lesson_id, .. } => {
                        ("catchup_available", lesson_id)
                    }
                    SchedulerEvent::Reminder { lesson_id, .. } => ("reminder_due", lesson_id),
                };
                recorder
                    .events
                    .push((stamp, kind.into(), key_of(&storage, id)));
            }
            now += chrono::TimeDelta::seconds(15);
            monotonic += TICK;
        }
    }
    let occurrences = storage
        .occurrences_between(
            "2026-01-01".parse().expect("date"),
            "2027-01-01".parse().expect("date"),
        )
        .expect("occurrences")
        .into_iter()
        .map(|o| {
            (
                (key_of(&storage, o.lesson_id), o.date.to_string()),
                o.status.as_str().to_owned(),
            )
        })
        .collect();
    (recorder.events, occurrences)
}

#[test]
fn simulations_match_python_except_where_rust_is_stricter() {
    let mut compared = 0;
    let mut divergent = 0;
    for document in documents() {
        let golden: Value = serde_json::from_str(
            &fs::read_to_string(document.with_extension("scheduler.json")).expect("golden file"),
        )
        .expect("golden JSON");
        for simulation in golden["simulations"].as_array().expect("simulations") {
            let context = format!(
                "{} {} course {}",
                document.display(),
                simulation["scenario"],
                simulation["course"]
            );
            let expected = python_events(simulation);
            let expected_occurrences: BTreeMap<(String, String), String> =
                simulation["occurrences"]
                    .as_array()
                    .expect("occurrences")
                    .iter()
                    .map(|o| {
                        let field = |name: &str| o[name].as_str().expect("field").to_owned();
                        ((field("lesson"), field("date")), field("status"))
                    })
                    .collect();
            let (actual, occurrences) = rust_simulation(&document, simulation);

            // Lessons the Python app opened after they had started: opened unasked on a start or
            // a wake. Those are where Rust deliberately differs.
            let mut starts: BTreeMap<String, Vec<String>> = BTreeMap::new();
            let courses = parse_file_default(&document).expect("parses");
            let ordinal = simulation["course"].as_u64().expect("course") as u32;
            for lesson in &courses[ordinal as usize - 1].lessons {
                starts
                    .entry(lesson.source_key(ordinal))
                    .or_default()
                    .push(lesson.start.to_string());
            }
            let late: BTreeSet<String> = expected
                .iter()
                .filter(|(when, kind, lesson)| {
                    kind == "browser"
                        && starts
                            .get(lesson)
                            .is_some_and(|s| s.iter().any(|start| &when[11..16] >= start.as_str()))
                })
                .map(|(_, _, lesson)| lesson.clone())
                .collect();
            let differs = simulation["catchup_mode"] == "open"
                && ["late_start_open", "sleep_across_start_open"]
                    .contains(&simulation["scenario"].as_str().expect("name"));
            assert!(
                late.is_empty() || differs,
                "{context}: only the two open-mode scenarios may differ"
            );
            divergent += usize::from(!late.is_empty());

            let keep = |events: &[Event]| -> Vec<Event> {
                events
                    .iter()
                    .filter(|(_, _, lesson)| !late.contains(lesson))
                    .cloned()
                    .collect()
            };
            assert_eq!(keep(&actual), keep(&expected), "{context}");
            for lesson in &late {
                let mine: Vec<&str> = actual
                    .iter()
                    .filter(|(_, _, l)| l == lesson)
                    .map(|(_, kind, _)| kind.as_str())
                    .collect();
                assert!(
                    !mine.contains(&"browser"),
                    "{context}: {lesson} must not open unasked"
                );
                assert_eq!(
                    mine.iter()
                        .filter(|kind| **kind == "catchup_available")
                        .count(),
                    1,
                    "{context}: {lesson}"
                );
                assert_eq!(
                    mine.last(),
                    Some(&"lesson_missed"),
                    "{context}: {lesson} ends missed"
                );
            }
            for (key, status) in &expected_occurrences {
                let want = if late.contains(&key.0) {
                    "missed"
                } else {
                    status.as_str()
                };
                assert_eq!(
                    occurrences.get(key).map(String::as_str),
                    Some(want),
                    "{context}: {key:?}"
                );
            }
            assert_eq!(occurrences.len(), expected_occurrences.len(), "{context}");
            compared += 1;
        }
    }
    assert_eq!(compared, 72, "every simulation was compared");
    assert!(divergent > 0, "the stricter catch-up rule was exercised");
}
