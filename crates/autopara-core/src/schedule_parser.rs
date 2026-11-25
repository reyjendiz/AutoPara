use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::docx_reader::{Block, Cell, Document, DocxError, Row, Table, read_document};
use crate::models::SESSION_KEY_PREFIX;
use crate::normalize::{
    DEFAULT_CLASS_MINUTES, add_minutes, casefold, course_label, course_ordinal, day_index, dedupe,
    detect_provider, find_urls, is_course_heading, is_teacher_line, last_pair_covered, normalize_text,
    pair_from_time, pair_slot, parse_date, parse_time, parse_time_range,
};

const FIRST_GROUP_COLUMN: i64 = 3;
const HEADER_ROWS: usize = 2;
/// Where session entries without a time are placed; their title says the time is not given.
const UNTIMED_START: &str = "08:00";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedGroup {
    pub name: String,
    pub specialty: String,
    pub col_lo: i64,
    pub col_hi: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedLesson {
    pub day_index: i64,
    pub pair: i64,
    pub start_time: String,
    pub end_time: String,
    pub subject: String,
    pub teacher: String,
    pub url: Option<String>,
    pub provider: String,
    pub group_names: Vec<String>,
    pub linked_cells: usize,
    /// Set for exams and credits, `None` for weekly classes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_date: Option<NaiveDate>,
}

impl ParsedLesson {
    pub fn needs_link(&self) -> bool {
        self.url.as_ref().is_none_or(|u| u.is_empty())
    }

    pub fn source_key(&self, course_ordinal: i64) -> String {
        let subject = casefold(&normalize_text(&self.subject));
        if let Some(date) = self.on_date {
            return format!(
                "{SESSION_KEY_PREFIX}{course_ordinal}|{date}|{}|{}|{subject}",
                self.start_time,
                self.group_names.join(",")
            );
        }
        format!("{course_ordinal}|{}|{}|{subject}", self.day_index, self.pair)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedCourse {
    pub ordinal: i64,
    pub name: String,
    pub groups: Vec<ParsedGroup>,
    pub lessons: Vec<ParsedLesson>,
}

fn header_groups(table: &Table) -> Vec<ParsedGroup> {
    if table.rows.len() < HEADER_ROWS {
        return Vec::new();
    }
    let mut groups = Vec::new();
    for cell in &table.rows[0].cells {
        if cell.column < FIRST_GROUP_COLUMN || cell.is_empty() {
            continue;
        }
        let name = normalize_text(&cell.lines.join(" "));
        groups.push(ParsedGroup {
            name,
            specialty: String::new(),
            col_lo: cell.column,
            col_hi: cell.last_column(),
        });
    }
    for group in &mut groups {
        if let Some(cell) = table.rows[1].at(group.col_lo)
            && !cell.is_empty()
        {
            group.specialty = normalize_text(&cell.lines.join(" "));
        }
    }
    groups
}

/// Text lines of a cell without the links in them, and its first link.
fn cell_lines(cell: &Cell) -> (Vec<String>, Option<String>) {
    let mut urls = cell.links.clone();
    let mut text_lines = Vec::new();
    for line in &cell.lines {
        let found = find_urls(line);
        if !found.is_empty() {
            urls.extend(found.iter().cloned());
            let mut remainder = line.clone();
            for url in &found {
                remainder = remainder.replace(url, " ");
            }
            let remainder = normalize_text(&remainder);
            if !remainder.is_empty() {
                text_lines.push(remainder);
            }
        } else {
            let norm = normalize_text(line);
            if !norm.is_empty() {
                text_lines.push(norm);
            }
        }
    }
    (text_lines, dedupe(urls).into_iter().next())
}

fn cell_content(cell: &Cell) -> (String, String, Option<String>) {
    let (text_lines, url) = cell_lines(cell);
    let mut subject = String::new();
    let mut teacher = String::new();
    for line in &text_lines {
        if is_teacher_line(line) {
            if teacher.is_empty() {
                teacher = line.clone();
            }
        } else if subject.is_empty() {
            subject = line.clone();
        }
    }
    if subject.is_empty()
        && let Some(first) = text_lines.first()
    {
        subject = first.clone();
    }
    (subject, teacher, url)
}

fn groups_for(cell: &Cell, groups: &[ParsedGroup]) -> Vec<String> {
    groups
        .iter()
        .filter(|g| cell.overlaps(g.col_lo, g.col_hi))
        .map(|g| g.name.clone())
        .collect()
}

fn dedupe_adjacent_duplicates(lessons: Vec<ParsedLesson>) -> Vec<ParsedLesson> {
    let mut merged: HashMap<_, usize> = HashMap::new();
    let mut result: Vec<ParsedLesson> = Vec::new();
    for lesson in lessons {
        let key = (
            lesson.on_date.map(|date| (date, lesson.start_time.clone())),
            lesson.day_index,
            lesson.pair,
            casefold(&normalize_text(&lesson.subject)),
            casefold(&normalize_text(&lesson.teacher)),
            lesson.url.clone().unwrap_or_default(),
        );
        if let Some(&idx) = merged.get(&key) {
            let existing = &mut result[idx];
            existing.linked_cells += lesson.linked_cells;
            for name in lesson.group_names {
                if !existing.group_names.contains(&name) {
                    existing.group_names.push(name);
                }
            }
        } else {
            merged.insert(key, result.len());
            result.push(lesson);
        }
    }
    result
}

fn compatible_text(left: &str, right: &str) -> bool {
    let first = normalize_text(left);
    let second = normalize_text(right);
    first.is_empty() || second.is_empty() || casefold(&first) == casefold(&second)
}

fn continues(block: &ParsedLesson, following: &ParsedLesson) -> bool {
    let mut left_groups = block.group_names.clone();
    left_groups.sort();
    let mut right_groups = following.group_names.clone();
    right_groups.sort();
    if left_groups != right_groups {
        return false;
    }
    if !compatible_text(&block.teacher, &following.teacher) {
        return false;
    }
    if let (Some(a), Some(b)) = (&block.url, &following.url)
        && !a.is_empty()
        && !b.is_empty()
        && a != b
    {
        return false;
    }
    if following.start_time < block.end_time {
        return false;
    }
    following.pair == last_pair_covered(&block.start_time, &block.end_time) + 1
        || following.start_time == block.end_time
}

fn merge_consecutive_slots(lessons: Vec<ParsedLesson>) -> Vec<ParsedLesson> {
    let mut kept: Vec<ParsedLesson> = Vec::new();
    let mut open_blocks: HashMap<(i64, Vec<String>, String), usize> = HashMap::new();

    for lesson in lessons {
        let mut sorted_groups = lesson.group_names.clone();
        sorted_groups.sort();
        let key = (
            lesson.day_index,
            sorted_groups,
            casefold(&normalize_text(&lesson.subject)),
        );
        if let Some(&idx) = open_blocks.get(&key)
            && continues(&kept[idx], &lesson)
        {
            let block = &mut kept[idx];
            if lesson.end_time > block.end_time {
                block.end_time = lesson.end_time;
            }
            if block.teacher.is_empty() {
                block.teacher = lesson.teacher;
            }
            if block.url.is_none() && lesson.url.is_some() {
                block.provider = detect_provider(lesson.url.as_deref());
                block.url = lesson.url;
            }
            block.linked_cells += lesson.linked_cells;
            continue;
        }
        let idx = kept.len();
        kept.push(lesson);
        open_blocks.insert(key, idx);
    }
    kept
}

fn parse_table(table: &Table, ordinal: i64, name: String, class_minutes: i64) -> ParsedCourse {
    let groups = header_groups(table);
    if groups.is_empty() {
        return ParsedCourse {
            ordinal,
            name,
            groups,
            lessons: Vec::new(),
        };
    }

    let mut current_day: Option<i64> = None;
    let mut open_blocks: HashMap<i64, usize> = HashMap::new();
    let mut lessons: Vec<ParsedLesson> = Vec::new();

    for row in table.rows.iter().skip(HEADER_ROWS) {
        let day_cell = row.at(0);
        let pair_cell = row.at(1);
        let time_cell = row.at(2);

        if let Some(cell) = day_cell
            && !cell.is_empty()
            && let Some(resolved) = day_index(&cell.lines[0])
        {
            current_day = Some(resolved);
        }

        let (start_time, explicit_end) = match time_cell {
            Some(cell) if !cell.is_empty() => parse_time_range(&cell.lines[0]),
            _ => (None, None),
        };
        let (Some(start_time), Some(day)) = (start_time, current_day) else {
            continue;
        };

        let mut pair: Option<i64> = None;
        if let Some(cell) = pair_cell
            && !cell.is_empty()
        {
            let raw = normalize_text(&cell.lines[0]);
            if !raw.is_empty() && raw.chars().all(|c| c.is_ascii_digit()) {
                pair = raw.parse().ok();
            }
        }
        let pair = pair.unwrap_or_else(|| pair_from_time(&start_time).unwrap_or_else(|| pair_slot(&start_time)));
        let row_end = explicit_end.unwrap_or_else(|| add_minutes(&start_time, class_minutes));

        for cell in &row.cells {
            if cell.column < FIRST_GROUP_COLUMN {
                continue;
            }
            if cell.vmerge.as_deref() == Some("continue") {
                if let Some(&idx) = open_blocks.get(&cell.column) {
                    lessons[idx].end_time = row_end.clone();
                }
                continue;
            }
            if cell.is_empty() {
                open_blocks.remove(&cell.column);
                continue;
            }
            let (subject, teacher, url) = cell_content(cell);
            if subject.is_empty() {
                open_blocks.remove(&cell.column);
                continue;
            }
            let linked_cells = usize::from(url.is_some());
            let provider = detect_provider(url.as_deref());
            let lesson = ParsedLesson {
                day_index: day,
                pair,
                start_time: start_time.clone(),
                end_time: row_end.clone(),
                subject,
                teacher,
                url,
                provider,
                group_names: groups_for(cell, &groups),
                linked_cells,
                on_date: None,
            };
            let idx = lessons.len();
            lessons.push(lesson);
            if cell.vmerge.as_deref() == Some("restart") {
                open_blocks.insert(cell.column, idx);
            } else {
                open_blocks.remove(&cell.column);
            }
        }
    }

    let lessons = merge_consecutive_slots(dedupe_adjacent_duplicates(lessons));
    ParsedCourse {
        ordinal,
        name,
        groups,
        lessons,
    }
}

fn is_date_header(row: &Row) -> bool {
    row.cells
        .first()
        .is_some_and(|c| ["дата", "date"].contains(&casefold(&normalize_text(&c.text())).as_str()))
}

/// Exam-session timetable: "Дата | group | group", one row per date.
fn is_session_table(table: &Table) -> bool {
    table.rows.iter().take(HEADER_ROWS).any(is_date_header)
        && table
            .rows
            .iter()
            .any(|row| row.cells.first().is_some_and(|c| parse_date(&c.text()).is_some()))
}

fn session_kind(title: &str) -> Option<&'static str> {
    let title = casefold(&normalize_text(title));
    match (
        title.contains("залік"),
        title.contains("іспит") || title.contains("екзамен"),
    ) {
        (true, false) => Some("Залік"),
        (false, true) => Some("Іспит"),
        _ => None,
    }
}

/// "ДВВ: 08.00, 09.30 Психологія" -> start/end times and "ДВВ: Психологія".
fn split_times(text: &str) -> (Vec<(String, Option<String>)>, String) {
    let mut times = Vec::new();
    let mut rest: Vec<&str> = Vec::new();
    let mut open = true;
    for word in text.split_whitespace() {
        if open {
            let mut parts = word.trim_matches([',', ';']).splitn(2, ['-', '–', '—']);
            if let Some(start) = parts.next().and_then(parse_time) {
                times.push((start, parts.next().and_then(parse_time)));
                continue;
            }
        }
        rest.push(word);
        open = times.is_empty() && word.ends_with(':');
    }
    (times, rest.join(" "))
}

// shortcut: one subject per cell, as in the weekly parser; split cells here if a session ever packs several
fn parse_session_table(
    table: &Table,
    titles: &[&str],
    ordinal: i64,
    class_minutes: i64,
) -> ParsedCourse {
    let ordinal = titles.iter().rev().find_map(|t| course_ordinal(t)).unwrap_or(ordinal);
    let mut kind = titles.iter().rev().find_map(|t| session_kind(t));
    let mut groups: Vec<ParsedGroup> = Vec::new();
    let mut columns: Vec<ParsedGroup> = Vec::new();
    let mut date: Option<NaiveDate> = None;
    let mut lessons = Vec::new();

    for row in &table.rows {
        let Some(first) = row.cells.first() else {
            continue;
        };
        if is_date_header(row) {
            columns = row.cells[1..]
                .iter()
                .map(|cell| ParsedGroup {
                    name: normalize_text(&cell.text()),
                    specialty: String::new(),
                    col_lo: cell.column,
                    col_hi: cell.last_column(),
                })
                .collect();
            for group in &columns {
                if !group.name.is_empty() && !groups.iter().any(|g| g.name == group.name) {
                    groups.push(group.clone());
                }
            }
            date = None;
            continue;
        }
        if first.vmerge.as_deref() != Some("continue") {
            date = parse_date(&first.text());
        }
        let Some(day) = date else {
            kind = session_kind(&first.text()).or(kind);
            continue;
        };

        let cells = &row.cells[1..];
        for (i, cell) in cells.iter().enumerate() {
            let (lines, url) = cell_lines(cell);
            let (teachers, texts): (Vec<String>, Vec<String>) =
                lines.into_iter().partition(|line| is_teacher_line(line));
            let (mut times, mut subject) = split_times(&texts.join(" "));
            if subject.is_empty() {
                continue;
            }
            // cell borders of a row can drift from the header's, so equal counts win over geometry
            let mut group_names = if cells.len() == columns.len() {
                vec![columns[i].name.clone()]
            } else {
                groups_for(cell, &columns)
            };
            group_names.retain(|name| !name.is_empty());
            if group_names.is_empty() {
                continue;
            }
            let mut teacher = teachers.into_iter().next().unwrap_or_default();
            if teacher.is_empty()
                && subject.ends_with(')')
                && let Some(at) = subject.rfind('(').filter(|&at| at > 0)
            {
                teacher = subject.split_off(at);
                subject.truncate(subject.trim_end().len());
            }
            let untimed = times.is_empty();
            if untimed {
                times.push((UNTIMED_START.to_string(), None));
            }
            let label = match (kind, untimed) {
                (Some(kind), false) => format!("{kind}: {subject}"),
                (Some(kind), true) => format!("{kind} (час не вказано): {subject}"),
                (None, false) => subject,
                (None, true) => format!("(час не вказано) {subject}"),
            };
            for (start_time, end_time) in times {
                lessons.push(ParsedLesson {
                    day_index: i64::from(day.weekday().num_days_from_monday()),
                    pair: pair_slot(&start_time),
                    end_time: end_time.unwrap_or_else(|| add_minutes(&start_time, class_minutes)),
                    start_time,
                    subject: label.clone(),
                    teacher: teacher.clone(),
                    provider: detect_provider(url.as_deref()),
                    linked_cells: usize::from(url.is_some()),
                    url: url.clone(),
                    group_names: group_names.clone(),
                    on_date: Some(day),
                });
            }
        }
    }

    ParsedCourse {
        ordinal,
        name: course_label(ordinal),
        groups,
        lessons: dedupe_adjacent_duplicates(lessons),
    }
}

pub fn parse_document(document: &Document, class_minutes: i64) -> Vec<ParsedCourse> {
    let mut courses: Vec<ParsedCourse> = Vec::new();
    let mut ordinal = 0;
    let mut titles: Vec<&str> = Vec::new();
    for block in &document.blocks {
        let table = match block {
            Block::Paragraph(text) => {
                titles.push(text);
                continue;
            }
            Block::Table(table) => table,
        };
        ordinal += 1;
        if !is_session_table(table) {
            courses.push(parse_table(table, ordinal, course_label(ordinal), class_minutes));
        } else {
            let parsed = parse_session_table(table, &titles, ordinal, class_minutes);
            // credits and exams of one course can come as separate tables
            if let Some(course) = courses.iter_mut().find(|c| c.ordinal == parsed.ordinal) {
                for group in parsed.groups {
                    if !course.groups.iter().any(|g| g.name == group.name) {
                        course.groups.push(group);
                    }
                }
                course.lessons.extend(parsed.lessons);
            } else if !parsed.lessons.is_empty() {
                courses.push(parsed);
            }
        }
        titles.clear();
    }
    courses
}

pub fn looks_like_schedule(document: &Document) -> bool {
    document
        .blocks
        .iter()
        .any(|b| matches!(b, Block::Paragraph(text) if is_course_heading(&normalize_text(text))))
}

pub fn parse_file(path: impl AsRef<Path>, class_minutes: Option<i64>) -> Result<Vec<ParsedCourse>, DocxError> {
    let doc = read_document(path)?;
    Ok(parse_document(&doc, class_minutes.unwrap_or(DEFAULT_CLASS_MINUTES)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<ParsedCourse> {
        // weekly table of course I and session table of course II, saved by Word as Word 97-2003
        parse_file(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/sample.doc"), None).unwrap()
    }

    #[test]
    fn doc_weekly_table() {
        let course = &sample()[0];
        assert_eq!(course.ordinal, 1);
        let got: Vec<_> = course
            .lessons
            .iter()
            .map(|l| (l.day_index, l.pair, l.subject.as_str(), l.url.as_deref(), l.group_names.join(",")))
            .collect();
        assert_eq!(
            got,
            [
                (0, 1, "Математика", Some("https://example.com/math"), "11 група,12 група".to_string()),
                (0, 2, "Фізика", Some("https://example.com/physics"), "12 група".to_string()),
            ]
        );
        assert!(course.lessons.iter().all(|l| l.on_date.is_none()));
    }

    #[test]
    fn doc_session_table() {
        let course = &sample()[1];
        assert_eq!(course.ordinal, 2);
        let got: Vec<_> = course
            .lessons
            .iter()
            .map(|l| {
                let date = l.on_date.unwrap().to_string();
                (date, l.start_time.as_str(), l.subject.as_str(), l.teacher.as_str(), l.group_names.join(","))
            })
            .collect();
        let row = |date: &str, time, subject, teacher, groups: &str| {
            (date.to_string(), time, subject, teacher, groups.to_string())
        };
        let ivanenko = "(доц. Іваненко І.І.)";
        assert_eq!(
            got,
            [
                row("2026-12-05", "08:00", "Залік (час не вказано): Практика", ivanenko, "21 група"),
                row("2026-12-10", "08:00", "Залік: ДВВ: Психологія", ivanenko, "21 група,22 група"),
                row("2026-12-10", "09:30", "Залік: ДВВ: Психологія", ivanenko, "21 група,22 група"),
                row("2026-12-16", "09:00", "Іспит: Педагогіка", "(проф. Коваль К.К.)", "22 група"),
            ]
        );
        assert_eq!(course.lessons[3].day_index, 2);
        assert_eq!(course.lessons[3].url.as_deref(), Some("https://example.com/ped"));
    }

    #[test]
    fn damaged_doc_is_an_error_not_a_panic() {
        let bytes = include_bytes!("../testdata/sample.doc");
        let path = std::env::temp_dir().join(format!("autopara-damaged-{}.doc", std::process::id()));
        for len in (0..bytes.len()).step_by(997) {
            std::fs::write(&path, &bytes[..len]).unwrap();
            let _ = parse_file(&path, None);
        }
        for at in (0..bytes.len()).step_by(61) {
            let mut broken = bytes.to_vec();
            broken[at] ^= 0xFF;
            std::fs::write(&path, &broken).unwrap();
            let _ = parse_file(&path, None);
        }
        let _ = std::fs::remove_file(&path);
    }
}
