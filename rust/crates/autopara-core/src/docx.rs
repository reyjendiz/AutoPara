//! The physical layer of the import: a `.docx` (a zip) → paragraphs and tables of cells that carry
//! their logical column, merges and hyperlink targets (PORT_SPEC §A.1, rules R1, R3, R4, R5).
//!
//! No document library: the meaning is in real cell merges (`w:gridSpan`, `w:vMerge`) and in
//! hyperlink relationships, which converters flatten or drop. The XML is read into a small tree and
//! walked with the same rules as the Python `docx_reader` (direct children here, every descendant
//! there), so both produce the same grid.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use quick_xml::NsReader;
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const DOCUMENT_PART: &str = "word/document.xml";
const RELS_PART: &str = "word/_rels/document.xml.rels";

#[derive(Debug, thiserror::Error)]
pub enum DocxError {
    #[error("could not open the document: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a .docx archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("the document's XML is broken: {0}")]
    Xml(String),
}

/// How a cell takes part in a vertical merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VMerge {
    Restart,
    Continue,
}

/// One `<w:tc>`, placed by logical column: the running sum of the spans before it (R1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub column: u32,
    pub span: u32,
    pub vmerge: Option<VMerge>,
    pub lines: Vec<String>,
    pub links: Vec<String>,
}

impl Cell {
    pub fn last_column(&self) -> u32 {
        self.column + self.span - 1
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Whether the cell's columns intersect `lo..=hi` (R2).
    pub fn overlaps(&self, lo: u32, hi: u32) -> bool {
        !(self.last_column() < lo || self.column > hi)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    pub cells: Vec<Cell>,
}

impl Row {
    /// The cell covering `column`, if the row reaches it.
    pub fn at(&self, column: u32) -> Option<&Cell> {
        self.cells
            .iter()
            .find(|cell| cell.column <= column && column <= cell.last_column())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    pub grid_columns: u32,
    pub rows: Vec<Row>,
}

/// The body's paragraphs and tables, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Paragraph(String),
    Table(Table),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub blocks: Vec<Block>,
}

impl Document {
    pub fn tables(&self) -> impl Iterator<Item = &Table> {
        self.blocks.iter().filter_map(|block| match block {
            Block::Table(table) => Some(table),
            Block::Paragraph(_) => None,
        })
    }
}

/// Read a `.docx` file.
pub fn read_document(path: &Path) -> Result<Document, DocxError> {
    read_archive(File::open(path)?)
}

/// Read a `.docx` from anything seekable.
pub fn read_archive(reader: impl Read + Seek) -> Result<Document, DocxError> {
    let mut archive = zip::ZipArchive::new(reader)?;
    let rels = match part(&mut archive, RELS_PART) {
        Ok(xml) => relationships(&parse_xml(&xml)?),
        Err(DocxError::Zip(zip::result::ZipError::FileNotFound)) => HashMap::new(),
        Err(error) => return Err(error),
    };
    let root = parse_xml(&part(&mut archive, DOCUMENT_PART)?)?;

    let mut document = Document::default();
    let Some(body) = root.child(Ns::W, "body") else {
        return Ok(document);
    };
    for child in body.elements() {
        if child.is(Ns::W, "p") {
            let (lines, _) = paragraph(child, &rels);
            let text = lines.join(" ");
            let text = text.trim_matches(crate::text::is_space);
            if !text.is_empty() {
                document.blocks.push(Block::Paragraph(text.to_owned()));
            }
        } else if child.is(Ns::W, "tbl") {
            document.blocks.push(Block::Table(table(child, &rels)));
        }
    }
    Ok(document)
}

fn part(archive: &mut zip::ZipArchive<impl Read + Seek>, name: &str) -> Result<String, DocxError> {
    let mut text = String::new();
    archive.by_name(name)?.read_to_string(&mut text)?;
    Ok(text)
}

/// Relationship id → target, for hyperlink relationships only (R3).
fn relationships(root: &Element) -> HashMap<String, String> {
    root.elements()
        .filter_map(|rel| {
            let id = rel.attr(Ns::None, "Id")?;
            let target = rel.attr(Ns::None, "Target")?;
            let kind = rel.attr(Ns::None, "Type").unwrap_or_default();
            (!id.is_empty() && !target.is_empty() && kind.contains("hyperlink"))
                .then(|| (id.to_owned(), target.to_owned()))
        })
        .collect()
}

/// A paragraph's display lines and hyperlink targets. `w:br`/`w:cr` break lines (R9).
fn paragraph(p: &Element, rels: &HashMap<String, String>) -> (Vec<String>, Vec<String>) {
    let mut buffer = String::new();
    let mut links = Vec::new();
    p.walk(&mut |node| {
        if node.is(Ns::W, "t") {
            buffer.push_str(node.leading_text());
        } else if node.is(Ns::W, "br") || node.is(Ns::W, "cr") {
            buffer.push('\n');
        } else if node.is(Ns::W, "tab") {
            buffer.push(' ');
        } else if node.is(Ns::W, "hyperlink")
            && let Some(target) = node.attr(Ns::R, "id").and_then(|id| rels.get(id))
        {
            links.push(target.clone());
        }
    });
    let lines = buffer
        .split('\n')
        .map(|line| line.trim_matches(crate::text::is_space))
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    (lines, links)
}

fn table(tbl: &Element, rels: &HashMap<String, String>) -> Table {
    let grid_columns = tbl.child(Ns::W, "tblGrid").map_or(0, |grid| {
        grid.elements()
            .filter(|col| col.is(Ns::W, "gridCol"))
            .count() as u32
    });
    let rows = tbl
        .elements()
        .filter(|tr| tr.is(Ns::W, "tr"))
        .map(|tr| {
            let mut column = 0;
            let cells = tr
                .elements()
                .filter(|tc| tc.is(Ns::W, "tc"))
                .map(|tc| {
                    let cell = cell(tc, column, rels);
                    column += cell.span;
                    cell
                })
                .collect();
            Row { cells }
        })
        .collect();
    Table { grid_columns, rows }
}

fn cell(tc: &Element, column: u32, rels: &HashMap<String, String>) -> Cell {
    let props = tc.child(Ns::W, "tcPr");
    let span = props
        .and_then(|props| props.child(Ns::W, "gridSpan"))
        .map_or(1, |span| {
            span.attr(Ns::W, "val").map_or(1, python_int_or_one).max(1)
        });
    let vmerge = props
        .and_then(|props| props.child(Ns::W, "vMerge"))
        .and_then(|merge| {
            // An omitted (or empty) value means "continue", per OOXML. Any other value is neither,
            // and the Python parser then treats the cell as unmerged.
            match merge.attr(Ns::W, "val") {
                Some("restart") => Some(VMerge::Restart),
                None | Some("" | "continue") => Some(VMerge::Continue),
                Some(_) => None,
            }
        });
    let mut lines = Vec::new();
    let mut links = Vec::new();
    for p in tc.elements().filter(|p| p.is(Ns::W, "p")) {
        let (more_lines, more_links) = paragraph(p, rels);
        lines.extend(more_lines);
        links.extend(more_links);
    }
    Cell {
        column,
        span,
        vmerge,
        lines,
        links,
    }
}

/// `int(value or 1)` with Python's leniency about surrounding space and a sign; junk → 1.
fn python_int_or_one(value: &str) -> u32 {
    let text = value.trim_matches(crate::text::is_space);
    if text.is_empty() {
        return 1;
    }
    match text.parse::<i64>() {
        Ok(n) => u32::try_from(n.clamp(1, i64::from(u32::MAX))).unwrap_or(1),
        Err(_) => 1,
    }
}

// ----------------------------------------------------------------- the XML tree

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ns {
    W,
    R,
    Other,
    None,
}

impl Ns {
    fn of(result: &ResolveResult<'_>) -> Self {
        match result {
            ResolveResult::Bound(ns) if ns.as_ref() == W_NS => Ns::W,
            ResolveResult::Bound(ns) if ns.as_ref() == R_NS => Ns::R,
            ResolveResult::Unbound => Ns::None,
            _ => Ns::Other,
        }
    }
}

#[derive(Debug)]
enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug)]
struct Element {
    ns: Ns,
    local: String,
    attrs: Vec<(Ns, String, String)>,
    children: Vec<Node>,
}

