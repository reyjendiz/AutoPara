package app.autopara.core.importer

import app.autopara.core.importer.DocxReader.Block
import app.autopara.core.importer.DocxReader.Cell
import app.autopara.core.importer.DocxReader.Table
import app.autopara.core.importer.Normalize.normalizeText
import app.autopara.core.link.MeetingLink
import app.autopara.core.model.Provider
import java.io.InputStream

/**
 * Domain layer of the import: a docx cell grid -> courses, groups and lessons.
 *
 * A port of `importer/schedule_parser.py`, rule for rule (docs/BACKEND.md section 2). The two that
 * matter most: **R2** a group owns a *range* of logical columns and a lesson cell belongs to every
 * group whose range it overlaps, and **R5/R12** a vertically merged cell -- or the same class
 * retyped in the next slot -- is one block spanning several pairs.
 */
object ScheduleParser {
    /** Columns 0..2 are Day / Pair / Time; group columns start at 3. */
    private const val FIRST_GROUP_COLUMN = 3
    private const val HEADER_ROWS = 2

    class ParsedGroup(val name: String, var specialty: String, val colLo: Int, val colHi: Int)

    class ParsedLesson(
        val dayIndex: Int,
        val pair: Int,
        val startTime: String,
        var endTime: String,
        val subject: String,
        var teacher: String,
        var url: String?,
        var provider: Provider,
        val groupNames: MutableList<String>,
        var linkedCells: Int = 0,
    )

    class ParsedCourse(val ordinal: Int, val name: String) {
        var groups: List<ParsedGroup> = emptyList()
        var lessons: List<ParsedLesson> = emptyList()
    }

    fun parse(input: InputStream, classMinutes: Int = Normalize.DEFAULT_CLASS_MINUTES): List<ParsedCourse> =
        parseDocument(DocxReader.read(input), classMinutes)

    /** One course per table, in document order (R8): the heading text is never trusted. */
    fun parseDocument(
        document: DocxReader.Document,
        classMinutes: Int = Normalize.DEFAULT_CLASS_MINUTES,
    ): List<ParsedCourse> {
        var ordinal = 0
        return document.blocks.filterIsInstance<Block.Tbl>().map { block ->
            ordinal += 1
            parseTable(block.table, ordinal, Normalize.courseLabel(ordinal), classMinutes)
        }
    }

    // ------------------------------------------------------------------ groups

    /** Group column ranges from the two header rows (R2). A header cell may span several columns. */
    private fun headerGroups(table: Table): List<ParsedGroup> {
        if (table.rows.size < HEADER_ROWS) return emptyList()
        val groups = ArrayList<ParsedGroup>()
        for (cell in table.rows[0].cells) {
            if (cell.column < FIRST_GROUP_COLUMN || cell.isEmpty) continue
            groups += ParsedGroup(normalizeText(cell.lines.joinToString(" ")), "", cell.column, cell.lastColumn)
        }
        // Row 1 repeats the specialty under each group column.
        for (group in groups) {
            val cell = table.rows[1].at(group.colLo)
            if (cell != null && !cell.isEmpty) group.specialty = normalizeText(cell.lines.joinToString(" "))
        }
        return groups
    }

    private data class CellContent(val subject: String, val teacher: String, val url: String?)

    /** Split a lesson cell into subject, teacher and url (R9), deduping the doubled link (R3). */
    private fun cellContent(cell: Cell): CellContent {
        val urls = ArrayList(cell.links)
        val textLines = ArrayList<String>()
        for (line in cell.lines) {
            val found = Normalize.findUrls(line)
            if (found.isNotEmpty()) {
                urls += found
                var remainder = line
                for (url in found) remainder = remainder.replace(url, " ")
                remainder = normalizeText(remainder)
                if (remainder.isNotEmpty()) textLines += remainder
            } else {
                textLines += normalizeText(line)
            }
        }
        val lines = textLines.filter { it.isNotEmpty() }
        var subject = ""
        var teacher = ""
        for (line in lines) {
            if (Normalize.isTeacherLine(line)) {
                if (teacher.isEmpty()) teacher = line
            } else if (subject.isEmpty()) {
                subject = line
            }
        }
        if (subject.isEmpty() && lines.isNotEmpty()) subject = lines.first()
        return CellContent(subject, teacher, Normalize.dedupe(urls).firstOrNull())
    }

    private fun groupsFor(cell: Cell, groups: List<ParsedGroup>): MutableList<String> =
        groups.filter { cell.overlaps(it.colLo, it.colHi) }.map { it.name }.toMutableList()

    // ------------------------------------------------------------------ merging

    private data class DedupeKey(val day: Int, val pair: Int, val subject: String, val teacher: String, val url: String)

    /**
     * Defensive fallback for documents whose merges were flattened into repeated text: the same
     * class duplicated across adjacent group columns becomes one lesson for all those groups (R2).
     */
    private fun dedupeAdjacentDuplicates(lessons: List<ParsedLesson>): List<ParsedLesson> {
        val merged = LinkedHashMap<DedupeKey, ParsedLesson>()
        for (lesson in lessons) {
            val key = DedupeKey(
                lesson.dayIndex, lesson.pair,
                normalizeText(lesson.subject).lowercase(), normalizeText(lesson.teacher).lowercase(),
                lesson.url ?: "",
            )
            val existing = merged[key]
            if (existing == null) {
                merged[key] = lesson
            } else {
                existing.linkedCells += lesson.linkedCells
                for (name in lesson.groupNames) if (name !in existing.groupNames) existing.groupNames += name
            }
        }
        return merged.values.toList()
    }

