//! Reader for legacy binary Word files (`.doc`, Word 97 and newer, [MS-DOC]).
//! Produces the same `Document` model as the `.docx` reader.

use crate::docx_reader::{Block, Cell, Document, DocxError, ParagraphBuilder, Row, Table};
use std::fs::File;
use std::io::{Read, Seek};

/// Cell edges closer than this (twips, ~0.7 mm) are the same grid line.
const EDGE_TOLERANCE: i64 = 40;
/// Windows-1252 characters for bytes 0x80..=0x9F (0 = undefined).
const CP1252_HIGH: [u16; 32] = [
    0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0,
    0x017D, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A,
    0x0153, 0, 0x017E, 0x0178,
];

fn slice(data: &[u8], at: usize, len: usize) -> Option<&[u8]> {
    data.get(at..at.checked_add(len)?)
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(slice(data, at, 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(slice(data, at, 4)?.try_into().ok()?) as usize)
}

/// One text character with the file offset Word uses to look up its properties.
type CharAt = (u16, usize);

/// Main-document text, put together from the piece table.
fn read_text(word: &[u8], clx: &[u8], char_count: usize) -> Option<Vec<CharAt>> {
    let mut at = 0;
    while *clx.get(at)? == 1 {
        at += 3 + usize::from(u16_at(clx, at + 1)?);
    }
    if clx[at] != 2 {
        return None;
    }
    let plc = slice(clx, at + 5, u32_at(clx, at + 1)?)?;
    let pieces = plc.len().checked_sub(4)? / 12;
    let mut text = Vec::new();
    for i in 0..pieces {
        let start = u32_at(plc, i * 4)?;
        let end = u32_at(plc, i * 4 + 4)?.min(char_count);
        let raw_fc = u32_at(plc, (pieces + 1) * 4 + i * 8 + 2)?;
        let fc = raw_fc & 0x3FFF_FFFF;
        for n in 0..end.saturating_sub(start) {
            if raw_fc & 0x4000_0000 != 0 {
                let at = fc / 2 + n;
                let byte = *word.get(at)?;
                let ch = match byte {
                    0x80..=0x9F => CP1252_HIGH[usize::from(byte - 0x80)],
                    _ => u16::from(byte),
                };
                text.push((ch, at));
            } else {
                text.push((u16_at(word, fc + n * 2)?, fc + n * 2));
            }
        }
    }
    Some(text)
}

#[derive(Default)]
struct ParaProps {
    in_table: bool,
    row_end: bool,
    depth: usize,
    /// Cell edges of the row, in twips.
    edges: Vec<i64>,
    /// Per cell: 0 = not merged, 1 = continues the cell above, 3 = first of a merged set.
    vmerge: Vec<u8>,
}

impl ParaProps {
    // shortcut: no horizontal-merge flags (Word 97 era) and no table sprms besides the definition,
    // add them if a real file loses a column span
    fn apply(&mut self, sprms: &[u8], data: &[u8], follow_huge: bool) -> Option<()> {
        let mut at = 0;
        while at + 2 <= sprms.len() {
            let sprm = u16_at(sprms, at)?;
            at += 2;
            let len = match (sprm, sprm >> 13) {
                (0xD608, _) => usize::from(u16_at(sprms, at)?) + 1,
                (_, 0 | 1) => 1,
                (_, 2 | 4 | 5) => 2,
                (_, 3) => 4,
                (_, 7) => 3,
                _ => usize::from(*sprms.get(at)?) + 1,
            };
            let arg = slice(sprms, at, len)?;
            at += len;
            match sprm {
                0x2416 => self.in_table = arg[0] != 0,
                0x2417 => self.row_end = arg[0] != 0,
                0x6649 => self.depth = u32_at(arg, 0)?,
                0x6645 | 0x6646 if follow_huge => {
                    let offset = u32_at(arg, 0)?;
                    let huge = slice(data, offset + 2, usize::from(u16_at(data, offset)?))?;
                    self.apply(huge, data, false)?;
                }
                0xD608 => {
                    let cells = usize::from(*arg.get(2)?);
                    self.edges = (0..=cells)
                        .map(|i| u16_at(arg, 3 + i * 2).map(|v| i64::from(v as i16)))
                        .collect::<Option<_>>()?;
                    let flags_at = 3 + (cells + 1) * 2;
                    self.vmerge = (0..cells)
                        .map(|i| u16_at(arg, flags_at + i * 20).map_or(0, |v| ((v >> 5) & 3) as u8))
                        .collect();
                }
                0xD62B => {
                    if let (Some(&cell), Some(&flag)) = (arg.get(1), arg.get(2))
                        && let Some(slot) = self.vmerge.get_mut(usize::from(cell))
                    {
                        *slot = flag;
                    }
                }
                _ => {}
            }
        }
        Some(())
    }
}

/// Paragraph properties for the paragraph whose end mark sits at file offset `fc`.
fn para_props(word: &[u8], data: &[u8], bte: &[u8], fc: usize) -> Option<ParaProps> {
    let covers = |table: &[u8], i: usize| {
        u32_at(table, i * 4).is_some_and(|lo| lo <= fc)
            && u32_at(table, i * 4 + 4).is_some_and(|hi| fc < hi)
    };
    let pages = bte.len().checked_sub(4)? / 8;
    let page = (0..pages).find(|&i| covers(bte, i))?;
    let number = u32_at(bte, (pages + 1) * 4 + page * 4)? & 0x3F_FFFF;
    let fkp = slice(word, number * 512, 512)?;
    let runs = usize::from(fkp[511]);
    let run = (0..runs).find(|&i| covers(fkp, i))?;
    let mut props = ParaProps::default();
    let mut at = usize::from(*fkp.get((runs + 1) * 4 + run * 13)?) * 2;
    if at == 0 {
        return Some(props);
    }
    let len = match *fkp.get(at)? {
        0 => {
            at += 1;
            usize::from(*fkp.get(at)?) * 2
        }
        half => usize::from(half) * 2 - 1,
    };
    // the first two bytes are the style index
    props.apply(slice(fkp, at + 1, len)?.get(2..)?, data, true)?;
    Some(props)
}

fn hyperlink_target(instruction: &str) -> Option<String> {
    let rest = instruction.trim().strip_prefix("HYPERLINK")?.trim_start();
    let target = match rest.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next()?,
        None => rest.split_whitespace().next()?,
    };
    (!target.is_empty() && !target.starts_with('\\')).then(|| target.to_string())
}

