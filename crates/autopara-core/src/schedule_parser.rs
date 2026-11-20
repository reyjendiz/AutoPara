use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::docx_reader::{Block, Cell, Document, DocxError, Table, read_document};
use crate::normalize::{
    DEFAULT_CLASS_MINUTES, add_minutes, casefold, course_label, day_index, dedupe, detect_provider,
    find_urls, is_course_heading, is_teacher_line, last_pair_covered, normalize_text,
    pair_from_time, pair_slot, parse_time_range,
};

const FIRST_GROUP_COLUMN: i64 = 3;
const HEADER_ROWS: usize = 2;

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
}

impl ParsedLesson {
    pub fn needs_link(&self) -> bool {
        self.url.as_ref().is_none_or(|u| u.is_empty())
    }

    pub fn source_key(&self, course_ordinal: i64) -> String {
        let subject = casefold(&normalize_text(&self.subject));
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

fn cell_content(cell: &Cell) -> (String, String, Option<String>) {
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

    let unique = dedupe(urls);
    (subject, teacher, unique.into_iter().next())
}

fn groups_for(cell: &Cell, groups: &[ParsedGroup]) -> Vec<String> {
    groups
        .iter()
        .filter(|g| cell.overlaps(g.col_lo, g.col_hi))
        .map(|g| g.name.clone())
        .collect()
}

fn dedupe_adjacent_duplicates(lessons: Vec<ParsedLesson>) -> Vec<ParsedLesson> {
    let mut merged: HashMap<(i64, i64, String, String, String), usize> = HashMap::new();
    let mut result: Vec<ParsedLesson> = Vec::new();
    for lesson in lessons {
        let key = (
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

pub fn parse_document(document: &Document, class_minutes: i64) -> Vec<ParsedCourse> {
    let mut courses = Vec::new();
    let mut ordinal = 0;
    for block in &document.blocks {
        if let Block::Table(table) = block {
            ordinal += 1;
            courses.push(parse_table(table, ordinal, course_label(ordinal), class_minutes));
        }
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