impl Element {
    fn is(&self, ns: Ns, local: &str) -> bool {
        self.ns == ns && self.local == local
    }

    fn attr(&self, ns: Ns, local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, l, _)| *n == ns && l == local)
            .map(|(_, _, v)| v.as_str())
    }

    fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|node| match node {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        })
    }

    fn child(&self, ns: Ns, local: &str) -> Option<&Element> {
        self.elements().find(|element| element.is(ns, local))
    }

    /// ElementTree's `.text`: the text before the first child element.
    fn leading_text(&self) -> &str {
        match self.children.first() {
            Some(Node::Text(text)) => text,
            _ => "",
        }
    }

    /// Every element of the subtree, this one first, in document order (ElementTree's `iter`).
    fn walk<'a>(&'a self, visit: &mut impl FnMut(&'a Element)) {
        visit(self);
        for child in self.elements() {
            child.walk(visit);
        }
    }
}

fn xml_error(error: impl std::fmt::Display) -> DocxError {
    DocxError::Xml(error.to_string())
}

/// Parse a whole XML part into a tree, resolving namespaces and entities as ElementTree does.
fn parse_xml(xml: &str) -> Result<Element, DocxError> {
    let mut reader = NsReader::from_str(xml.strip_prefix('\u{feff}').unwrap_or(xml));
    let mut stack: Vec<Element> = vec![Element {
        ns: Ns::None,
        local: String::new(),
        attrs: Vec::new(),
        children: Vec::new(),
    }];
    loop {
        let (resolved, event) = reader.read_resolved_event().map_err(xml_error)?;
        let ns = Ns::of(&resolved);
        let is_empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(start) | Event::Empty(start) => {
                let mut element = Element {
                    ns,
                    local: start.local_name().as_ref().to_owned(),
                    attrs: Vec::new(),
                    children: Vec::new(),
                };
                for attr in start.attributes() {
                    let attr = attr.map_err(xml_error)?;
                    if attr.key.as_namespace_binding().is_some() {
                        continue;
                    }
                    let (attr_ns, local) = reader.resolver().resolve_attribute(attr.key);
                    let value = attr
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(xml_error)?;
                    element.attrs.push((
                        Ns::of(&attr_ns),
                        local.as_ref().to_owned(),
                        value.into_owned(),
                    ));
                }
                if is_empty {
                    push_node(&mut stack, Node::Element(element));
                } else {
                    stack.push(element);
                }
            }
            Event::End(_) => {
                let element = stack.pop().ok_or_else(|| xml_error("unbalanced end tag"))?;
                if stack.is_empty() {
                    return Err(xml_error("unbalanced end tag"));
                }
                push_node(&mut stack, Node::Element(element));
            }
            Event::Text(text) => push_text(&mut stack, &text.xml10_content()),
            Event::CData(data) => push_text(&mut stack, &data.xml10_content()),
            Event::GeneralRef(reference) => {
                let resolved = match reference.resolve_char_ref().map_err(xml_error)? {
                    Some(c) => c.to_string(),
                    None => match reference.xml10_content().as_ref() {
                        "amp" => "&".into(),
                        "lt" => "<".into(),
                        "gt" => ">".into(),
                        "quot" => "\"".into(),
                        "apos" => "'".into(),
                        other => return Err(xml_error(format!("unknown entity &{other};"))),
                    },
                };
                push_text(&mut stack, &resolved);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut document = stack.pop().ok_or_else(|| xml_error("empty document"))?;
    if !stack.is_empty() {
        return Err(xml_error("unclosed element"));
    }
    let mut roots = document.children.drain(..).filter_map(|node| match node {
        Node::Element(element) => Some(element),
        Node::Text(_) => None,
    });
    roots.next().ok_or_else(|| xml_error("no root element"))
}

fn push_node(stack: &mut [Element], node: Node) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    }
}