#[derive(Default)]
struct RawCell {
    lines: Vec<String>,
    links: Vec<String>,
}

struct RawRow {
    cells: Vec<RawCell>,
    edges: Vec<i64>,
    vmerge: Vec<u8>,
}

/// Turn cell edges into grid columns, the way Word builds the grid when saving as `.docx`.
fn build_table(mut rows: Vec<RawRow>) -> Table {
    let left = rows.iter().filter_map(|r| r.edges.first().copied()).min().unwrap_or(0);
    for row in &mut rows {
        if let Some(first) = row.edges.first_mut() {
            *first = left;
        }
    }
    let mut all: Vec<i64> = rows.iter().flat_map(|r| r.edges.iter().copied()).collect();
    all.sort_unstable();
    let mut grid: Vec<i64> = Vec::new();
    for edge in all {
        if grid.last().is_none_or(|&last| edge - last > EDGE_TOLERANCE) {
            grid.push(edge);
        }
    }
    let line = |edge: i64| grid.partition_point(|&g| g <= edge) as i64 - 1;
    Table {
        grid_columns: grid.len().saturating_sub(1),
        rows: rows
            .into_iter()
            .map(|row| {
                let cells = row.cells.into_iter().enumerate().filter_map(|(i, cell)| {
                    let column = line(*row.edges.get(i)?);
                    Some(Cell {
                        column,
                        span: (line(*row.edges.get(i + 1)?) - column).max(1),
                        vmerge: match row.vmerge.get(i) {
                            Some(1) => Some("continue".to_string()),
                            Some(2 | 3) => Some("restart".to_string()),
                            _ => None,
                        },
                        lines: cell.lines,
                        links: cell.links,
                    })
                });
                Row { cells: cells.collect() }
            })
            .collect(),
    }
}

