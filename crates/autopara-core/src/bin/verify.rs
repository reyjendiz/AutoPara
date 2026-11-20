//! Differential verification CLI used by `verify/diff_against_python.py` to prove
//! behavioral equivalence between `autopara-core` (Rust) and the original Python AutoPara.

use autopara_core::filters::{self, LessonFilter};
use autopara_core::models::Lesson;
use autopara_core::nextup;
use autopara_core::normalize;
use autopara_core::schedule_parser;
use autopara_core::scheduler;
use chrono::{NaiveDate, NaiveDateTime};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet};
use std::io::{self, BufRead};

fn parse_lesson(v: &Value) -> Lesson {
    let skip_dates: BTreeSet<NaiveDate> = v["skip_dates"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str())
                .filter_map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                .collect()
        })
        .unwrap_or_default();
    Lesson {
        id: v["id"].as_i64().unwrap_or(1),
        course_id: v["course_id"].as_i64().unwrap_or(1),
        day_index: v["day_index"].as_i64().unwrap_or(0),
        pair: v["pair"].as_i64().unwrap_or(1),
        start_time: v["start_time"].as_str().unwrap_or("08:00").to_string(),
        end_time: v["end_time"].as_str().unwrap_or("09:20").to_string(),
        subject: v["subject"].as_str().unwrap_or("").to_string(),
        teacher: v["teacher"].as_str().unwrap_or("").to_string(),
        url: v["url"].as_str().map(|s| s.to_string()),
        provider: v["provider"].as_str().unwrap_or("unknown").to_string(),
        needs_link: v["needs_link"].as_bool().unwrap_or(false),
        is_manual: v["is_manual"].as_bool().unwrap_or(false),
        source_key: v["source_key"].as_str().unwrap_or("").to_string(),
        group_names: v["group_names"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        on_date: v["on_date"]
            .as_str()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        repeat_until: v["repeat_until"]
            .as_str()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()),
        skip_dates,
    }
}

fn dispatch(req: &Value) -> Value {
    let op = req["op"].as_str().unwrap_or("");
    let args = &req["args"];
    match op {
        "normalize_text" => json!(normalize::normalize_text(args[0].as_str().unwrap_or(""))),
        "day_index" => json!(normalize::day_index(args[0].as_str().unwrap_or(""))),
        "parse_time" => json!(normalize::parse_time(args[0].as_str().unwrap_or(""))),
        "parse_time_range" => {
            let (s, e) = normalize::parse_time_range(args[0].as_str().unwrap_or(""));
            json!([s, e])
        }
        "pair_from_time" => json!(normalize::pair_from_time(args[0].as_str().unwrap_or(""))),
        "pair_slot" => json!(normalize::pair_slot(args[0].as_str().unwrap_or(""))),
        "last_pair_covered" => json!(normalize::last_pair_covered(
            args[0].as_str().unwrap_or(""),
            args[1].as_str().unwrap_or("")
        )),
        "add_minutes" => json!(normalize::add_minutes(
            args[0].as_str().unwrap_or("00:00"),
            args[1].as_i64().unwrap_or(0)
        )),
        "minutes_between" => json!(normalize::minutes_between(
            args[0].as_str().unwrap_or("00:00"),
            args[1].as_str().unwrap_or("00:00")
        )),
        "clean_url" => json!(normalize::clean_url(args[0].as_str().unwrap_or(""))),
        "find_urls" => json!(normalize::find_urls(args[0].as_str().unwrap_or(""))),
        "detect_provider" => json!(normalize::detect_provider(args[0].as_str())),
        "is_course_heading" => json!(normalize::is_course_heading(args[0].as_str().unwrap_or(""))),
        "course_label" => json!(normalize::course_label(args[0].as_i64().unwrap_or(1))),
        "subject_color" => json!(normalize::subject_color(args[0].as_str().unwrap_or(""))),
        "parse_file" => {
            let path = args[0].as_str().unwrap_or("");
            let mins = args.get(1).and_then(|v| v.as_i64());
            match schedule_parser::parse_file(path, mins) {
                Ok(courses) => json!(courses),
                Err(e) => json!({ "error": e.to_string() }),
            }
        }
        "evaluate" => {
            let lesson = parse_lesson(&args[0]);
            let now = NaiveDateTime::parse_from_str(
                args[1].as_str().unwrap_or(""),
                "%Y-%m-%dT%H:%M:%S",
            )
            .unwrap();
            let lead = args[2].as_i64().unwrap_or(1);
            let fired = args[3].as_bool().unwrap_or(false);
            let d = scheduler::evaluate(&lesson, now, lead, fired, None);
            json!({ "action": d.action, "lesson_id": d.lesson_id, "reason": d.reason })
        }
        "reminder_due" => {
            let lesson = parse_lesson(&args[0]);
            let now = NaiveDateTime::parse_from_str(
                args[1].as_str().unwrap_or(""),
                "%Y-%m-%dT%H:%M:%S",
            )
            .unwrap();
            let notify = args[2].as_i64().unwrap_or(10);
            let fired = args[3].as_bool().unwrap_or(false);
            json!(scheduler::reminder_due(&lesson, now, notify, fired, None))
        }
        "filter_subjects" => {
            let lessons: Vec<Lesson> = args[0]
                .as_array()
                .map(|a| a.iter().map(parse_lesson).collect())
                .unwrap_or_default();
            json!(filters::subjects(&lessons))
        }
        "filter_apply" => {
            let lessons: Vec<Lesson> = args[0]
                .as_array()
                .map(|a| a.iter().map(parse_lesson).collect())
                .unwrap_or_default();
            let flt = LessonFilter {
                text: args[1].as_str().unwrap_or("").to_string(),
                subject: args[2].as_str().unwrap_or("").to_string(),
            };
            let ids: Vec<i64> = filters::apply(&lessons, &flt).iter().map(|l| l.id).collect();
            json!(ids)
        }
        "nextup" => {
            let lessons: Vec<Lesson> = args[0]
                .as_array()
                .map(|a| a.iter().map(parse_lesson).collect())
                .unwrap_or_default();
            let now = NaiveDateTime::parse_from_str(
                args[1].as_str().unwrap_or(""),
                "%Y-%m-%dT%H:%M:%S",
            )
            .unwrap();
            let settled: HashSet<i64> = args[2]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_i64()).collect())
                .unwrap_or_default();
            match nextup::find(&lessons, now, &settled) {
                Some(item) => json!({
                    "lesson_id": item.lesson_id,
                    "ongoing": item.ongoing,
                    "minutes": item.minutes,
                    "label_uk": nextup::label(&item, "uk"),
                }),
                None => Value::Null,
            }
        }
        _ => json!({ "error": format!("unknown op: {op}") }),
    }
}

fn main() {
    let stdin = io::stdin();
    for line in stdin.lock().lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = serde_json::from_str(&line).expect("valid json line");
        let resp = dispatch(&req);
        println!("{}", serde_json::to_string(&resp).unwrap());
    }
}
