//! Parity with the Python implementation: `rust/fixtures/golden` (see its README).
//!
//! For every input document the Rust parser's output must equal the Python parser's, field for
//! field. `cargo xtask golden` runs this target.

use std::fs;
use std::path::{Path, PathBuf};

use autopara_core::parser::{ParsedCourse, parse_file_default};
use serde_json::{Value, json};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden")
}

fn documents() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(golden_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "docx"))
        .collect();
    found.sort();
    assert_eq!(found.len(), 3, "expected three golden input documents");
    found
}

/// The parse in the shape the Python exporter wrote it.
fn as_golden(courses: &[ParsedCourse]) -> Value {
    Value::Array(
        courses
            .iter()
            .map(|course| {
                json!({
                    "ordinal": course.ordinal,
                    "name": course.name,
                    "groups": course.groups.iter().map(|g| json!({
                        "name": g.name, "specialty": g.specialty, "col_lo": g.col_lo, "col_hi": g.col_hi,
                    })).collect::<Vec<_>>(),
                    "lessons": course.lessons.iter().map(|l| json!({
                        "day_index": l.weekday.index(),
                        "pair": l.pair,
                        "start_time": l.start.to_string(),
                        "end_time": l.end.to_string(),
                        "subject": l.subject,
                        "teacher": l.teacher,
                        "url": l.url,
                        "provider": l.provider.as_str(),
                        "group_names": l.group_names,
                        "linked_cells": l.linked_cells,
                        "needs_link": l.needs_link(),
                        "source_key": l.source_key(course.ordinal),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_parser_matches_python_on_every_golden_document() {
    for document in documents() {
        let expected = read_json(&document.with_extension("parse.json"));
        let actual = as_golden(&parse_file_default(&document).unwrap());
        let (Value::Array(expected), Value::Array(actual)) = (&expected, &actual) else {
            panic!("parse files are arrays");
        };
        assert_eq!(
            actual.len(),
            expected.len(),
            "{}: course count",
            document.display()
        );
        for (course, (got, want)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(got, want, "{}: course {}", document.display(), course + 1);
        }
    }
}

#[test]
fn every_document_has_its_scheduler_golden_file() {
    for document in documents() {
        let scheduler = read_json(&document.with_extension("scheduler.json"));
        assert!(
            scheduler["pure"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty())
        );
    }
}