/// Text is merged with the text right before it, so an entity inside a run stays one string.
fn push_text(stack: &mut [Element], text: &str) {
    if let Some(parent) = stack.last_mut() {
        match parent.children.last_mut() {
            Some(Node::Text(previous)) => previous.push_str(text),
            _ => parent.children.push(Node::Text(text.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

    fn tree(xml: &str) -> Element {
        parse_xml(xml).unwrap()
    }

    #[test]
    fn paragraphs_split_on_breaks_and_resolve_links() {
        let p = tree(&format!(
            r#"<w:p {HEAD}><w:r><w:t>Logic &amp; </w:t><w:t>x</w:t></w:r><w:r><w:br/></w:r><w:r><w:t>(Teacher</w:t><w:tab/><w:t>A.)</w:t></w:r><w:hyperlink r:id="rId1"><w:r><w:t>link</w:t></w:r></w:hyperlink><w:hyperlink r:id="rId9"/></w:p>"#
        ));
        let rels = HashMap::from([("rId1".to_owned(), "https://zoom.us/j/1".to_owned())]);
        let (lines, links) = paragraph(&p, &rels);
        assert_eq!(lines, ["Logic & x", "(Teacher A.)link"]);
        assert_eq!(links, ["https://zoom.us/j/1"]);
    }

    #[test]
    fn cells_carry_span_and_merge() {
        let tc = tree(&format!(
            r#"<w:tc {HEAD}><w:tcPr><w:gridSpan w:val="3"/><w:vMerge/></w:tcPr><w:p><w:r><w:t>A</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>nested</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:tc>"#
        ));
        let cell = cell(&tc, 3, &HashMap::new());
        assert_eq!(
            (cell.column, cell.span, cell.vmerge),
            (3, 3, Some(VMerge::Continue))
        );
        assert_eq!(
            cell.lines,
            ["A"],
            "a nested table is not part of the cell's text"
        );
        assert!(cell.overlaps(5, 7) && !cell.overlaps(6, 7));
    }

    #[test]
    fn bad_spans_count_as_one() {
        assert_eq!(python_int_or_one("x"), 1);
        assert_eq!(python_int_or_one(" 2 "), 2);
        assert_eq!(python_int_or_one("-3"), 1);
        assert_eq!(python_int_or_one(""), 1);
    }

    #[test]
    fn character_references_are_resolved() {
        let t = tree(&format!(r#"<w:t {HEAD}>&#x41;&#66;&lt;</w:t>"#));
        assert_eq!(t.leading_text(), "AB<");
    }
}
