package app.autopara.core.importer

import org.w3c.dom.Element
import org.w3c.dom.Node
import java.io.ByteArrayInputStream
import java.io.InputStream
import java.util.zip.ZipInputStream
import javax.xml.parsers.DocumentBuilderFactory

/**
 * Physical layer of the .docx import: zip -> XML -> a cell grid with merge metadata.
 *
 * A port of `importer/docx_reader.py`. A .docx is a zip archive; `word/document.xml` carries the
 * content and `word/_rels/document.xml.rels` resolves hyperlink ids into URLs. No docx library is
 * used on purpose: the timetable's meaning is in real cell merges (`gridSpan`, `vMerge`) and in
 * hyperlink relationships, which converters flatten or drop.
 */
object DocxReader {
    private const val W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
    private const val R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    private const val DOCUMENT_PART = "word/document.xml"
    private const val RELS_PART = "word/_rels/document.xml.rels"

    /** A timetable is a few hundred KB; this only stops a hostile archive inflating without bound. */
    private const val MAX_PART_BYTES = 32L * 1024 * 1024

    class InvalidDocumentException(message: String, cause: Throwable? = null) : Exception(message, cause)

    /** One `<w:tc>`, positioned by logical column rather than by index. */
    class Cell(
        val column: Int,
        val span: Int = 1,
        val vmerge: String? = null, // null | "restart" | "continue"
        val lines: List<String> = emptyList(),
        val links: List<String> = emptyList(),
    ) {
        val lastColumn: Int get() = column + span - 1
        val isEmpty: Boolean get() = lines.isEmpty()
        fun overlaps(lo: Int, hi: Int): Boolean = !(lastColumn < lo || column > hi)
    }

    class Row(val cells: List<Cell>) {
        /** The cell covering [column], or null if the row does not reach it. */
        fun at(column: Int): Cell? = cells.firstOrNull { column >= it.column && column <= it.lastColumn }
    }

    class Table(val gridColumns: Int, val rows: List<Row>)

    /** Body content in document order: paragraphs (text) and tables. */
    sealed interface Block {
        data class Paragraph(val text: String) : Block
        data class Tbl(val table: Table) : Block
    }

    class Document(val blocks: List<Block>) {
        val tables: List<Table> get() = blocks.filterIsInstance<Block.Tbl>().map { it.table }
    }

    fun read(input: InputStream): Document {
        val parts = HashMap<String, ByteArray>()
        try {
            ZipInputStream(input).use { zip ->
                while (true) {
                    val entry = zip.nextEntry ?: break
                    if (entry.name == DOCUMENT_PART || entry.name == RELS_PART) {
                        parts[entry.name] = readLimited(zip)
                    }
                }
            }
        } catch (e: java.io.IOException) {
            throw InvalidDocumentException("Not a readable .docx file", e)
        }
        val documentXml = parts[DOCUMENT_PART]
            ?: throw InvalidDocumentException("Not a Word document: word/document.xml is missing")

        val rels = parts[RELS_PART]?.let { readRelationships(parse(it)) } ?: emptyMap()
        val body = parse(documentXml).documentElement.children("body").firstOrNull()
            ?: return Document(emptyList())

        val blocks = ArrayList<Block>()
        for (child in body.childElements()) {
            when {
                child.isW("p") -> {
                    val (lines, _) = paragraphSegments(child, rels)
                    val text = lines.joinToString(" ").trim()
                    if (text.isNotEmpty()) blocks += Block.Paragraph(text)
                }
                child.isW("tbl") -> blocks += Block.Tbl(parseTable(child, rels))
            }
        }
        return Document(blocks)
    }

    private fun readLimited(stream: InputStream): ByteArray {
        val out = java.io.ByteArrayOutputStream()
        val buffer = ByteArray(16 * 1024)
        var total = 0L
        while (true) {
            val n = stream.read(buffer)
            if (n < 0) break
            total += n
            if (total > MAX_PART_BYTES) throw InvalidDocumentException("The document is too large")
            out.write(buffer, 0, n)
        }
        return out.toByteArray()
    }