    /** Equal, or one side left blank -- the block then inherits the filled one (R12). */
    private fun compatibleText(left: String, right: String): Boolean {
        val first = normalizeText(left)
        val second = normalizeText(right)
        return first.isEmpty() || second.isEmpty() || first.lowercase() == second.lowercase()
    }

    /** Is [following] the same session as [block], carrying on into the next slot (R12)? */
    private fun continues(block: ParsedLesson, following: ParsedLesson): Boolean {
        if (block.groupNames.sorted() != following.groupNames.sorted()) return false
        if (!compatibleText(block.teacher, following.teacher)) return false
        val blockUrl = block.url
        val followingUrl = following.url
        if (!blockUrl.isNullOrEmpty() && !followingUrl.isNullOrEmpty() && blockUrl != followingUrl) return false
        if (following.startTime < block.endTime) return false
        // Adjacency is a slot rule, never "this subject appears twice today": the same class can
        // run twice with a gap, and those are two lessons. Comparing against the *last* pair the
        // block covers is what lets 2 -> 3 -> 4 chain.
        return following.pair == Normalize.lastPairCovered(block.startTime, block.endTime) + 1 ||
            following.startTime == block.endTime
    }

    private data class BlockKey(val day: Int, val groups: List<String>, val subject: String)

    /** R12: one class written into two adjacent slots is one session, not two. */
    private fun mergeConsecutiveSlots(lessons: List<ParsedLesson>): List<ParsedLesson> {
        val kept = ArrayList<ParsedLesson>()
        val openBlocks = HashMap<BlockKey, ParsedLesson>()
        for (lesson in lessons) {
            val key = BlockKey(lesson.dayIndex, lesson.groupNames.sorted(), normalizeText(lesson.subject).lowercase())
            val block = openBlocks[key]
            if (block != null && continues(block, lesson)) {
                if (lesson.endTime > block.endTime) block.endTime = lesson.endTime
                if (block.teacher.isEmpty()) block.teacher = lesson.teacher
                if (block.url.isNullOrEmpty() && !lesson.url.isNullOrEmpty()) {
                    block.url = lesson.url
                    block.provider = MeetingLink.providerOf(lesson.url)
                }
                block.linkedCells += lesson.linkedCells
                continue
            }
            kept += lesson
            openBlocks[key] = lesson
        }
        return kept
    }

    // ------------------------------------------------------------------ table walk

    private fun parseTable(table: Table, ordinal: Int, name: String, classMinutes: Int): ParsedCourse {
        val course = ParsedCourse(ordinal, name)
        course.groups = headerGroups(table)
        if (course.groups.isEmpty()) return course

        var currentDay: Int? = null
        val openBlocks = HashMap<Int, ParsedLesson>() // column -> block started above (R5)
        val lessons = ArrayList<ParsedLesson>()

        for (row in table.rows.drop(HEADER_ROWS)) {
            val dayCell = row.at(0)
            val pairCell = row.at(1)
            val timeCell = row.at(2)

            // R4: the day is only present on the vMerge 'restart' row; carry it forward.
            if (dayCell != null && !dayCell.isEmpty) {
                Normalize.dayIndex(dayCell.lines[0])?.let { currentDay = it }
            }

            // R11: the time cell may hold a bare start ("8.00") or a range ("8.00-9.20").
            var startTime: String? = null
            var explicitEnd: String? = null
            if (timeCell != null && !timeCell.isEmpty) {
                val (start, end) = Normalize.parseTimeRange(timeCell.lines[0])
                startTime = start
                explicitEnd = end
            }
            val day = currentDay
            if (startTime == null || day == null) continue

            var pair: Int? = null
            if (pairCell != null && !pairCell.isEmpty) {
                val raw = normalizeText(pairCell.lines[0])
                if (raw.isNotEmpty() && raw.all { it.isDigit() }) pair = raw.toIntOrNull()
            }
            // R6: infer the pair from the start time; a foreign timetable falls back to the row
            // the time lands in rather than dropping the row (R10).
            if (pair == null) pair = Normalize.pairFromTime(startTime) ?: Normalize.pairSlot(startTime)

            val rowEnd = explicitEnd ?: Normalize.addMinutes(startTime, classMinutes)

            for (cell in row.cells) {
                if (cell.column < FIRST_GROUP_COLUMN) continue

                if (cell.vmerge == "continue") {
                    // R5: extend the block that started above so it covers this pair too.
                    openBlocks[cell.column]?.endTime = rowEnd
                    continue
                }
                if (cell.isEmpty) {
                    openBlocks.remove(cell.column)
                    continue
                }
                val (subject, teacher, url) = cellContent(cell)
                if (subject.isEmpty()) {
                    openBlocks.remove(cell.column)
                    continue
                }
                val lesson = ParsedLesson(
                    dayIndex = day,
                    pair = pair,
                    startTime = startTime,
                    endTime = rowEnd,
                    subject = subject,
                    teacher = teacher,
                    url = url,
                    provider = MeetingLink.providerOf(url),
                    groupNames = groupsFor(cell, course.groups),
                    linkedCells = if (url != null) 1 else 0,
                )
                lessons += lesson
                if (cell.vmerge == "restart") openBlocks[cell.column] = lesson else openBlocks.remove(cell.column)
            }
        }
        course.lessons = mergeConsecutiveSlots(dedupeAdjacentDuplicates(lessons))
        return course
    }
}
