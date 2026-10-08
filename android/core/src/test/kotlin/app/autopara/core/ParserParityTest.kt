package app.autopara.core

import app.autopara.core.importer.DocxReader
import app.autopara.core.importer.ScheduleParser
import app.autopara.core.model.Provider
import org.json.JSONArray
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotNull

/**
 * The Kotlin parser must agree with the desktop (Python) parser on the same document. The golden
 * file is what `autopara.importer.schedule_parser.parse_file` produced for the fixture, so any
 * drift between the two implementations fails here.
 */
class ParserParityTest {
    private fun resource(name: String) = assertNotNull(javaClass.getResourceAsStream("/$name"), name)

    private val parsed by lazy { ScheduleParser.parse(resource("sample_timetable.docx")) }
    private val golden by lazy { JSONArray(resource("sample_timetable.golden.json").readBytes().toString(Charsets.UTF_8)) }

    @Test
    fun `same courses and lesson counts as the desktop parser`() {
        assertEquals(listOf(12, 3, 2, 3, 0, 2), parsed.map { it.lessons.size })
        assertEquals(22, parsed.sumOf { it.lessons.size })
        assertEquals(golden.length(), parsed.size)
    }

    @Test
    fun `every course matches the golden output field for field`() {
        for (c in 0 until golden.length()) {
            val want = golden.getJSONObject(c)
            val got = parsed[c]
            assertEquals(want.getInt("ordinal"), got.ordinal)
            assertEquals(want.getString("name"), got.name)

            val groups = want.getJSONArray("groups")
            assertEquals(groups.length(), got.groups.size, "groups of course ${got.ordinal}")
            for (g in 0 until groups.length()) {
                val wg = groups.getJSONObject(g)
                assertEquals(wg.getString("name"), got.groups[g].name)
                assertEquals(wg.getString("specialty"), got.groups[g].specialty)
                assertEquals(wg.getInt("lo"), got.groups[g].colLo)
                assertEquals(wg.getInt("hi"), got.groups[g].colHi)
            }

            val lessons = want.getJSONArray("lessons")
            assertEquals(lessons.length(), got.lessons.size, "lessons of course ${got.ordinal}")
            for (l in 0 until lessons.length()) {
                val wl = lessons.getJSONObject(l)
                val gl = got.lessons[l]
                val where = "course ${got.ordinal} lesson $l (${gl.subject})"
                assertEquals(wl.getInt("day"), gl.dayIndex, where)
                assertEquals(wl.getInt("pair"), gl.pair, where)
                assertEquals(wl.getString("start"), gl.startTime, where)
                assertEquals(wl.getString("end"), gl.endTime, where)
                assertEquals(wl.getString("subject"), gl.subject, where)
                assertEquals(wl.getString("teacher"), gl.teacher, where)
                assertEquals(if (wl.isNull("url")) null else wl.getString("url"), gl.url, where)
                assertEquals(
                    when (wl.getString("provider")) {
                        "zoom" -> Provider.ZOOM
                        "google_meet" -> Provider.GOOGLE_MEET
                        else -> Provider.UNKNOWN
                    },
                    gl.provider, where,
                )
                val names = wl.getJSONArray("groups")
                assertEquals((0 until names.length()).map { names.getString(it) }, gl.groupNames, where)
            }
        }
    }

    @Test
    fun `a class shared by two groups is one lesson for both`() {
        val shared = parsed[0].lessons.filter { it.groupNames.size > 1 }
        assertEquals(true, shared.isNotEmpty())
    }

    @Test
    fun `a file that is not a docx is refused with a clear error`() {
        assertFailsWith<DocxReader.InvalidDocumentException> {
            ScheduleParser.parse("this is not a zip archive".byteInputStream())
        }
    }
}