    private fun parse(bytes: ByteArray): org.w3c.dom.Document {
        val factory = DocumentBuilderFactory.newInstance()
        factory.isNamespaceAware = true
        // The XML comes from a file the user picked: refuse DTDs/entities where the parser lets us.
        // Android's parser does not support every feature flag, so each is best-effort.
        for ((feature, value) in listOf(
            "http://apache.org/xml/features/disallow-doctype-decl" to true,
            "http://xml.org/sax/features/external-general-entities" to false,
            "http://xml.org/sax/features/external-parameter-entities" to false,
        )) {
            try { factory.setFeature(feature, value) } catch (_: Exception) { }
        }
        try { factory.isExpandEntityReferences = false } catch (_: Exception) { }
        try {
            return factory.newDocumentBuilder().parse(ByteArrayInputStream(bytes))
        } catch (e: Exception) {
            throw InvalidDocumentException("The document's XML could not be read", e)
        }
    }

    /** relationship id -> target URL for external hyperlinks. */
    private fun readRelationships(doc: org.w3c.dom.Document): Map<String, String> {
        val rels = HashMap<String, String>()
        for (rel in doc.documentElement.childElements()) {
            val id = rel.getAttribute("Id")
            val target = rel.getAttribute("Target")
            if (id.isNotEmpty() && target.isNotEmpty() && "hyperlink" in rel.getAttribute("Type")) {
                rels[id] = target
            }
        }
        return rels
    }

    /**
     * Split one `<w:p>` into display lines and hyperlink targets. `<w:br/>` is a line break; the
     * schedule stacks subject / teacher / URL inside one cell with it.
     */
    private fun paragraphSegments(paragraph: Element, rels: Map<String, String>): Pair<List<String>, List<String>> {
        val buffer = StringBuilder()
        val links = ArrayList<String>()
        fun walk(node: Element) {
            when {
                node.isW("t") -> buffer.append(node.textContent ?: "")
                node.isW("br") || node.isW("cr") -> buffer.append('\n')
                node.isW("tab") -> buffer.append(' ')
                node.isW("hyperlink") -> {
                    val rid = node.getAttributeNS(R, "id")
                    if (rid.isNotEmpty()) rels[rid]?.let { links += it }
                }
            }
            for (child in node.childElements()) walk(child)
        }
        walk(paragraph)
        val lines = buffer.toString().split('\n').map { it.trim() }.filter { it.isNotEmpty() }
        return lines to links
    }

    private fun parseCell(tc: Element, column: Int, rels: Map<String, String>): Cell {
        var span = 1
        var vmerge: String? = null
        tc.children("tcPr").firstOrNull()?.let { props ->
            props.children("gridSpan").firstOrNull()?.let { gs ->
                span = gs.getAttributeNS(W, "val").toIntOrNull()?.coerceAtLeast(1) ?: 1
            }
            props.children("vMerge").firstOrNull()?.let { merge ->
                // An omitted w:val means "continue" per the OOXML spec.
                vmerge = merge.getAttributeNS(W, "val").ifEmpty { "continue" }
            }
        }
        val lines = ArrayList<String>()
        val links = ArrayList<String>()
        for (paragraph in tc.children("p")) {
            val (paraLines, paraLinks) = paragraphSegments(paragraph, rels)
            lines += paraLines
            links += paraLinks
        }
        return Cell(column, span, vmerge, lines, links)
    }

    private fun parseTable(tbl: Element, rels: Map<String, String>): Table {
        val grid = tbl.children("tblGrid").firstOrNull()
        val gridColumns = grid?.children("gridCol")?.size ?: 0
        val rows = tbl.children("tr").map { tr ->
            var column = 0
            val cells = ArrayList<Cell>()
            for (tc in tr.children("tc")) {
                val cell = parseCell(tc, column, rels)
                cells += cell
                column += cell.span
            }
            Row(cells)
        }
        return Table(gridColumns, rows)
    }

    // ---- small DOM helpers (the DOM API has no convenient typed child iteration)

    private fun Element.isW(local: String) = namespaceURI == W && localName == local

    private fun Element.childElements(): List<Element> {
        val out = ArrayList<Element>()
        var node: Node? = firstChild
        while (node != null) {
            if (node is Element) out += node
            node = node.nextSibling
        }
        return out
    }

    private fun Element.children(local: String): List<Element> = childElements().filter { it.isW(local) }
}
