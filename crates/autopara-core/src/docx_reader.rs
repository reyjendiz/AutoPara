use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;
use thiserror::Error;
use zip::ZipArchive;

const DOCUMENT_PART: &str = "word/document.xml";
const OLE_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const RELS_PART: &str = "word/_rels/document.xml.rels";

#[derive(Debug, Error)]
pub enum DocxError {
    #[error("failed to open file: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid zip archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("invalid xml: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("cannot read .doc: {0}")]
    Doc(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub column: i64,
    pub span: i64,
    /// None | Some("restart") | Some("continue")
    pub vmerge: Option<String>,
    pub lines: Vec<String>,
    pub links: Vec<String>,
}

impl Cell {
    pub fn last_column(&self) -> i64 {
        self.column + self.span - 1
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn overlaps(&self, lo: i64, hi: i64) -> bool {
        !(self.last_column() < lo || self.column > hi)
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Row {
    pub cells: Vec<Cell>,
}

impl Row {
    pub fn at(&self, column: i64) -> Option<&Cell> {
        self.cells
            .iter()
            .find(|c| c.column <= column && column <= c.last_column())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    pub grid_columns: usize,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Block {
    Paragraph(String),
    Table(Table),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Document {
    pub blocks: Vec<Block>,
}

fn local_name(bytes: &[u8]) -> &[u8] {
    match bytes.iter().rposition(|&b| b == b':') {
        Some(pos) => &bytes[pos + 1..],
        None => bytes,
    }
}

fn attr_value(e: &BytesStart<'_>, target_local: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        if local_name(attr.key.as_ref()) == target_local {
            return attr.unescape_value().ok().map(|v| v.into_owned());
        }
    }
    None
}

fn read_relationships(archive: &mut ZipArchive<File>) -> Result<HashMap<String, String>, DocxError> {
    let mut rels = HashMap::new();
    let Ok(mut entry) = archive.by_name(RELS_PART) else {
        return Ok(rels);
    };
    let mut xml = String::new();
    entry.read_to_string(&mut xml)?;

    let mut reader = Reader::from_str(&xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) | Event::Empty(e) if local_name(e.name().as_ref()) == b"Relationship" => {
                let rid = attr_value(&e, b"Id");
                let target = attr_value(&e, b"Target");
                let kind = attr_value(&e, b"Type").unwrap_or_default();
                if let (Some(id), Some(url)) = (rid, target)
                    && kind.contains("hyperlink")
                {
                    rels.insert(id, url);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(rels)
}

#[derive(Default)]
pub(crate) struct ParagraphBuilder {
    pub(crate) buffer: String,
    pub(crate) links: Vec<String>,
    pub(crate) in_text: bool,
}

impl ParagraphBuilder {
    pub(crate) fn finish(self) -> (Vec<String>, Vec<String>) {
        let lines = self
            .buffer
            .split('\n')
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty())
            .collect();
        (lines, self.links)
    }
}

struct CellBuilder {
    column: i64,
    span: i64,
    vmerge: Option<String>,
    lines: Vec<String>,
    links: Vec<String>,
    in_tc_pr: bool,
}

impl CellBuilder {
    fn new(column: i64) -> Self {
        Self {
            column,
            span: 1,
            vmerge: None,
            lines: Vec::new(),
            links: Vec::new(),
            in_tc_pr: false,
        }
    }

    fn finish(self) -> Cell {
        Cell {
            column: self.column,
            span: self.span,
            vmerge: self.vmerge,
            lines: self.lines,
            links: self.links,
        }
    }
}

pub fn read_document(path: impl AsRef<Path>) -> Result<Document, DocxError> {
    let mut file = File::open(path)?;
    // by content, not by extension: renamed files are common
    let mut magic = Vec::new();
    file.by_ref().take(8).read_to_end(&mut magic)?;
    file.rewind()?;
    if magic == OLE_MAGIC {
        return crate::doc_reader::read_doc(file);
    }
    let mut archive = ZipArchive::new(file)?;
    let rels = read_relationships(&mut archive)?;

    let mut xml = String::new();
    archive.by_name(DOCUMENT_PART)?.read_to_string(&mut xml)?;

    let mut reader = Reader::from_str(&xml);
    // Keep exact whitespace inside <w:t xml:space="preserve">!
    reader.config_mut().trim_text(false);

    let mut doc = Document::default();
    let mut buf = Vec::new();
    let mut in_body = false;
    let mut table_depth: usize = 0;
    let mut current_table: Option<Table> = None;
    let mut current_row: Option<Row> = None;
    let mut row_column: i64 = 0;
    let mut current_cell: Option<CellBuilder> = None;
    let mut current_para: Option<ParagraphBuilder> = None;

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => {
                let qname = e.name();
                let name = local_name(qname.as_ref());
                if !in_body {
                    if name == b"body" {
                        in_body = true;
                    }
                    buf.clear();
                    continue;
                }
                match name {
                    b"tbl" => {
                        table_depth += 1;
                        if table_depth == 1 {
                            current_table = Some(Table {
                                grid_columns: 0,
                                rows: Vec::new(),
                            });
                        }
                    }
                    b"gridCol" if table_depth == 1 => {
                        if let Some(tbl) = current_table.as_mut() {
                            tbl.grid_columns += 1;
                        }
                    }
                    b"tr" if table_depth == 1 => {
                        current_row = Some(Row::default());
                        row_column = 0;
                    }
                    b"tc" if table_depth == 1 => {
                        current_cell = Some(CellBuilder::new(row_column));
                    }
                    b"tcPr" if table_depth == 1 => {
                        if let Some(cell) = current_cell.as_mut() {
                            cell.in_tc_pr = true;
                        }
                    }
                    b"gridSpan" if table_depth == 1 => {
                        if let Some(cell) = current_cell.as_mut()
                            && cell.in_tc_pr
                            && let Some(val) = attr_value(&e, b"val")
                            && let Ok(n) = val.parse::<i64>()
                        {
                            cell.span = n.max(1);
                        }
                    }
                    b"vMerge" if table_depth == 1 => {
                        if let Some(cell) = current_cell.as_mut()
                            && cell.in_tc_pr
                        {
                            cell.vmerge = Some(
                                attr_value(&e, b"val")
                                    .filter(|s| !s.is_empty())
                                    .unwrap_or_else(|| "continue".to_string()),
                            );
                        }
                    }
                    b"p" if (table_depth == 0) || (table_depth == 1 && current_cell.is_some()) => {
                        current_para = Some(ParagraphBuilder::default());
                    }
                    b"t" => {
                        if let Some(p) = current_para.as_mut() {
                            p.in_text = true;
                        }
                    }
                    b"br" | b"cr" => {
                        if let Some(p) = current_para.as_mut() {
                            p.buffer.push('\n');
                        }
                    }
                    b"tab" => {
                        if let Some(p) = current_para.as_mut() {
                            p.buffer.push(' ');
                        }
                    }
                    b"hyperlink" => {
                        if let Some(p) = current_para.as_mut()
                            && let Some(rid) = attr_value(&e, b"id")
                            && let Some(target) = rels.get(&rid)
                        {
                            p.links.push(target.clone());
                        }
                    }
                    _ => {}
                }
            }
            Event::Empty(e) => {
                if !in_body {
                    buf.clear();
                    continue;
                }
                let qname = e.name();
                let name = local_name(qname.as_ref());
                match name {
                    b"gridCol" if table_depth == 1 => {
                        if let Some(tbl) = current_table.as_mut() {
                            tbl.grid_columns += 1;
                        }
                    }
                    b"gridSpan" if table_depth == 1 => {
                        if let Some(cell) = current_cell.as_mut()
                            && cell.in_tc_pr
                            && let Some(val) = attr_value(&e, b"val")
                            && let Ok(n) = val.parse::<i64>()
                        {
                            cell.span = n.max(1);
                        }
                    }
                    b"vMerge" if table_depth == 1 => {
                        if let Some(cell) = current_cell.as_mut()
                            && cell.in_tc_pr
                        {
                            cell.vmerge = Some(
                                attr_value(&e, b"val")
                                    .filter(|s| !s.is_empty())
                                    .unwrap_or_else(|| "continue".to_string()),
                            );
                        }
                    }
                    b"br" | b"cr" => {
                        if let Some(p) = current_para.as_mut() {
                            p.buffer.push('\n');
                        }
                    }
                    b"tab" => {
                        if let Some(p) = current_para.as_mut() {
                            p.buffer.push(' ');
                        }
                    }
                    b"hyperlink" => {
                        if let Some(p) = current_para.as_mut()
                            && let Some(rid) = attr_value(&e, b"id")
                            && let Some(target) = rels.get(&rid)
                        {
                            p.links.push(target.clone());
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(e) => {
                if let Some(p) = current_para.as_mut()
                    && p.in_text
                    && let Ok(text) = e.unescape()
                {
                    p.buffer.push_str(&text);
                }
            }
            Event::End(e) => {
                if !in_body {
                    buf.clear();
                    continue;
                }
                let qname = e.name();
                let name = local_name(qname.as_ref());
                match name {
                    b"body" => in_body = false,
                    b"t" => {
                        if let Some(p) = current_para.as_mut() {
                            p.in_text = false;
                        }
                    }
                    b"p" => {
                        if let Some(p) = current_para.take() {
                            let (lines, links) = p.finish();
                            if table_depth == 0 {
                                let text = lines.join(" ").trim().to_string();
                                if !text.is_empty() {
                                    doc.blocks.push(Block::Paragraph(text));
                                }
                            } else if let Some(cell) = current_cell.as_mut() {
                                cell.lines.extend(lines);
                                cell.links.extend(links);
                            }
                        }
                    }
                    b"tcPr" if table_depth == 1 => {
                        if let Some(cell) = current_cell.as_mut() {
                            cell.in_tc_pr = false;
                        }
                    }
                    b"tc" if table_depth == 1 => {
                        if let Some(builder) = current_cell.take() {
                            let cell = builder.finish();
                            row_column += cell.span;
                            if let Some(row) = current_row.as_mut() {
                                row.cells.push(cell);
                            }
                        }
                    }
                    b"tr" if table_depth == 1 => {
                        if let Some(row) = current_row.take()
                            && let Some(tbl) = current_table.as_mut()
                        {
                            tbl.rows.push(row);
                        }
                    }
                    b"tbl" => {
                        if table_depth == 1
                            && let Some(tbl) = current_table.take()
                        {
                            doc.blocks.push(Block::Table(tbl));
                        }
                        table_depth = table_depth.saturating_sub(1);
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(doc)
}
