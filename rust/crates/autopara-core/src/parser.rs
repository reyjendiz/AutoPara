//! The domain layer of the import: a document's tables → courses, groups and lessons
//! (PORT_SPEC §A.5–§A.10). The rules that matter most:
//!
//! * R2 — a group owns a *range* of logical columns; a cell belongs to every group it overlaps,
//!   which is how one shared session becomes one lesson for several groups.
//! * R5 — a vertically merged lesson cell is one block spanning several pairs.
//! * R12 — the same class retyped in the next slot is that same block written the long way.

use std::collections::HashMap;
use std::path::Path;

use crate::days;
use crate::docx::{self, Cell, Document, DocxError, Table, VMerge};
use crate::i18n::tf;
use crate::model::{Provider, Weekday};
use crate::text::{dedupe, detect_provider, find_urls, is_teacher_line, key, normalize};
use crate::time::{
    DEFAULT_CLASS_MINUTES, TimeOfDay, last_pair_covered, pair_slot, parse_time_range,
};

/// Columns 0..=2 are День, Пара, Час; groups start here.
const FIRST_GROUP_COLUMN: u32 = 3;
const HEADER_ROWS: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGroup {
    pub name: String,
    pub specialty: String,
    pub col_lo: u32,
    pub col_hi: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLesson {
    pub weekday: Weekday,
    pub pair: u32,
    pub start: TimeOfDay,
    pub end: TimeOfDay,
    pub subject: String,
    /// The parenthesised line, parentheses included.
    pub teacher: String,
    pub url: Option<String>,
    pub provider: Provider,
    pub group_names: Vec<String>,
    /// How many source cells carried a link; merging collapses cells, so the link oracle in the
    /// tests compares this, not the lesson count, with the document's `<w:hyperlink>` count.
    pub linked_cells: u32,
}

impl ParsedLesson {
    pub fn needs_link(&self) -> bool {
        self.url.is_none()
    }

    /// Stable identity across re-imports: course ordinal, weekday, pair and folded subject.
    pub fn source_key(&self, course_ordinal: u32) -> String {
        format!(
            "{course_ordinal}|{}|{}|{}",
            self.weekday.index(),
            self.pair,
            key(&self.subject)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCourse {
    pub ordinal: u32,
    pub name: String,
    pub groups: Vec<ParsedGroup>,
    pub lessons: Vec<ParsedLesson>,
}

/// Parse a `.docx` file; `class_minutes` is the length assumed when a row gives no end time.
pub fn parse_file(path: &Path, class_minutes: u32) -> Result<Vec<ParsedCourse>, DocxError> {
    Ok(parse_document(&docx::read_document(path)?, class_minutes))
}

/// One course per table, in document order; the heading text is never used (R8, R10).
pub fn parse_document(document: &Document, class_minutes: u32) -> Vec<ParsedCourse> {
    document
        .tables()
        .zip(1..)
        .map(|(table, ordinal)| parse_table(table, ordinal, class_minutes))
        .collect()
}

/// The default when the caller has no class length of its own (tests, golden files).
pub fn parse_file_default(path: &Path) -> Result<Vec<ParsedCourse>, DocxError> {
    parse_file(path, DEFAULT_CLASS_MINUTES)
}

const ROMAN: [&str; 8] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII"];

/// `I курс`, `II курс`, … — always Ukrainian, whatever the document's heading said.
pub fn course_label(ordinal: u32) -> String {
    let numeral = ordinal
        .checked_sub(1)
        .and_then(|index| ROMAN.get(index as usize))
        .map_or_else(|| ordinal.to_string(), |roman| (*roman).to_owned());
    tf("course_label", &[("numeral", &numeral)])
}

fn header_groups(table: &Table) -> Vec<ParsedGroup> {
    if table.rows.len() < HEADER_ROWS {
        return Vec::new();
    }
    let mut groups: Vec<ParsedGroup> = table.rows[0]
        .cells
        .iter()
        .filter(|cell| cell.column >= FIRST_GROUP_COLUMN && !cell.is_empty())
        .map(|cell| ParsedGroup {
            name: normalize(&cell.lines.join(" ")),
            specialty: String::new(),
            col_lo: cell.column,
            col_hi: cell.last_column(),
        })
        .collect();
    for group in &mut groups {
        if let Some(cell) = table.rows[1]
            .at(group.col_lo)
            .filter(|cell| !cell.is_empty())
        {
            group.specialty = normalize(&cell.lines.join(" "));
        }
    }
    groups
}

/// A lesson cell's subject, teacher and link (R9), the doubled link deduped (R3).
fn cell_content(cell: &Cell) -> (String, String, Option<String>) {
    let mut urls: Vec<&str> = cell.links.iter().map(String::as_str).collect();
    let mut lines: Vec<String> = Vec::new();
    for line in &cell.lines {
        let found = find_urls(line);
        if found.is_empty() {
            lines.push(normalize(line));
            continue;
        }
        // Prose around a URL stays; the URL itself is not display text.
        let mut remainder = line.clone();
        for url in &found {
            remainder = remainder.replace(url, " ");
        }
        urls.extend(found);
        lines.push(normalize(&remainder));
    }
    lines.retain(|line| !line.is_empty());

    let mut subject = String::new();
    let mut teacher = String::new();
    for line in &lines {
        if is_teacher_line(line) {
            if teacher.is_empty() {
                teacher.clone_from(line);
            }
        } else if subject.is_empty() {
            subject.clone_from(line);
        }
    }
    if subject.is_empty()
        && let Some(first) = lines.first()
    {
        subject.clone_from(first);
    }
    let url = dedupe(urls).first().map(|url| (*url).to_owned());
    (subject, teacher, url)
}

fn parse_table(table: &Table, ordinal: u32, class_minutes: u32) -> ParsedCourse {
    let mut course = ParsedCourse {
        ordinal,
        name: course_label(ordinal),
        groups: header_groups(table),
        lessons: Vec::new(),
    };
    if course.groups.is_empty() {
        return course;
    }

    let mut current_day: Option<Weekday> = None;
    let mut open_blocks: HashMap<u32, usize> = HashMap::new(); // column → lesson started above (R5)
    let mut lessons: Vec<ParsedLesson> = Vec::new();

    for row in table.rows.iter().skip(HEADER_ROWS) {
        // R4: the day is written on the vMerge restart row only; it carries forward.
        if let Some(cell) = row.at(0).filter(|cell| !cell.is_empty())
            && let Some(day) = days::recognise(&cell.lines[0])
        {
            current_day = Some(day);
        }

        // R11: a bare start ("8.00") or a range ("8.00-9.20"); an explicit end wins.
        let (start, explicit_end) = row
            .at(2)
            .filter(|cell| !cell.is_empty())
            .map_or((None, None), |cell| parse_time_range(&cell.lines[0]));
        let (Some(start), Some(day)) = (start, current_day) else {
            continue;
        };

        // R6: a missing pair is inferred from the start time.
        let pair = row
            .at(1)
            .filter(|cell| !cell.is_empty())
            .map(|cell| normalize(&cell.lines[0]))
            .filter(|raw| !raw.is_empty() && raw.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|raw| raw.parse::<u32>().ok())
            .unwrap_or_else(|| u32::from(pair_slot(start)));

        let row_end = explicit_end.unwrap_or_else(|| start.add_minutes(class_minutes));

        for cell in row
            .cells
            .iter()
            .filter(|cell| cell.column >= FIRST_GROUP_COLUMN)
        {
            if cell.vmerge == Some(VMerge::Continue) {
                // R5: the block started above now covers this pair too.
                if let Some(&index) = open_blocks.get(&cell.column) {
                    lessons[index].end = row_end;
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
            let group_names = course
                .groups
                .iter()
                .filter(|group| cell.overlaps(group.col_lo, group.col_hi))
                .map(|group| group.name.clone())
                .collect();
            lessons.push(ParsedLesson {
                weekday: day,
                pair,
                start,
                end: row_end,
                subject,
                teacher,
                provider: detect_provider(url.as_deref()),
                linked_cells: u32::from(url.is_some()),
                url,
                group_names,
            });
            if cell.vmerge == Some(VMerge::Restart) {
                open_blocks.insert(cell.column, lessons.len() - 1);
            } else {
                open_blocks.remove(&cell.column);
            }
        }
    }

    course.lessons = merge_consecutive_slots(dedupe_duplicates(lessons));
    course
}

/// Fallback for documents where a shared class was written out in every group's column instead
/// of merged: equal day, pair, subject, teacher and link become one lesson for all those groups.
fn dedupe_duplicates(lessons: Vec<ParsedLesson>) -> Vec<ParsedLesson> {
    let mut kept: Vec<ParsedLesson> = Vec::new();
    let mut index: HashMap<(Weekday, u32, String, String, String), usize> = HashMap::new();
    for lesson in lessons {
        let identity = (
            lesson.weekday,
            lesson.pair,
            key(&lesson.subject),
            key(&lesson.teacher),
            lesson.url.clone().unwrap_or_default(),
        );
        if let Some(&at) = index.get(&identity) {
            let existing = &mut kept[at];
            existing.linked_cells += lesson.linked_cells;
            for name in lesson.group_names {
                if !existing.group_names.contains(&name) {
                    existing.group_names.push(name);
                }
            }
        } else {
            index.insert(identity, kept.len());
            kept.push(lesson);
        }
    }
    kept
}

/// Equal, or one side blank: the block then inherits the filled one (R12).
fn compatible(left: &str, right: &str) -> bool {
    let (first, second) = (normalize(left), normalize(right));
    first.is_empty() || second.is_empty() || key(&first) == key(&second)
}

/// Is `next` the same session as `block`, carrying on into the following slot (R12)?
///
/// Adjacency is a slot rule, never "this subject appears twice today": measured from the *last*
/// pair the block covers, which lets 2 → 3 → 4 chain.
fn continues(block: &ParsedLesson, next: &ParsedLesson) -> bool {
    let mut left = block.group_names.clone();
    let mut right = next.group_names.clone();
    left.sort();
    right.sort();
    if left != right || !compatible(&block.teacher, &next.teacher) {
        return false;
    }
    if let (Some(a), Some(b)) = (&block.url, &next.url)
        && a != b
    {
        return false;
    }
    if next.start < block.end {
        return false;
    }
    next.pair == u32::from(last_pair_covered(block.start, block.end)) + 1 || next.start == block.end
}

fn merge_consecutive_slots(lessons: Vec<ParsedLesson>) -> Vec<ParsedLesson> {
    let mut kept: Vec<ParsedLesson> = Vec::new();
    let mut open: HashMap<(Weekday, Vec<String>, String), usize> = HashMap::new();
    for lesson in lessons {
        let mut groups = lesson.group_names.clone();
        groups.sort();
        let block_key = (lesson.weekday, groups, key(&lesson.subject));
        if let Some(&at) = open.get(&block_key)
            && continues(&kept[at], &lesson)
        {
            let block = &mut kept[at];
            block.end = block.end.max(lesson.end);
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
        open.insert(block_key, kept.len());
        kept.push(lesson);
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::pair_start;

    fn slot(
        pair: u8,
        subject: &str,
        teacher: &str,
        url: Option<&str>,
        groups: &[&str],
    ) -> ParsedLesson {
        let start = pair_start(pair).unwrap();
        ParsedLesson {
            weekday: Weekday::Monday,
            pair: u32::from(pair),
            start,
            end: start.add_minutes(80),
            subject: subject.into(),
            teacher: teacher.into(),
            provider: detect_provider(url),
            linked_cells: u32::from(url.is_some()),
            url: url.map(Into::into),
            group_names: groups.iter().map(|g| (*g).to_owned()).collect(),
        }
    }

    const G: &[&str] = &["11 група", "12 група"];
    const Z: Option<&str> = Some("https://zoom.us/j/1");

    #[test]
    fn a_class_retyped_in_the_next_slot_is_one_lesson() {
        let merged = merge_consecutive_slots(vec![
            slot(2, "Педагогіка", "(Роман)", Z, G),
            slot(3, "Педагогіка", "", None, G),
        ]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].end.to_string(), "12:40");
        assert_eq!(merged[0].linked_cells, 1);
    }

    #[test]
    fn a_run_of_three_chains() {
        let merged = merge_consecutive_slots(vec![
            slot(2, "Педагогіка", "(Роман)", Z, G),
            slot(3, "Педагогіка", "(Роман)", Z, G),
            slot(4, "Педагогіка", "(Роман)", Z, G),
        ]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].end.to_string(), "14:20");
    }

    #[test]
    fn a_gap_keeps_two_sessions() {
        let merged = merge_consecutive_slots(vec![
            slot(2, "Педагогіка", "(Роман)", Z, G),
            slot(3, "Педагогіка", "(Роман)", Z, G),
            slot(5, "Педагогіка", "(Роман)", Z, G),
        ]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn different_teacher_link_or_groups_do_not_merge() {
        assert_eq!(
            merge_consecutive_slots(vec![slot(2, "S", "(A)", Z, G), slot(3, "S", "(B)", Z, G)])
                .len(),
            2
        );
        assert_eq!(
            merge_consecutive_slots(vec![
                slot(2, "S", "(A)", Z, G),
                slot(3, "S", "(A)", Some("https://zoom.us/j/2"), G)
            ])
            .len(),
            2
        );
        assert_eq!(
            merge_consecutive_slots(vec![
                slot(2, "S", "(A)", Z, G),
                slot(3, "S", "(A)", Z, &["11 група"])
            ])
            .len(),
            2
        );
    }

    #[test]
    fn course_labels_are_ukrainian_roman() {
        assert_eq!(course_label(1), "I курс");
        assert_eq!(course_label(4), "IV курс");
        assert_eq!(course_label(9), "9 курс");
    }

    #[test]
    fn source_keys_fold_the_subject() {
        let lesson = slot(2, "  Загальна  ПЕДАГОГІКА ", "", None, G);
        assert_eq!(lesson.source_key(1), "1|0|2|загальна педагогіка");
    }
}