// shortcut: hidden text and tracked deletions are read as ordinary text,
// look at character properties if a timetable ever comes with them
fn build_document(text: &[CharAt], props_at: impl Fn(usize) -> Option<ParaProps>) -> Option<Document> {
    let mut doc = Document::default();
    let mut rows: Vec<RawRow> = Vec::new();
    let mut cells: Vec<RawCell> = Vec::new();
    let mut cell = RawCell::default();
    let mut buffer: Vec<u16> = Vec::new();
    let mut links: Vec<String> = Vec::new();
    // open fields: the instruction text, and whether we are still inside it
    let mut fields: Vec<(Vec<u16>, bool)> = Vec::new();

    for &(ch, fc) in text {
        match ch {
            0x13 => fields.push((Vec::new(), true)),
            0x14 | 0x15 => {
                if let Some((instruction, reading)) = fields.last_mut()
                    && std::mem::take(reading)
                {
                    links.extend(hyperlink_target(&String::from_utf16_lossy(instruction)));
                }
                if ch == 0x15 {
                    fields.pop();
                }
            }
            0x07 | 0x0C | 0x0D => {
                let props = props_at(fc)?;
                let para = ParagraphBuilder {
                    buffer: String::from_utf16_lossy(&std::mem::take(&mut buffer)),
                    links: std::mem::take(&mut links),
                    in_text: false,
                };
                let (lines, para_links) = para.finish();
                if !props.in_table {
                    if !rows.is_empty() {
                        doc.blocks.push(Block::Table(build_table(std::mem::take(&mut rows))));
                    }
                    let line = lines.join(" ");
                    if !line.is_empty() {
                        doc.blocks.push(Block::Paragraph(line));
                    }
                    continue;
                }
                // nested tables are skipped, as in the .docx reader
                if props.depth > 1 {
                    continue;
                }
                if props.row_end {
                    rows.push(RawRow {
                        cells: std::mem::take(&mut cells),
                        edges: props.edges,
                        vmerge: props.vmerge,
                    });
                    continue;
                }
                cell.lines.extend(lines);
                cell.links.extend(para_links);
                if ch == 0x07 {
                    cells.push(std::mem::take(&mut cell));
                    fields.clear();
                }
            }
            _ => {
                let out = match ch {
                    0x09 => u16::from(b' '),
                    0x0B | 0x0E => u16::from(b'\n'),
                    0x1E => u16::from(b'-'),
                    0..0x20 => continue,
                    _ => ch,
                };
                match fields.iter_mut().find(|f| f.1) {
                    Some((instruction, _)) => instruction.push(out),
                    None => buffer.push(out),
                }
            }
        }
    }
    if !rows.is_empty() {
        doc.blocks.push(Block::Table(build_table(rows)));
    }
    Some(doc)
}

pub(crate) fn read_doc(file: File) -> Result<Document, DocxError> {
    let mut container = cfb::CompoundFile::open(file)?;
    let word = read_stream(&mut container, "WordDocument")?;
    let flags = u16_at(&word, 0x0A).ok_or(DocxError::Doc("damaged file"))?;
    if u16_at(&word, 0) != Some(0xA5EC) || u16_at(&word, 2).is_none_or(|v| v < 0xC1) {
        return Err(DocxError::Doc("the file is older than Word 97, save it as .docx"));
    }
    if flags & 0x0100 != 0 {
        return Err(DocxError::Doc("the file is password-protected"));
    }
    let table = read_stream(&mut container, if flags & 0x0200 != 0 { "1Table" } else { "0Table" })?;
    // only present when some paragraph properties did not fit next to the text
    let data = read_stream(&mut container, "Data").unwrap_or_default();

    let parse = || {
        let part = |at: usize| slice(&table, u32_at(&word, at)?, u32_at(&word, at + 4)?);
        let bte = part(0x102)?;
        let text = read_text(&word, part(0x1A2)?, u32_at(&word, 0x4C)?)?;
        build_document(&text, |fc| para_props(&word, &data, bte, fc))
    };
    parse().ok_or(DocxError::Doc("damaged file"))
}

fn read_stream<F: Read + Seek>(container: &mut cfb::CompoundFile<F>, name: &str) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    container.open_stream(name)?.read_to_end(&mut bytes)?;
    Ok(bytes)
}
